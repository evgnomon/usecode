// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use mongodb::bson::{Bson, Document, doc};
use mongodb::error::Error;

use crate::db::{self, fail};
use crate::pyfmt;

/// Dump collection schemas (indexes, validator rules) as JSON — no data.
pub fn run(database: &str) {
    let client = db::client();
    let d = client.database(database);
    let build = || -> Result<Document, Error> {
        let mut names = d.list_collection_names().run()?;
        names.sort();
        let mut collections = Document::new();
        for name in names {
            let indexes: Vec<Bson> = db::list_indexes(&d, &name)?
                .into_iter()
                .map(|info| {
                    Bson::Document(doc! {
                        "name": info.get("name").cloned().unwrap_or(Bson::Null),
                        "keys": info.get("key").cloned().unwrap_or(Bson::Null),
                        "unique": info.get("unique").cloned().unwrap_or(Bson::Boolean(false)),
                        "sparse": info.get("sparse").cloned().unwrap_or(Bson::Boolean(false)),
                        "expireAfterSeconds": info.get("expireAfterSeconds").cloned().unwrap_or(Bson::Null),
                    })
                })
                .collect();

            let mut validator = Bson::Null;
            let res = d
                .run_command(doc! {"listCollections": 1, "filter": {"name": &name}})
                .run()?;
            let batch = res
                .get_document("cursor")
                .ok()
                .and_then(|c| c.get_array("firstBatch").ok());
            for c in batch.into_iter().flatten() {
                let v = c
                    .as_document()
                    .and_then(|c| c.get_document("options").ok())
                    .and_then(|o| o.get("validator"));
                if let Some(v) = v.filter(|v| pyfmt::truthy(v)) {
                    validator = v.clone();
                }
            }

            collections.insert(name, doc! {"indexes": indexes, "validator": validator});
        }
        Ok(doc! {"database": database, "collections": collections})
    };
    let output = build().unwrap_or_else(|e| fail(e));
    println!(
        "{}",
        pyfmt::dumps(&Bson::Document(output)).unwrap_or_else(|e| fail(e))
    );
}
