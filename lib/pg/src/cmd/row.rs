use crate::Target;
use crate::db::{connect, die, err_text, qualified, rows_json};

pub fn rm(table: &str, id: Option<i64>, all: bool, t: &Target) {
    if id.is_none() && !all {
        die("Error: provide an id or use --all to delete all rows.");
    }
    let schema = &t.schema;
    let q = qualified(schema, table);
    let mut client = connect(t.database.as_deref());
    let result = match id {
        Some(id) => client.execute(&format!("DELETE FROM {q} WHERE id = $1"), &[&id]),
        None => client.execute(&format!("DELETE FROM {q}"), &[]),
    };
    let n = result.unwrap_or_else(|e| die(format!("Error: {}", err_text(&e))));
    match id {
        Some(id) => eprintln!("Deleted {n} row(s) from '{schema}.{table}' where id={id}."),
        None => eprintln!("Deleted {n} row(s) from '{schema}.{table}'."),
    }
}

pub fn list(table: &str, t: &Target, limit: i64) {
    let mut client = connect(t.database.as_deref());
    let sql = format!("SELECT * FROM {} LIMIT $1", qualified(&t.schema, table));
    let rows = client
        .query(&sql, &[&limit])
        .unwrap_or_else(|e| die(format!("Error: {}", err_text(&e))));
    println!(
        "{}",
        serde_json::to_string(&rows_json(&rows)).unwrap_or_default()
    );
}
