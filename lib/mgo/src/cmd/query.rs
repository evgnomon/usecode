// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use std::fs;
use std::io::{IsTerminal, Read};
use std::path::Path;
use std::process::exit;

use mongodb::bson::{Bson, Document, doc};
use mongodb::sync::{Collection, Database};
use serde_json::Value;

use crate::QueryOpts;
use crate::cmd::row::print_docs;
use crate::db::{self, fail};
use crate::pyfmt::{self, py_str, to_doc, truthy};

/// Outcome of a single query: documents, or a status/error object.
enum Outcome {
    Docs(Vec<Document>),
    Object(Document),
}

fn object_error(msg: &str) -> Outcome {
    Outcome::Object(doc! {"error": msg})
}

fn collect(cursor: mongodb::sync::Cursor<Document>) -> Result<Vec<Document>, String> {
    cursor
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn execute(db: &Database, mut q: Document, default_coll: Option<&str>) -> Result<Outcome, String> {
    let named = q.remove("collection").filter(truthy).map(|v| py_str(&v));
    let Some(col_name) = named.or_else(|| default_coll.map(str::to_string)) else {
        return Ok(object_error(
            "No collection specified. Use --collection or include 'collection' in the query.",
        ));
    };
    let col: Collection<Document> = db.collection(&col_name);
    let err = |e: mongodb::error::Error| e.to_string();

    if let Some(filter) = q.get("find") {
        let filter = as_doc(filter, "filter")?;
        let mut find = col.find(filter);
        match q.get("projection") {
            None | Some(Bson::Null) => {}
            Some(Bson::Array(fields)) => {
                let mut p = Document::new();
                for f in fields {
                    p.insert(py_str(f), 1);
                }
                find = find.projection(p);
            }
            Some(p) => find = find.projection(as_doc(p, "projection")?),
        }
        if let Some(sort) = q.get("sort").filter(|s| truthy(s)) {
            find = find.sort(as_doc(sort, "sort")?);
        }
        if let Some(limit) = q.get("limit").filter(|l| truthy(l)) {
            let n = match limit {
                Bson::Int32(i) => i64::from(*i),
                Bson::Int64(i) => *i,
                Bson::Boolean(_) => 1,
                _ => return Err("limit must be an integer".into()),
            };
            find = find.limit(n);
        }
        return Ok(Outcome::Docs(collect(find.run().map_err(err)?)?));
    }

    if let Some(pipeline) = q.get("aggregate") {
        let Bson::Array(stages) = pipeline else {
            return Err("pipeline must be a list".into());
        };
        let stages = stages
            .iter()
            .map(|s| as_doc(s, "pipeline stage"))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Outcome::Docs(collect(
            col.aggregate(stages).run().map_err(err)?,
        )?));
    }

    if let Some(data) = q.get("insert") {
        if let Bson::Array(items) = data {
            let docs = items
                .iter()
                .map(|d| as_doc(d, "document"))
                .collect::<Result<Vec<_>, _>>()?;
            let r = col.insert_many(docs).run().map_err(err)?;
            let n = i64::try_from(r.inserted_ids.len()).unwrap_or(i64::MAX);
            return Ok(Outcome::Object(
                doc! {"status": "success", "inserted_count": count(n)},
            ));
        }
        let r = col
            .insert_one(as_doc(data, "document")?)
            .run()
            .map_err(err)?;
        return Ok(Outcome::Object(
            doc! {"status": "success", "inserted_id": py_str(&r.inserted_id)},
        ));
    }

    if let Some(spec) = q.get("update") {
        let spec = as_doc(spec, "update")?;
        let filter = match spec.get("filter") {
            None => Document::new(),
            Some(f) => as_doc(f, "filter")?,
        };
        let mut update = Document::new();
        if let Some(set) = spec.get("set").filter(|s| truthy(s)) {
            update.insert("$set", set.clone());
        }
        if let Some(unset) = spec.get("unset").filter(|s| truthy(s)) {
            update.insert("$unset", unset_doc(unset));
        }
        if update.is_empty() {
            return Ok(object_error("Update requires 'set' and/or 'unset' fields."));
        }
        let many = spec.get("many").is_none_or(truthy);
        let r = if many {
            col.update_many(filter, update).run()
        } else {
            col.update_one(filter, update).run()
        }
        .map_err(err)?;
        return Ok(Outcome::Object(doc! {
            "status": "success",
            "matched_count": count(i64::try_from(r.matched_count).unwrap_or(i64::MAX)),
            "modified_count": count(i64::try_from(r.modified_count).unwrap_or(i64::MAX)),
        }));
    }

    if let Some(filter) = q.get("delete") {
        let r = col
            .delete_many(as_doc(filter, "filter")?)
            .run()
            .map_err(err)?;
        let n = i64::try_from(r.deleted_count).unwrap_or(i64::MAX);
        return Ok(Outcome::Object(
            doc! {"status": "success", "deleted_count": count(n)},
        ));
    }

    Ok(object_error(
        "Unknown query type. Use 'find', 'aggregate', 'insert', 'update', or 'delete'.",
    ))
}

fn count(n: i64) -> Bson {
    Bson::Int64(n)
}

fn as_doc(v: &Bson, what: &str) -> Result<Document, String> {
    match v {
        Bson::Document(d) => Ok(d.clone()),
        _ => Err(format!("{what} must be an instance of dict")),
    }
}

/// `{k: "" for k in unset_fields}` over a dict, list or string.
fn unset_doc(v: &Bson) -> Document {
    let mut d = Document::new();
    match v {
        Bson::Document(m) => m.keys().for_each(|k| {
            d.insert(k.clone(), "");
        }),
        Bson::Array(a) => a.iter().for_each(|k| {
            d.insert(py_str(k), "");
        }),
        Bson::String(s) => s.chars().for_each(|c| {
            d.insert(c.to_string(), "");
        }),
        other => {
            d.insert(py_str(other), "");
        }
    }
    d
}

fn parse(text: &str) -> Result<Document, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    to_doc(&v, "query")
}

fn outcome_json(o: Outcome) -> Bson {
    match o {
        Outcome::Docs(d) => Bson::Array(d.into_iter().map(Bson::Document).collect()),
        Outcome::Object(d) => Bson::Document(d),
    }
}

fn run_dir(db: &Database, dir: &Path, coll: Option<&str>) {
    let mut files: Vec<String> = fs::read_dir(dir)
        .unwrap_or_else(|e| fail(e))
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    files.sort();
    let mut results = Document::new();
    for name in files {
        let text = fs::read_to_string(dir.join(&name)).unwrap_or_else(|e| fail(e));
        let r = parse(&text)
            .and_then(|q| execute(db, q, coll))
            .map(outcome_json)
            .unwrap_or_else(|e| Bson::Document(doc! {"error": e}));
        results.insert(name, r);
    }
    println!(
        "{}",
        pyfmt::dumps(&Bson::Document(results)).unwrap_or_else(|e| fail(e))
    );
}

pub fn run(o: &QueryOpts) {
    if o.query_path.is_none() && std::io::stdin().is_terminal() {
        eprintln!("Error: No query path provided and no input on stdin.");
        exit(1);
    }
    let client = db::client();
    let db = client.database(&o.database);
    let coll = o.collection.as_deref();

    if let Some(dir) = o.query_path.as_deref().filter(|p| p.is_dir()) {
        run_dir(&db, dir, coll);
        return;
    }

    let text = match &o.query_path {
        Some(p) => fs::read_to_string(p).unwrap_or_else(|e| fail(e)),
        None => {
            let mut s = String::new();
            std::io::stdin()
                .read_to_string(&mut s)
                .unwrap_or_else(|e| fail(e));
            s
        }
    };
    match parse(&text).and_then(|q| execute(&db, q, coll)) {
        Ok(Outcome::Docs(docs)) => print_docs(docs, o.format),
        Ok(obj) => println!(
            "{}",
            pyfmt::dumps(&outcome_json(obj)).unwrap_or_else(|e| fail(e))
        ),
        Err(e) => {
            eprintln!(
                "{}",
                pyfmt::dumps_compact(&Bson::Document(doc! {"error": e}))
            );
            exit(1);
        }
    }
}
