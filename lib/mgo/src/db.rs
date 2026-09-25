// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use std::process::exit;
use std::time::Duration;

use mongodb::bson::{Bson, Document, doc};
use mongodb::error::{Error, ErrorKind};
use mongodb::options::ClientOptions;
use mongodb::sync::{Client, Database};

use crate::config;

/// Create a client and verify the server answers a ping, or exit.
pub fn client() -> Client {
    let cs = config::connection_string();
    let connect = || -> Result<Client, Error> {
        let mut opts = ClientOptions::parse(&cs).run()?;
        opts.server_selection_timeout = Some(Duration::from_secs(5));
        let client = Client::with_options(opts)?;
        client
            .database("admin")
            .run_command(doc! {"ping": 1})
            .run()?;
        Ok(client)
    };
    connect().unwrap_or_else(|e| {
        eprintln!("Connection failed: {e}");
        exit(1)
    })
}

/// Print `Error: {e}` to stderr and exit 1.
pub fn fail(e: impl std::fmt::Display) -> ! {
    eprintln!("Error: {e}");
    exit(1)
}

fn is_ns_not_found(e: &Error) -> bool {
    matches!(&*e.kind, ErrorKind::Command(c) if c.code == 26)
}

/// Raw index specs as returned by `listIndexes` (empty if the collection is
/// missing, like pymongo).
pub fn list_indexes(db: &Database, coll: &str) -> Result<Vec<Document>, Error> {
    let res = match db.run_command(doc! {"listIndexes": coll}).run() {
        Ok(r) => r,
        Err(e) if is_ns_not_found(&e) => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let batch = res
        .get_document("cursor")
        .ok()
        .and_then(|c| c.get_array("firstBatch").ok())
        .cloned()
        .unwrap_or_default();
    Ok(batch
        .into_iter()
        .filter_map(|b| match b {
            Bson::Document(d) => Some(d),
            _ => None,
        })
        .collect())
}

/// Index name as pymongo generates it: `field_dir` pairs joined by `_`.
pub fn gen_index_name(keys: &Document) -> String {
    keys.iter()
        .map(|(k, v)| format!("{k}_{}", crate::pyfmt::py_str(v)))
        .collect::<Vec<_>>()
        .join("_")
}

/// Create an index from a raw spec (`key` plus options) and return its name.
pub fn create_index(db: &Database, coll: &str, mut spec: Document) -> Result<String, Error> {
    if !spec.contains_key("name") {
        let keys = spec.get_document("key").cloned().unwrap_or_default();
        spec.insert("name", gen_index_name(&keys));
    }
    let name = spec.get_str("name").unwrap_or_default().to_string();
    db.run_command(doc! {"createIndexes": coll, "indexes": [spec]})
        .run()?;
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_names() {
        assert_eq!(
            gen_index_name(&doc! {"email": 1, "ts": -1, "body": "text"}),
            "email_1_ts_-1_body_text"
        );
    }
}
