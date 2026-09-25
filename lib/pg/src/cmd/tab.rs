// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use postgres::Client;

use crate::Target;
use crate::db::{connect, die, err_text, ident, qualified, table_exists};

pub fn list(database: &str, schema: &str) {
    let mut client = connect(Some(database));
    let rows = client.query(
        "SELECT tablename::text FROM pg_tables WHERE schemaname = $1 ORDER BY tablename",
        &[&schema],
    );
    match rows {
        Ok(rows) if rows.is_empty() => eprintln!("No tables found in {database}.{schema}."),
        Ok(rows) => {
            eprintln!("Tables in {database}.{schema}:");
            for row in rows {
                eprintln!("  • {}", row.get::<_, String>(0));
            }
        }
        Err(e) => eprintln!("Error: {}", err_text(&e)),
    }
}

/// Run a single DDL statement, exiting on error.
fn ddl(target: &Target, sql: &str) {
    let mut client = connect(target.database.as_deref());
    if let Err(e) = client.batch_execute(sql) {
        die(format!("Error: {}", err_text(&e)));
    }
}

pub fn add(name: &str, t: &Target) {
    let q = qualified(&t.schema, name);
    ddl(
        t,
        &format!("CREATE TABLE IF NOT EXISTS {q} (id BIGSERIAL PRIMARY KEY)"),
    );
    eprintln!("Table '{}.{name}' created.", t.schema);
}

pub fn drop(name: &str, t: &Target) {
    ddl(
        t,
        &format!("DROP TABLE IF EXISTS {}", qualified(&t.schema, name)),
    );
    eprintln!("Table '{}.{name}' dropped.", t.schema);
}

pub fn rename(old: &str, new: &str, t: &Target) {
    let mut client = connect(t.database.as_deref());
    if let Err(e) = try_rename(&mut client, old, new, &t.schema) {
        die(format!("Error: {}", err_text(&e)));
    }
}

fn try_rename(
    client: &mut Client,
    old: &str,
    new: &str,
    schema: &str,
) -> Result<(), postgres::Error> {
    if table_exists(client, schema, new)? && !table_exists(client, schema, old)? {
        eprintln!("Table '{schema}.{new}' already exists. Nothing to do.");
        return Ok(());
    }

    let mut tx = client.transaction()?;
    tx.batch_execute(&format!(
        "ALTER TABLE {} RENAME TO {}",
        qualified(schema, old),
        ident(new)
    ))?;

    // Rename owned sequences, e.g. old_id_seq -> new_id_seq.
    let seqs = tx.query(
        "SELECT s.relname::text
         FROM pg_class s
         JOIN pg_namespace n ON n.oid = s.relnamespace
         JOIN pg_depend d ON d.objid = s.oid
         JOIN pg_class t ON t.oid = d.refobjid
         WHERE s.relkind = 'S' AND n.nspname = $1 AND t.relname = $2 AND d.deptype = 'a'",
        &[&schema, &new],
    )?;
    let prefix = format!("{old}_");
    for row in seqs {
        let old_seq: String = row.get(0);
        let Some(suffix) = old_seq.strip_prefix(&prefix) else {
            continue;
        };
        let new_seq = format!("{new}_{suffix}");
        tx.batch_execute(&format!(
            "ALTER SEQUENCE {} RENAME TO {}",
            qualified(schema, &old_seq),
            ident(&new_seq)
        ))?;
        eprintln!("Sequence '{old_seq}' renamed to '{new_seq}'.");
    }
    tx.commit()?;
    eprintln!("Table '{schema}.{old}' renamed to '{schema}.{new}'.");
    Ok(())
}

pub fn copy(src: &str, dst: &str, t: &Target, data: bool) {
    let schema = &t.schema;
    let mut client = connect(t.database.as_deref());
    match table_exists(&mut client, schema, dst) {
        Ok(true) => {
            eprintln!("Table '{schema}.{dst}' already exists. Nothing to do.");
            return;
        }
        Ok(false) => {}
        Err(e) => die(format!("Error: {}", err_text(&e))),
    }

    let suffix = if data { "" } else { " WITH NO DATA" };
    let sql = format!(
        "CREATE TABLE {} AS TABLE {}{suffix}",
        qualified(schema, dst),
        qualified(schema, src)
    );
    if let Err(e) = client.batch_execute(&sql) {
        die(format!("Error: {}", err_text(&e)));
    }
    let label = if data { "with data" } else { "structure only" };
    eprintln!("Table '{schema}.{src}' copied to '{schema}.{dst}' ({label}).");
}
