// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! MongoDB / Azure Cosmos DB (MongoDB API) CLI tool.
//! Mirrors the pg CLI interface (lib/pg), adapted for MongoDB concepts:
//! table → collection, column → field, row → document, SQL → JSON query.

mod cmd;
mod config;
mod db;
mod pyfmt;

use std::io::IsTerminal;
use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};

use crate::config::CFG;

/// MongoDB CLI tool with interactive mode and collection commands.
///
/// Settings come from the nearest .mongo.json (searched from the current
/// directory upwards); MONGO_CONNECTION_STRING, or MONGO_HOST, MONGO_PORT,
/// MONGO_USER and MONGO_PASSWORD override them. With no subcommand, a JSON
/// query piped on stdin is executed.
#[derive(Parser)]
#[command(name = "mgo", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start a MongoDB instance using podman.
    Kickstart(cmd::kickstart::Opts),
    /// Commands related to collections (tables).
    #[command(subcommand)]
    Tab(TabCommand),
    /// Commands to add, drop, and rename fields on a collection.
    #[command(subcommand)]
    Col(ColCommand),
    /// Commands to add, drop, and list indexes on a collection.
    #[command(subcommand)]
    Idx(IdxCommand),
    /// Commands to operate on documents (rows).
    #[command(subcommand)]
    Row(RowCommand),
    /// Execute a JSON query file, directory of query files, or JSON from stdin.
    ///
    /// Query file format (JSON):
    ///   { "find": { "field": "value" } }                         # find query
    ///   { "find": { ... }, "projection": { "field": 1 } }        # with projection
    ///   { "aggregate": [ { "$match": { ... } }, ... ] }           # aggregation pipeline
    ///   { "insert": { "field": "value" } }                        # insert one
    ///   { "insert": [ { ... }, { ... } ] }                        # insert many
    ///   { "update": { "filter": { ... }, "set": { ... } } }      # update
    ///   { "delete": { "field": "value" } }                        # delete
    ///
    /// Collection can be specified in the JSON as "collection" or via --collection.
    #[command(verbatim_doc_comment)]
    Query(QueryOpts),
    /// Dump collection schemas (indexes, validator rules) as JSON — no data.
    Dump {
        /// Database name
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
    },
    /// Commands related to databases.
    #[command(subcommand)]
    Db(DbCommand),
}

#[derive(Subcommand)]
enum TabCommand {
    /// List all collections in a database.
    List {
        /// Database name
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
    },
    /// Create a new collection.
    Add {
        name: String,
        /// Database name
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
    },
    /// Drop a collection.
    Drop {
        name: String,
        /// Database name
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
    },
    /// Rename a collection.
    Rename {
        old_name: String,
        new_name: String,
        /// Database name
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
    },
    /// Copy a collection (structure and optionally data) to a new collection.
    Copy {
        source: String,
        destination: String,
        /// Database name
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
        /// Copy data as well (default)
        #[arg(long, overrides_with = "no_data")]
        data: bool,
        /// Copy structure (indexes) only
        #[arg(long, overrides_with = "data")]
        no_data: bool,
    },
}

#[derive(Subcommand)]
enum ColCommand {
    /// List all fields found in a collection (sampled from documents).
    List {
        collection: String,
        /// Database name
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
        /// Number of documents to sample for field discovery
        #[arg(long, default_value_t = 100, allow_negative_numbers = true)]
        sample: i64,
    },
    /// Add a field to all documents in a collection.
    ///
    /// Sets the field to the given default value (or null) on every document
    /// that doesn't already have it.
    Add {
        collection: String,
        field_name: String,
        /// Default value (JSON literal) to set on all existing documents
        #[arg(long = "default", allow_hyphen_values = true)]
        default_value: Option<String>,
        /// Database name
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
    },
    /// Remove a field from all documents in a collection.
    Drop {
        collection: String,
        field_name: String,
        /// Database name
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
    },
    /// Rename a field on all documents in a collection.
    Rename {
        collection: String,
        old_name: String,
        new_name: String,
        /// Database name
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
    },
}

#[derive(Subcommand)]
enum IdxCommand {
    /// List all indexes on a collection.
    List {
        collection: String,
        /// Database name
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
    },
    /// Add an index to a collection.
    ///
    /// Examples:
    ///
    ///   mongo idx add users email                     # ascending index on email
    ///
    ///   mongo idx add users email username --unique    # unique compound index
    ///
    ///   mongo idx add events created_at --ttl 86400   # TTL index (24h)
    ///
    ///   mongo idx add articles title body --text       # text search index
    ///
    ///   mongo idx add logs timestamp --desc timestamp  # descending index
    #[command(verbatim_doc_comment)]
    Add(cmd::idx::AddOpts),
    /// Drop an index by name.
    Drop {
        collection: String,
        name: String,
        /// Database name
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
    },
}

#[derive(Subcommand)]
enum RowCommand {
    /// List documents in a collection.
    List {
        collection: String,
        /// Database name
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
        /// Maximum documents to return
        #[arg(short, long, default_value_t = 100, allow_negative_numbers = true)]
        limit: i64,
        /// Output format
        #[arg(short, long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
    /// Remove a document by _id, or all documents with --all.
    Rm {
        collection: String,
        id: Option<String>,
        /// Delete all documents in the collection.
        #[arg(long)]
        all: bool,
        /// Database name
        #[arg(short, long, default_value = CFG.database.as_str())]
        database: String,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Json,
    Table,
}

fn existing_path(s: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(s);
    if p.exists() {
        Ok(p)
    } else {
        Err(format!("Path '{s}' does not exist."))
    }
}

#[derive(clap::Args)]
pub struct QueryOpts {
    /// JSON query file or directory of .json files (default: stdin)
    #[arg(value_parser = existing_path)]
    pub query_path: Option<PathBuf>,
    /// Database name
    #[arg(short, long, default_value = CFG.database.as_str())]
    pub database: String,
    /// Collection name (required for find queries)
    #[arg(short, long)]
    pub collection: Option<String>,
    /// Output format
    #[arg(short, long, value_enum, default_value_t = Format::Json)]
    pub format: Format,
}

#[derive(Subcommand)]
enum DbCommand {
    /// List all databases.
    List,
    /// Create a new database (by creating an init collection).
    Add { name: String },
    /// Drop a database.
    Drop { name: String },
    /// Save (dump) a database using mongodump.
    Save {
        name: String,
        /// Output file path (default: <name>.archive.gz)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Restore a database from a mongodump archive.
    Restore {
        #[arg(value_parser = existing_path)]
        input_file: PathBuf,
        /// Database name (default: inferred from filename)
        #[arg(short, long)]
        name: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    let Some(command) = cli.command else {
        if std::io::stdin().is_terminal() {
            eprintln!("{}", Cli::command().render_help());
            return;
        }
        cmd::query::run(&QueryOpts {
            query_path: None,
            database: CFG.database.clone(),
            collection: None,
            format: Format::Json,
        });
        return;
    };

    match command {
        Command::Kickstart(opts) => cmd::kickstart::run(&opts),
        Command::Tab(c) => match c {
            TabCommand::List { database } => cmd::tab::list(&database),
            TabCommand::Add { name, database } => cmd::tab::add(&name, &database),
            TabCommand::Drop { name, database } => cmd::tab::drop(&name, &database),
            TabCommand::Rename {
                old_name,
                new_name,
                database,
            } => cmd::tab::rename(&old_name, &new_name, &database),
            TabCommand::Copy {
                source,
                destination,
                database,
                no_data,
                ..
            } => cmd::tab::copy(&source, &destination, &database, !no_data),
        },
        Command::Col(c) => match c {
            ColCommand::List {
                collection,
                database,
                sample,
            } => cmd::col::list(&collection, &database, sample),
            ColCommand::Add {
                collection,
                field_name,
                default_value,
                database,
            } => cmd::col::add(
                &collection,
                &field_name,
                default_value.as_deref(),
                &database,
            ),
            ColCommand::Drop {
                collection,
                field_name,
                database,
            } => cmd::col::drop(&collection, &field_name, &database),
            ColCommand::Rename {
                collection,
                old_name,
                new_name,
                database,
            } => cmd::col::rename(&collection, &old_name, &new_name, &database),
        },
        Command::Idx(c) => match c {
            IdxCommand::List {
                collection,
                database,
            } => cmd::idx::list(&collection, &database),
            IdxCommand::Add(opts) => cmd::idx::add(&opts),
            IdxCommand::Drop {
                collection,
                name,
                database,
            } => cmd::idx::drop(&collection, &name, &database),
        },
        Command::Row(c) => match c {
            RowCommand::List {
                collection,
                database,
                limit,
                format,
            } => cmd::row::list(&collection, &database, limit, format),
            RowCommand::Rm {
                collection,
                id,
                all,
                database,
            } => cmd::row::rm(&collection, id.as_deref(), all, &database),
        },
        Command::Query(opts) => cmd::query::run(&opts),
        Command::Dump { database } => cmd::dump::run(&database),
        Command::Db(c) => match c {
            DbCommand::List => cmd::database::list(),
            DbCommand::Add { name } => cmd::database::add(&name),
            DbCommand::Drop { name } => cmd::database::drop(&name),
            DbCommand::Save { name, output } => cmd::database::save(&name, output),
            DbCommand::Restore { input_file, name } => cmd::database::restore(&input_file, name),
        },
    }
}
