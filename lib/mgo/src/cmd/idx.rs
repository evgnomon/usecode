// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use clap::Args;
use mongodb::bson::{Bson, Document, doc};

use crate::config::CFG;
use crate::db::{self, fail};
use crate::pyfmt::{py_str, truthy};

#[derive(Args)]
pub struct AddOpts {
    pub collection: String,
    #[arg(required = true)]
    pub fields: Vec<String>,
    /// Create a unique index
    #[arg(long)]
    pub unique: bool,
    /// Create a sparse index
    #[arg(long)]
    pub sparse: bool,
    /// TTL in seconds (for date fields)
    #[arg(long, allow_negative_numbers = true)]
    pub ttl: Option<i64>,
    /// Field(s) to index in descending order
    #[arg(long = "desc")]
    pub descending: Vec<String>,
    /// Create a text index
    #[arg(long)]
    pub text: bool,
    /// Custom index name (default: auto-generated)
    #[arg(short, long)]
    pub name: Option<String>,
    /// Database name
    #[arg(short, long, default_value = CFG.database.as_str())]
    pub database: String,
}

pub fn list(collection: &str, database: &str) {
    let client = db::client();
    let indexes = match db::list_indexes(&client.database(database), collection) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("Error: {e}");
            return;
        }
    };
    if indexes.is_empty() {
        eprintln!("No indexes found on {database}.{collection}.");
        return;
    }
    eprintln!("Indexes on {database}.{collection}:");
    for info in indexes {
        let keys: Vec<String> = info
            .get_document("key")
            .map(|k| {
                k.iter()
                    .map(|(k, v)| format!("{k}: {}", py_str(v)))
                    .collect()
            })
            .unwrap_or_default();
        let mut flags = Vec::new();
        if info.get("unique").is_some_and(truthy) {
            flags.push("unique".to_string());
        }
        if info.get("sparse").is_some_and(truthy) {
            flags.push("sparse".to_string());
        }
        if let Some(ttl) = info.get("expireAfterSeconds").filter(|v| **v != Bson::Null) {
            flags.push(format!("ttl={}s", py_str(ttl)));
        }
        let flags = if flags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", flags.join(", "))
        };
        let name = info.get("name").map(py_str).unwrap_or_default();
        eprintln!("  • {name}{flags}");
        eprintln!("    keys: {{{}}}", keys.join(", "));
    }
}

fn index_spec(o: &AddOpts) -> Document {
    let mut keys = Document::new();
    for f in &o.fields {
        let dir = if o.text {
            Bson::String("text".into())
        } else if o.descending.contains(f) {
            Bson::Int32(-1)
        } else {
            Bson::Int32(1)
        };
        keys.insert(f.clone(), dir);
    }
    let mut spec = doc! {"key": keys};
    if o.unique {
        spec.insert("unique", true);
    }
    if o.sparse {
        spec.insert("sparse", true);
    }
    if let Some(ttl) = o.ttl {
        spec.insert("expireAfterSeconds", ttl_bson(ttl));
    }
    if let Some(n) = o.name.as_deref().filter(|n| !n.is_empty()) {
        spec.insert("name", n);
    }
    spec
}

fn ttl_bson(ttl: i64) -> Bson {
    i32::try_from(ttl).map_or(Bson::Int64(ttl), Bson::Int32)
}

pub fn add(o: &AddOpts) {
    let client = db::client();
    let d = client.database(&o.database);
    let idx_name = db::create_index(&d, &o.collection, index_spec(o)).unwrap_or_else(|e| fail(e));

    let mut flags = Vec::new();
    if o.unique {
        flags.push("unique".to_string());
    }
    if o.sparse {
        flags.push("sparse".to_string());
    }
    if o.text {
        flags.push("text".to_string());
    }
    if let Some(ttl) = o.ttl {
        flags.push(format!("ttl={ttl}s"));
    }
    let flags = if flags.is_empty() {
        String::new()
    } else {
        format!(" ({})", flags.join(", "))
    };
    eprintln!(
        "Index '{idx_name}'{flags} created on '{}.{}' ({}).",
        o.database,
        o.collection,
        o.fields.join(", ")
    );
}

pub fn drop(collection: &str, name: &str, database: &str) {
    let client = db::client();
    client
        .database(database)
        .collection::<Document>(collection)
        .drop_index(name)
        .run()
        .unwrap_or_else(|e| fail(e));
    eprintln!("Index '{name}' dropped from '{database}.{collection}'.");
}
