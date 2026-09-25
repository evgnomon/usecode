use clap::{Args, ValueEnum};
use postgres::Client;

use crate::Target;
use crate::db::{connect, die, err_text, ident, qualified};

#[derive(Clone, Copy, PartialEq, ValueEnum)]
pub enum ColType {
    String,
    Number,
    Bool,
    Jsonb,
}

#[derive(Args)]
pub struct AddOpts {
    table: String,
    col_name: String,
    #[arg(value_enum)]
    col_type: ColType,
    /// Allow NULL values
    #[arg(long)]
    nullable: bool,
    /// Use 4-byte float (REAL) for number type
    #[arg(long)]
    float: bool,
    /// Use 8-byte float (DOUBLE PRECISION) for number type
    #[arg(long)]
    double: bool,
    /// Use 8-byte integer (BIGINT) for number type
    #[arg(long)]
    long: bool,
    /// Use unsigned (adds CHECK >= 0 constraint)
    #[arg(long)]
    unsigned: bool,
    /// Use fixed-point decimal with N fractional digits (NUMERIC)
    #[arg(long, value_name = "N")]
    fixed: Option<u32>,
    /// Default value (SQL literal or expression)
    #[arg(long, value_name = "EXPR")]
    default: Option<String>,
    #[command(flatten)]
    target: Target,
}

fn pg_type(o: &AddOpts) -> String {
    match o.col_type {
        ColType::String => "TEXT".into(),
        ColType::Bool => "BOOLEAN".into(),
        ColType::Jsonb => "JSONB".into(),
        ColType::Number => match o.fixed {
            Some(n) => format!("NUMERIC(38,{n})"),
            None if o.double => "DOUBLE PRECISION".into(),
            None if o.float => "REAL".into(),
            None if o.long => "BIGINT".into(),
            None => "INTEGER".into(),
        },
    }
}

pub fn list(table: &str, t: &Target) {
    let schema = &t.schema;
    let mut client = connect(t.database.as_deref());
    let rows = client.query(
        "SELECT column_name::text, data_type::text, is_nullable::text, column_default::text
         FROM information_schema.columns
         WHERE table_schema = $1 AND table_name = $2
         ORDER BY ordinal_position",
        &[schema, &table],
    );
    match rows {
        Ok(rows) if rows.is_empty() => eprintln!("No columns found for {schema}.{table}."),
        Ok(rows) => {
            eprintln!("Columns in {schema}.{table}:");
            for row in rows {
                let name: String = row.get(0);
                let data_type: String = row.get(1);
                let nullable = if row.get::<_, String>(2) == "YES" {
                    "nullable"
                } else {
                    "not null"
                };
                let default = row
                    .get::<_, Option<String>>(3)
                    .map(|d| format!(" default={d}"))
                    .unwrap_or_default();
                eprintln!("  • {name} ({data_type}, {nullable}{default})");
            }
        }
        Err(e) => eprintln!("Error: {}", err_text(&e)),
    }
}

pub fn add(o: &AddOpts) {
    let mut client = connect(o.target.database.as_deref());
    let ty = pg_type(o);
    if let Err(e) = try_add(&mut client, o, &ty) {
        die(format!("Error: {}", err_text(&e)));
    }
    let mut label = ty.to_lowercase();
    if o.unsigned {
        label = format!("unsigned {label}");
    }
    if o.nullable {
        label.push_str(" (nullable)");
    }
    eprintln!(
        "Column '{}' ({label}) added to '{}.{}'.",
        o.col_name, o.target.schema, o.table
    );
}

fn try_add(client: &mut Client, o: &AddOpts, ty: &str) -> Result<(), postgres::Error> {
    let schema = &o.target.schema;
    let q = qualified(schema, &o.table);
    let col = ident(&o.col_name);
    let null_clause = if o.nullable { "" } else { " NOT NULL" };
    let default_clause = o
        .default
        .as_deref()
        .filter(|d| !d.is_empty())
        .map(|d| format!(" DEFAULT {d}"))
        .unwrap_or_default();

    let mut tx = client.transaction()?;
    tx.batch_execute(&format!(
        "ALTER TABLE {q} ADD COLUMN IF NOT EXISTS {col} {ty}{null_clause}{default_clause}"
    ))?;
    if o.unsigned && o.col_type == ColType::Number {
        let constraint = format!("{}_unsigned", o.col_name);
        let exists: bool = tx
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM information_schema.table_constraints
                 WHERE table_schema = $1 AND table_name = $2 AND constraint_name = $3)",
                &[schema, &o.table, &constraint],
            )?
            .get(0);
        if !exists {
            tx.batch_execute(&format!(
                "ALTER TABLE {q} ADD CONSTRAINT {} CHECK ({col} >= 0)",
                ident(&constraint)
            ))?;
        }
    }
    tx.commit()
}

pub fn drop(table: &str, col: &str, t: &Target) {
    let mut client = connect(t.database.as_deref());
    let sql = format!(
        "ALTER TABLE {} DROP COLUMN IF EXISTS {}",
        qualified(&t.schema, table),
        ident(col)
    );
    if let Err(e) = client.batch_execute(&sql) {
        die(format!("Error: {}", err_text(&e)));
    }
    eprintln!("Column '{col}' dropped from '{}.{table}'.", t.schema);
}

pub fn rename(table: &str, old: &str, new: &str, t: &Target) {
    let schema = &t.schema;
    let mut client = connect(t.database.as_deref());
    let existing = client
        .query(
            "SELECT column_name::text FROM information_schema.columns
             WHERE table_schema = $1 AND table_name = $2 AND column_name IN ($3, $4)",
            &[schema, &table, &old, &new],
        )
        .unwrap_or_else(|e| die(format!("Error: {}", err_text(&e))));
    let has = |name: &str| existing.iter().any(|r| r.get::<_, String>(0) == name);
    if has(new) && !has(old) {
        eprintln!("Column '{new}' already exists on '{schema}.{table}'. Nothing to do.");
        return;
    }

    let sql = format!(
        "ALTER TABLE {} RENAME COLUMN {} TO {}",
        qualified(schema, table),
        ident(old),
        ident(new)
    );
    if let Err(e) = client.batch_execute(&sql) {
        die(format!("Error: {}", err_text(&e)));
    }
    eprintln!("Column '{old}' renamed to '{new}' on '{schema}.{table}'.");
}
