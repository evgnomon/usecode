// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use std::process::exit;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, Document, doc};

use crate::Format;
use crate::db::{self, fail};
use crate::pyfmt;

/// Print documents as JSON or as a table (`No documents returned.` if empty).
pub fn print_docs(docs: Vec<Document>, format: Format) {
    if format == Format::Json {
        let arr = Bson::Array(docs.into_iter().map(Bson::Document).collect());
        println!("{}", pyfmt::dumps(&arr).unwrap_or_else(|e| fail(e)));
        return;
    }
    match pyfmt::table(&docs) {
        Some(t) => println!("{t}"),
        None => eprintln!("No documents returned."),
    }
}

pub fn list(collection: &str, database: &str, limit: i64, format: Format) {
    let client = db::client();
    let coll = client.database(database).collection::<Document>(collection);
    let docs = coll
        .find(doc! {})
        .limit(limit)
        .run()
        .and_then(|c| c.collect::<Result<Vec<_>, _>>())
        .unwrap_or_else(|e| fail(e));
    print_docs(docs, format);
}

pub fn rm(collection: &str, id: Option<&str>, all: bool, database: &str) {
    if id.is_none() && !all {
        eprintln!("Error: provide an _id or use --all to delete all documents.");
        exit(1);
    }
    if id.is_some() && all {
        eprintln!("Error: cannot specify both an _id and --all.");
        exit(1);
    }
    let client = db::client();
    let coll = client.database(database).collection::<Document>(collection);
    match id {
        None => {
            let r = coll.delete_many(doc! {}).run().unwrap_or_else(|e| fail(e));
            eprintln!(
                "Deleted {} document(s) from '{database}.{collection}'.",
                r.deleted_count
            );
        }
        Some(id) => {
            // Try as ObjectId first, fall back to string _id.
            let doc_id = ObjectId::parse_str(id)
                .map(Bson::ObjectId)
                .unwrap_or_else(|_| Bson::String(id.to_string()));
            let r = coll
                .delete_one(doc! {"_id": doc_id})
                .run()
                .unwrap_or_else(|e| fail(e));
            eprintln!(
                "Deleted {} document(s) from '{database}.{collection}' where _id={id}.",
                r.deleted_count
            );
        }
    }
}
