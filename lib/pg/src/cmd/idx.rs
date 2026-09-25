// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use clap::Args;
use postgres::error::SqlState;

use crate::Target;
use crate::db::{connect, die, err_text, ident, qualified};

#[derive(Args)]
pub struct AddOpts {
    table: String,
    #[arg(required = true)]
    columns: Vec<String>,
    /// Force a GIN index (default: auto-detect from column type)
    #[arg(long, overrides_with = "no_gin")]
    gin: bool,
    /// Never use a GIN index
    #[arg(long, overrides_with = "gin")]
    no_gin: bool,
    /// Create a UNIQUE constraint instead of an index
    #[arg(long)]
    unique: bool,
    /// Custom index/constraint name (default: auto-generated)
    #[arg(short, long)]
    name: Option<String>,
    #[command(flatten)]
    target: Target,
}

pub fn list(table: &str, t: &Target) {
    let schema = &t.schema;
    let mut client = connect(t.database.as_deref());
    let rows = client.query(
        "SELECT indexname::text, indexdef FROM pg_indexes
         WHERE schemaname = $1 AND tablename = $2 ORDER BY indexname",
        &[schema, &table],
    );
    match rows {
        Ok(rows) if rows.is_empty() => eprintln!("No indexes found on {schema}.{table}."),
        Ok(rows) => {
            eprintln!("Indexes on {schema}.{table}:");
            for row in rows {
                eprintln!("  • {}", row.get::<_, String>(0));
                eprintln!("    {}", row.get::<_, String>(1));
            }
        }
        Err(e) => eprintln!("Error: {}", err_text(&e)),
    }
}

pub fn add(o: &AddOpts) {
    let schema = &o.target.schema;
    let table = &o.table;
    let mut client = connect(o.target.database.as_deref());

    // Auto-detect GIN when every column is jsonb, unless forced either way.
    let gin = if o.gin || o.no_gin || o.unique {
        o.gin
    } else {
        let rows = client
            .query(
                "SELECT column_name::text FROM information_schema.columns
                 WHERE table_schema = $1 AND table_name = $2
                   AND column_name::text = ANY($3) AND udt_name::text = 'jsonb'",
                &[schema, table, &o.columns],
            )
            .unwrap_or_else(|e| die(format!("Error: {}", err_text(&e))));
        let jsonb: Vec<String> = rows.iter().map(|r| r.get(0)).collect();
        o.columns.iter().all(|c| jsonb.contains(c))
    };

    let joined = o.columns.join("_");
    let name = o.name.clone().unwrap_or_else(|| match o.unique {
        true => format!("{table}_unique_{joined}"),
        false => format!("idx_{table}_{joined}"),
    });
    let q = qualified(schema, table);
    let cols = o
        .columns
        .iter()
        .map(|c| ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let listed = o.columns.join(", ");

    let result = if o.unique {
        let exists = client
            .query_opt("SELECT 1 FROM pg_constraint WHERE conname = $1", &[&name])
            .unwrap_or_else(|e| die(format!("Error: {}", err_text(&e))));
        if exists.is_some() {
            eprintln!("Unique constraint '{name}' already exists on '{schema}.{table}', skipping.");
            return;
        }
        client
            .batch_execute(&format!(
                "ALTER TABLE {q} ADD CONSTRAINT {} UNIQUE ({cols})",
                ident(&name)
            ))
            .map(|_| {
                format!("Unique constraint '{name}' added to '{schema}.{table}' on ({listed}).")
            })
    } else {
        let using = if gin { "USING gin " } else { "" };
        let kind = if gin { "GIN index" } else { "Index" };
        client
            .batch_execute(&format!(
                "CREATE INDEX IF NOT EXISTS {} ON {q} {using}({cols})",
                ident(&name)
            ))
            .map(|_| format!("{kind} '{name}' created on '{schema}.{table}' ({listed})."))
    };

    match result {
        Ok(msg) => eprintln!("{msg}"),
        Err(e)
            if e.code() == Some(&SqlState::DUPLICATE_OBJECT)
                || e.code() == Some(&SqlState::DUPLICATE_TABLE) =>
        {
            eprintln!("Constraint or index '{name}' already exists, skipping.")
        }
        Err(e) => die(format!("Error: {}", err_text(&e))),
    }
}

pub fn drop(name: &str, t: &Target) {
    let mut client = connect(t.database.as_deref());
    if let Err(e) = client.batch_execute(&format!(
        "DROP INDEX IF EXISTS {}",
        qualified(&t.schema, name)
    )) {
        die(format!("Error: {}", err_text(&e)));
    }
    eprintln!("Index '{name}' dropped.");
}
