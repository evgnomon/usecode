// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use std::fmt::Display;
use std::path::PathBuf;
use std::process::exit;

use crate::migrations::{file_name, read_migration};

fn dry_prefix(dry_run: bool) -> &'static str {
    if dry_run { "[DRY RUN] " } else { "" }
}

fn fail(e: impl Display) -> ! {
    println!("  ✗ Error: {e}");
    exit(1);
}

/// Apply migrations using SQLite. Each file runs as a script and is
/// committed on success; the first failure rolls back and exits 1.
pub fn apply_sqlite(db_path: &str, files: &[PathBuf], dry_run: bool) {
    let conn = match rusqlite::Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            exit(1);
        }
    };

    for file in files {
        let sql = read_migration(file).unwrap_or_else(|e| fail(e));
        println!("{}Applying: {}", dry_prefix(dry_run), file_name(file));
        if !dry_run && let Err(e) = conn.execute_batch(&sql) {
            if !conn.is_autocommit() {
                let _ = conn.execute_batch("ROLLBACK");
            }
            fail(sqlite_error(&e));
        }
        println!("  ✓ Success");
    }
}

/// SQLite error text without rusqlite's " in <sql> at offset N" suffix,
/// matching what Python's sqlite3 module reports.
fn sqlite_error(e: &rusqlite::Error) -> String {
    match e {
        rusqlite::Error::SqlInputError { msg, .. } => msg.clone(),
        _ => e.to_string(),
    }
}

pub struct PgParams<'a> {
    pub host: &'a str,
    pub port: u16,
    pub database: &'a str,
    pub user: &'a str,
    pub password: &'a str,
}

fn pg_error(e: &postgres::Error) -> String {
    match e.as_db_error() {
        Some(db) => db.to_string(),
        None => e.to_string(),
    }
}

/// Apply migrations using PostgreSQL, one transaction per file.
pub fn apply_postgres(params: &PgParams, files: &[PathBuf], dry_run: bool) {
    let mut config = postgres::Config::new();
    config
        .host(params.host)
        .port(params.port)
        .dbname(params.database)
        .user(params.user);
    if !params.password.is_empty() {
        config.password(params.password);
    }
    let mut client = match config.connect(postgres::NoTls) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {}", pg_error(&e));
            exit(1);
        }
    };

    for file in files {
        let sql = read_migration(file).unwrap_or_else(|e| fail(e));
        println!("{}Applying: {}", dry_prefix(dry_run), file_name(file));
        if !dry_run {
            let result = client
                .transaction()
                .and_then(|mut tx| tx.batch_execute(&sql).and_then(|_| tx.commit()));
            if let Err(e) = result {
                fail(pg_error(&e));
            }
        }
        println!("  ✓ Success");
    }
}
