// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use mongodb::bson::doc;

use crate::db::{self, fail};
use crate::pyfmt::{as_f64, py_str};

pub fn list() {
    let client = db::client();
    let run = || -> Result<(), mongodb::error::Error> {
        let mut dbs = client.list_database_names().run()?;
        if dbs.is_empty() {
            eprintln!("No databases found.");
            return Ok(());
        }
        eprintln!("Databases:");
        dbs.sort();
        for name in dbs {
            let stats = client
                .database(&name)
                .run_command(doc! {"dbStats": 1})
                .run()?;
            let size_mb = stats.get("dataSize").and_then(as_f64).unwrap_or(0.0) / (1024.0 * 1024.0);
            let collections = stats
                .get("collections")
                .map(py_str)
                .unwrap_or_else(|| "0".into());
            eprintln!("  • {name} ({collections} collections, {size_mb:.2} MB)");
        }
        Ok(())
    };
    if let Err(e) = run() {
        eprintln!("Error: {e}");
    }
}

pub fn add(name: &str) {
    let client = db::client();
    let dbs = client
        .list_database_names()
        .run()
        .unwrap_or_else(|e| fail(e));
    if dbs.iter().any(|d| d == name) {
        eprintln!("Database '{name}' already exists.");
        return;
    }
    // MongoDB creates databases lazily; create a placeholder collection.
    client
        .database(name)
        .create_collection("_init")
        .run()
        .unwrap_or_else(|e| fail(e));
    eprintln!("Database '{name}' created (with placeholder collection '_init').");
}

pub fn drop(name: &str) {
    let client = db::client();
    client
        .database(name)
        .drop()
        .run()
        .unwrap_or_else(|e| fail(e));
    eprintln!("Database '{name}' dropped.");
}
