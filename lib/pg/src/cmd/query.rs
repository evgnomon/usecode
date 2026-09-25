use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::Path;

use postgres::types::Type;
use postgres::{Client, SimpleQueryMessage};
use serde_json::{Map, Value, json};

use crate::db::{binary_supported, connect, die, err_text, rows_json, text_value};
use crate::{Format, QueryOpts};

enum Outcome {
    Rows(Vec<Map<String, Value>>),
    Affected(u64),
}

impl Outcome {
    fn into_json(self) -> Value {
        match self {
            Outcome::Rows(rows) => json!(rows),
            Outcome::Affected(n) => json!({"status": "success", "rows_affected": n}),
        }
    }
}

/// Execute `sql` and return the outcome of its last statement.
///
/// A single statement is prepared so results keep their column types. SQL
/// that cannot be prepared (e.g. several statements) goes through the simple
/// query protocol, where values come back as text. Results with types the
/// binary decoder does not know are also fetched as text, typed by column.
fn execute(client: &mut Client, sql: &str) -> Result<Outcome, postgres::Error> {
    let Ok(stmt) = client.prepare(sql) else {
        return simple(client, sql, &[]);
    };
    if stmt.columns().is_empty() {
        return client.execute(&stmt, &[]).map(Outcome::Affected);
    }
    let types: Vec<Type> = stmt.columns().iter().map(|c| c.type_().clone()).collect();
    if !types.iter().all(binary_supported) {
        return simple(client, sql, &types);
    }
    client
        .query(&stmt, &[])
        .map(|rows| Outcome::Rows(rows_json(&rows)))
}

fn simple(client: &mut Client, sql: &str, types: &[Type]) -> Result<Outcome, postgres::Error> {
    let mut last = Outcome::Affected(0);
    let mut current: Option<Vec<Map<String, Value>>> = None;
    for msg in client.simple_query(sql)? {
        match msg {
            SimpleQueryMessage::RowDescription(_) => current = Some(Vec::new()),
            SimpleQueryMessage::Row(row) => {
                let obj = row
                    .columns()
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        (
                            c.name().to_string(),
                            row.get(i)
                                .map_or(Value::Null, |v| text_value(types.get(i), v)),
                        )
                    })
                    .collect();
                current.get_or_insert_with(Vec::new).push(obj);
            }
            SimpleQueryMessage::CommandComplete(n) => {
                last = current.take().map_or(Outcome::Affected(n), Outcome::Rows);
            }
            _ => {}
        }
    }
    Ok(last)
}

fn pretty(v: &impl serde::Serialize) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

fn cell(v: &Value) -> String {
    match v {
        Value::Null => "NULL".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn print_table(rows: &[Map<String, Value>]) {
    let Some(first) = rows.first() else {
        eprintln!("No rows returned.");
        return;
    };
    let headers: Vec<&String> = first.keys().collect();
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|r| headers.iter().map(|h| cell(&r[h.as_str()])).collect())
        .collect();
    let widths: Vec<usize> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            cells
                .iter()
                .map(|r| r[i].chars().count())
                .fold(h.chars().count(), usize::max)
        })
        .collect();

    let line = |vals: Vec<&str>| {
        vals.iter()
            .zip(&widths)
            .map(|(v, w)| format!("{v:<w$}"))
            .collect::<Vec<_>>()
            .join(" | ")
    };
    println!("{}", line(headers.iter().map(|h| h.as_str()).collect()));
    println!(
        "{}",
        widths
            .iter()
            .map(|w| "-".repeat(*w))
            .collect::<Vec<_>>()
            .join("-|-")
    );
    for r in &cells {
        println!("{}", line(r.iter().map(String::as_str).collect()));
    }
}

fn run_dir(client: &mut Client, dir: &Path) {
    let mut files: Vec<String> = fs::read_dir(dir)
        .unwrap_or_else(|e| die(json!({"error": e.to_string()})))
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".sql"))
        .collect();
    files.sort();

    let mut results = Map::new();
    for name in files {
        let result = match fs::read_to_string(dir.join(&name)) {
            Ok(sql) => execute(client, &sql)
                .map(Outcome::into_json)
                .unwrap_or_else(|e| json!({"error": err_text(&e)})),
            Err(e) => json!({"error": e.to_string()}),
        };
        results.insert(name, result);
    }
    println!("{}", pretty(&results));
}

pub fn run(o: &QueryOpts) {
    let path = o.sql_path.as_deref();
    if let Some(p) = path.filter(|p| !p.exists()) {
        die(format!("Error: Path '{}' does not exist.", p.display()));
    }
    if path.is_none() && io::stdin().is_terminal() {
        die("Error: No SQL path provided and no input on stdin.");
    }

    let mut client = connect(o.database.as_deref());
    if let Some(dir) = path.filter(|p| p.is_dir()) {
        run_dir(&mut client, dir);
        return;
    }

    let sql = match path {
        Some(p) => fs::read_to_string(p),
        None => {
            let mut s = String::new();
            io::stdin().read_to_string(&mut s).map(|_| s)
        }
    }
    .unwrap_or_else(|e| die(json!({"error": e.to_string()})));

    match execute(&mut client, &sql) {
        Ok(Outcome::Rows(rows)) if matches!(o.format, Format::Table) => print_table(&rows),
        Ok(outcome) => println!("{}", pretty(&outcome.into_json())),
        Err(e) => die(json!({"error": err_text(&e)})),
    }
}
