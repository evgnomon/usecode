// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use mongodb::bson::{Document, doc};
use mongodb::error::Error;

use crate::db::{self, fail};
use crate::pyfmt::truthy;

pub fn list(database: &str) {
    let client = db::client();
    match client.database(database).list_collection_names().run() {
        Ok(mut names) => {
            names.sort();
            if names.is_empty() {
                eprintln!("No collections found in {database}.");
            } else {
                eprintln!("Collections in {database}:");
                for n in names {
                    eprintln!("  • {n}");
                }
            }
        }
        Err(e) => eprintln!("Error: {e}"),
    }
}

pub fn add(name: &str, database: &str) {
    let client = db::client();
    let d = client.database(database);
    let names = d.list_collection_names().run().unwrap_or_else(|e| fail(e));
    if names.iter().any(|n| n == name) {
        eprintln!("Collection '{name}' already exists in {database}.");
        return;
    }
    d.create_collection(name).run().unwrap_or_else(|e| fail(e));
    eprintln!("Collection '{name}' created in {database}.");
}

pub fn drop(name: &str, database: &str) {
    let client = db::client();
    client
        .database(database)
        .collection::<Document>(name)
        .drop()
        .run()
        .unwrap_or_else(|e| fail(e));
    eprintln!("Collection '{name}' dropped from {database}.");
}

pub fn rename(old_name: &str, new_name: &str, database: &str) {
    let client = db::client();
    let d = client.database(database);
    let names = d.list_collection_names().run().unwrap_or_else(|e| fail(e));
    let has = |n: &str| names.iter().any(|c| c == n);
    if has(new_name) && !has(old_name) {
        eprintln!("Collection '{new_name}' already exists. Nothing to do.");
        return;
    }
    client
        .database("admin")
        .run_command(doc! {
            "renameCollection": format!("{database}.{old_name}"),
            "to": format!("{database}.{new_name}"),
        })
        .run()
        .unwrap_or_else(|e| fail(e));
    eprintln!("Collection '{old_name}' renamed to '{new_name}' in {database}.");
}

pub fn copy(source: &str, destination: &str, database: &str, data: bool) {
    let client = db::client();
    let d = client.database(database);
    let run = || -> Result<bool, Error> {
        if d.list_collection_names()
            .run()?
            .iter()
            .any(|n| n == destination)
        {
            return Ok(false);
        }
        if data {
            d.collection::<Document>(source)
                .aggregate([doc! {"$out": destination}])
                .run()?;
        } else {
            d.create_collection(destination).run()?;
        }
        // Copy indexes (excluding _id which is auto-created).
        for info in db::list_indexes(&d, source)? {
            if info.get_str("name").ok() == Some("_id_") {
                continue;
            }
            let mut spec = doc! {"key": info.get_document("key").cloned().unwrap_or_default()};
            if info.get("unique").is_some_and(truthy) {
                spec.insert("unique", true);
            }
            if info.get("sparse").is_some_and(truthy) {
                spec.insert("sparse", true);
            }
            if let Some(ttl) = info.get("expireAfterSeconds") {
                spec.insert("expireAfterSeconds", ttl.clone());
            }
            if let Some(n) = info.get("name") {
                spec.insert("name", n.clone());
            }
            db::create_index(&d, destination, spec)?;
        }
        Ok(true)
    };
    match run() {
        Ok(false) => {
            eprintln!("Collection '{destination}' already exists in {database}. Nothing to do.")
        }
        Ok(true) => {
            let label = if data { "with data" } else { "structure only" };
            eprintln!("Collection '{source}' copied to '{destination}' ({label}) in {database}.");
        }
        Err(e) => fail(e),
    }
}
