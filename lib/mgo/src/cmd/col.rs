// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use mongodb::bson::{Bson, Document, doc};

use crate::db::{self, fail};
use crate::pyfmt::{self, as_f64};

pub fn list(collection: &str, database: &str, sample: i64) {
    let client = db::client();
    let coll = client.database(database).collection::<Document>(collection);
    let pipeline = [
        doc! {"$sample": {"size": sample}},
        doc! {"$project": {"_doc": {"$objectToArray": "$$ROOT"}}},
        doc! {"$unwind": "$_doc"},
        doc! {"$group": {
            "_id": "$_doc.k",
            "types": {"$addToSet": {"$type": "$_doc.v"}},
            "count": {"$sum": 1},
        }},
        doc! {"$sort": {"_id": 1}},
    ];
    let fields: Result<Vec<Document>, _> = coll
        .aggregate(pipeline)
        .run()
        .and_then(|c| c.collect::<Result<Vec<_>, _>>());
    let fields = match fields {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error: {e}");
            return;
        }
    };
    if fields.is_empty() {
        eprintln!("No fields found in {database}.{collection} (collection may be empty).");
        return;
    }
    eprintln!("Fields in {database}.{collection} (sampled {sample} docs):");
    for f in fields {
        let mut types: Vec<String> = f
            .get_array("types")
            .map(|a| a.iter().map(pyfmt::py_str).collect())
            .unwrap_or_default();
        types.sort();
        let count = f.get("count").and_then(as_f64).unwrap_or(0.0);
        let nullable = if count < sample as f64 {
            ", nullable"
        } else {
            ""
        };
        let id = f.get("_id").map(pyfmt::py_str).unwrap_or_default();
        eprintln!("  • {id} ({}{nullable})", types.join(", "));
    }
}

pub fn add(collection: &str, field: &str, default_value: Option<&str>, database: &str) {
    let client = db::client();
    let value = match default_value {
        None => Bson::Null,
        Some(s) => serde_json::from_str(s)
            .map_err(|e| e.to_string())
            .and_then(|v| pyfmt::to_bson(&v))
            .unwrap_or_else(|e| fail(e)),
    };
    let coll = client.database(database).collection::<Document>(collection);
    let r = coll
        .update_many(
            doc! {field: {"$exists": false}},
            doc! {"$set": {field: value}},
        )
        .run()
        .unwrap_or_else(|e| fail(e));
    eprintln!(
        "Field '{field}' added to {} document(s) in '{database}.{collection}'.",
        r.modified_count
    );
}

pub fn drop(collection: &str, field: &str, database: &str) {
    let client = db::client();
    let coll = client.database(database).collection::<Document>(collection);
    let r = coll
        .update_many(
            doc! {field: {"$exists": true}},
            doc! {"$unset": {field: ""}},
        )
        .run()
        .unwrap_or_else(|e| fail(e));
    eprintln!(
        "Field '{field}' removed from {} document(s) in '{database}.{collection}'.",
        r.modified_count
    );
}

pub fn rename(collection: &str, old_name: &str, new_name: &str, database: &str) {
    let client = db::client();
    let coll = client.database(database).collection::<Document>(collection);
    let r = coll
        .update_many(
            doc! {old_name: {"$exists": true}},
            doc! {"$rename": {old_name: new_name}},
        )
        .run()
        .unwrap_or_else(|e| fail(e));
    eprintln!(
        "Field '{old_name}' renamed to '{new_name}' on {} document(s) in '{database}.{collection}'.",
        r.modified_count
    );
}
