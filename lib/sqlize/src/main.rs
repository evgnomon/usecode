// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! SQL Migration Runner
//!
//! Applies SQL migration files from a directory in alphabetical order.
//! Supports both 'up' (apply) and 'down' (rollback) migrations.

mod backends;
mod migrations;

use std::process::exit;

use clap::{CommandFactory, Parser};

const EXAMPLES: &str = "\
Examples:
  # List migrations without executing
  sqlize ./db/templates --print-only

  # Apply 'up' migrations to SQLite database
  sqlize ./db/templates --sqlite ./database.db

  # Apply 'down' migrations (rollback) to SQLite
  sqlize ./db/templates --sqlite ./database.db --direction down

  # Apply to PostgreSQL
  sqlize ./db/templates --postgres --host localhost --database mydb --user admin --password secret

  # Dry run (show what would be executed)
  sqlize ./db/templates --sqlite ./database.db --dry-run";

/// Apply SQL migrations from a directory in alphabetical order
#[derive(Parser)]
#[command(name = "sqlize", after_help = EXAMPLES)]
struct Cli {
    /// Directory containing SQL migration files
    directory: String,

    /// Migration direction: 'up' to apply, 'down' to rollback (default: up)
    #[arg(long, default_value = "up", value_parser = ["up", "down"])]
    direction: String,

    /// Print migration files and their content without executing
    #[arg(long)]
    print_only: bool,

    /// Show what would be executed without making changes
    #[arg(long)]
    dry_run: bool,

    /// Path to SQLite database file
    #[arg(long, value_name = "DB_PATH")]
    sqlite: Option<String>,

    /// Use PostgreSQL database
    #[arg(long)]
    postgres: bool,

    /// PostgreSQL host (default: localhost)
    #[arg(long, default_value = "localhost")]
    host: String,

    /// PostgreSQL port (default: 5432)
    #[arg(long, default_value_t = 5432)]
    port: u16,

    /// PostgreSQL database name
    #[arg(long)]
    database: Option<String>,

    /// PostgreSQL username
    #[arg(long)]
    user: Option<String>,

    /// PostgreSQL password
    #[arg(long, default_value = "")]
    password: String,
}

fn main() {
    let cli = Cli::parse();

    let files = migrations::get_migration_files(&cli.directory, &cli.direction);
    if files.is_empty() {
        println!(
            "No *_{}.sql files found in '{}'",
            cli.direction, cli.directory
        );
        exit(0);
    }

    println!("Found {} migration(s) ({})", files.len(), cli.direction);

    if cli.print_only {
        migrations::print_only(&files);
    } else if let Some(db_path) = cli.sqlite.as_deref().filter(|p| !p.is_empty()) {
        backends::apply_sqlite(db_path, &files, cli.dry_run);
    } else if cli.postgres {
        let (Some(database), Some(user)) = (
            cli.database.as_deref().filter(|s| !s.is_empty()),
            cli.user.as_deref().filter(|s| !s.is_empty()),
        ) else {
            println!("Error: --database and --user are required for PostgreSQL");
            exit(1);
        };
        let pg = backends::PgParams {
            host: &cli.host,
            port: cli.port,
            database,
            user,
            password: &cli.password,
        };
        backends::apply_postgres(&pg, &files, cli.dry_run);
    } else {
        println!("No database specified. Use --print-only to preview, --sqlite, or --postgres");
        let _ = Cli::command().print_help();
        exit(1);
    }

    println!("\nDone!");
}
