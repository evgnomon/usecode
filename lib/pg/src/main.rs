mod cmd;
mod config;
mod db;

use std::io::IsTerminal;
use std::path::PathBuf;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};

use crate::config::CFG;

/// PostgreSQL CLI tool with interactive mode and table commands.
///
/// Settings come from the nearest .pg.json (searched from the current
/// directory upwards); PGHOST, PGPORT, PGUSER, PGPASSWORD and PGDATABASE
/// override them. With no subcommand, SQL piped on stdin is executed.
#[derive(Parser)]
#[command(name = "pg", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start a PostgreSQL instance using podman.
    Kickstart(cmd::kickstart::Opts),
    /// Commands related to tables.
    #[command(subcommand)]
    Tab(TabCommand),
    /// Commands to add, drop, and rename columns on a table.
    #[command(subcommand)]
    Col(ColCommand),
    /// Commands to add, drop, and list indexes on a table.
    #[command(subcommand)]
    Idx(IdxCommand),
    /// Commands to operate on table rows.
    #[command(subcommand)]
    Row(RowCommand),
    /// List all schemas in a database.
    Schema {
        #[arg(default_value = CFG.database.as_str())]
        dbname: String,
    },
    /// Dump SQL DDL statements to recreate database schema and tables (no data).
    Dump {
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
        #[arg(short, long, default_value = "public")]
        schema: String,
    },
    /// Execute a SQL file, directory of SQL files, or SQL from stdin.
    ///
    /// If path is a file, returns the query result array. If path is a
    /// directory, returns JSON with file names as keys and result arrays as
    /// values. If no path is given, reads SQL from stdin.
    Query(QueryOpts),
    /// Commands related to databases.
    #[command(subcommand)]
    Db(DbCommand),
}

/// Target database and schema shared by table-level commands.
#[derive(Args)]
pub struct Target {
    /// Database name (default: from env or config)
    #[arg(short, long)]
    pub database: Option<String>,
    /// Schema name
    #[arg(short, long, default_value = "public")]
    pub schema: String,
}

#[derive(Subcommand)]
enum TabCommand {
    /// List all tables in a schema.
    List {
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
        #[arg(short, long, default_value = "public")]
        schema: String,
    },
    /// Create a new table with an auto-increment bigint primary key.
    Add {
        name: String,
        #[command(flatten)]
        target: Target,
    },
    /// Drop a table.
    Drop {
        name: String,
        #[command(flatten)]
        target: Target,
    },
    /// Rename a table (and its owned sequences).
    Rename {
        old_name: String,
        new_name: String,
        #[command(flatten)]
        target: Target,
    },
    /// Copy a table structure (and optionally data) to a new table.
    Copy {
        source: String,
        destination: String,
        #[command(flatten)]
        target: Target,
        /// Copy data as well (default)
        #[arg(long, overrides_with = "no_data")]
        data: bool,
        /// Copy structure only
        #[arg(long, overrides_with = "data")]
        no_data: bool,
    },
}

#[derive(Subcommand)]
enum ColCommand {
    /// List all columns of a table.
    List {
        table: String,
        #[command(flatten)]
        target: Target,
    },
    /// Add a column to a table. Numbers default to INTEGER (4-byte signed).
    Add(cmd::col::AddOpts),
    /// Drop a column from a table.
    Drop {
        table: String,
        col_name: String,
        #[command(flatten)]
        target: Target,
    },
    /// Rename a column on a table.
    Rename {
        table: String,
        old_name: String,
        new_name: String,
        #[command(flatten)]
        target: Target,
    },
}

#[derive(Subcommand)]
enum IdxCommand {
    /// List all indexes on a table.
    List {
        table: String,
        #[command(flatten)]
        target: Target,
    },
    /// Add an index or unique constraint to a table.
    ///
    /// For JSONB columns, GIN is used automatically unless --no-gin is passed.
    ///
    /// Examples:
    ///
    ///   pg idx add resources kind                     # btree index on kind
    ///
    ///   pg idx add resources kind namespace           # composite btree index
    ///
    ///   pg idx add resources labels                   # GIN index (auto-detected from jsonb)
    ///
    ///   pg idx add resources api_version kind name namespace --unique  # unique constraint
    #[command(verbatim_doc_comment)]
    Add(cmd::idx::AddOpts),
    /// Drop an index by name.
    Drop {
        name: String,
        #[command(flatten)]
        target: Target,
    },
}

#[derive(Subcommand)]
enum RowCommand {
    /// Remove a row by id from a table, or all rows with --all.
    Rm {
        table: String,
        id: Option<i64>,
        /// Delete all rows in the table
        #[arg(long, conflicts_with = "id")]
        all: bool,
        #[command(flatten)]
        target: Target,
    },
    /// List rows of a table as JSON.
    List {
        table: String,
        #[command(flatten)]
        target: Target,
        /// Maximum rows to return
        #[arg(short, long, default_value_t = 100)]
        limit: i64,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Format {
    Json,
    Table,
}

#[derive(Args)]
pub struct QueryOpts {
    /// SQL file or directory of .sql files (default: stdin)
    pub sql_path: Option<PathBuf>,
    /// Database name (default: from env or config)
    #[arg(short, long)]
    pub database: Option<String>,
    /// Output format
    #[arg(short, long, value_enum, default_value_t = Format::Json)]
    pub format: Format,
}

#[derive(Subcommand)]
enum DbCommand {
    /// List all databases in the PostgreSQL server.
    List,
    /// Create a new database.
    Add { name: String },
    /// Drop a database.
    Drop {
        name: String,
        /// Terminate active connections before dropping
        #[arg(long)]
        force: bool,
    },
    /// Save (dump) a database to a gzip-compressed file using pg_dump.
    Save {
        name: String,
        /// Output file path (default: <name>.sql.gz)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Restore a database from a gzip-compressed dump file.
    ///
    /// The database name is inferred from the filename (e.g. hamed.sql.gz ->
    /// hamed), or can be specified with --name.
    Restore {
        input_file: PathBuf,
        /// Database name (default: inferred from filename)
        #[arg(short, long)]
        name: Option<String>,
        /// Create the database before restoring (default)
        #[arg(long, overrides_with = "no_create")]
        create: bool,
        /// Restore into an existing database
        #[arg(long, overrides_with = "create")]
        no_create: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let Some(command) = cli.command else {
        if std::io::stdin().is_terminal() {
            let help = Cli::command().render_help();
            eprintln!("{help}");
            return;
        }
        cmd::query::run(&QueryOpts {
            sql_path: None,
            database: None,
            format: Format::Json,
        });
        return;
    };

    match command {
        Command::Kickstart(opts) => cmd::kickstart::run(&opts),
        Command::Tab(c) => match c {
            TabCommand::List { database, schema } => cmd::tab::list(&database, &schema),
            TabCommand::Add { name, target } => cmd::tab::add(&name, &target),
            TabCommand::Drop { name, target } => cmd::tab::drop(&name, &target),
            TabCommand::Rename {
                old_name,
                new_name,
                target,
            } => cmd::tab::rename(&old_name, &new_name, &target),
            TabCommand::Copy {
                source,
                destination,
                target,
                no_data,
                ..
            } => cmd::tab::copy(&source, &destination, &target, !no_data),
        },
        Command::Col(c) => match c {
            ColCommand::List { table, target } => cmd::col::list(&table, &target),
            ColCommand::Add(opts) => cmd::col::add(&opts),
            ColCommand::Drop {
                table,
                col_name,
                target,
            } => cmd::col::drop(&table, &col_name, &target),
            ColCommand::Rename {
                table,
                old_name,
                new_name,
                target,
            } => cmd::col::rename(&table, &old_name, &new_name, &target),
        },
        Command::Idx(c) => match c {
            IdxCommand::List { table, target } => cmd::idx::list(&table, &target),
            IdxCommand::Add(opts) => cmd::idx::add(&opts),
            IdxCommand::Drop { name, target } => cmd::idx::drop(&name, &target),
        },
        Command::Row(c) => match c {
            RowCommand::Rm {
                table,
                id,
                all,
                target,
            } => cmd::row::rm(&table, id, all, &target),
            RowCommand::List {
                table,
                target,
                limit,
            } => cmd::row::list(&table, &target, limit),
        },
        Command::Schema { dbname } => cmd::schema::run(&dbname),
        Command::Dump { database, schema } => cmd::dump::run(&database, &schema),
        Command::Query(opts) => cmd::query::run(&opts),
        Command::Db(c) => match c {
            DbCommand::List => cmd::database::list(),
            DbCommand::Add { name } => cmd::database::add(&name),
            DbCommand::Drop { name, force } => cmd::database::drop(&name, force),
            DbCommand::Save { name, output } => cmd::database::save(&name, output),
            DbCommand::Restore {
                input_file,
                name,
                no_create,
                ..
            } => cmd::database::restore(&input_file, name, !no_create),
        },
    }
}
