// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Connection settings from the nearest `.pg.json`.

use std::env;
use std::fs;
use std::time::Duration;

use serde_json::{Map, Value};

use crate::die;

/// Search for `.pg.json` from the cwd up to `/`; empty when none is found.
fn find_pg_config() -> Map<String, Value> {
    let Ok(cwd) = env::current_dir() else {
        return Map::new();
    };
    let Some(path) = cwd
        .ancestors()
        .map(|d| d.join(".pg.json"))
        .find(|p| p.is_file())
    else {
        return Map::new();
    };
    let data = fs::read_to_string(&path)
        .unwrap_or_else(|e| die(format!("error reading {}: {e}", path.display())));
    match serde_json::from_str(&data) {
        Ok(Value::Object(m)) => m,
        Ok(_) => die(format!("{}: expected a JSON object", path.display())),
        Err(e) => die(format!("{}: {e}", path.display())),
    }
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}

/// libpq-style key/value DSN, as the original tool built it.
pub fn build_dsn() -> String {
    let cfg = find_pg_config();
    let get = |k: &str, default: &str| cfg.get(k).map_or_else(|| default.to_string(), text);
    format!(
        "host={} port={} dbname={} user={} password={}",
        get("host", "localhost"),
        get("port", "5432"),
        get("database", "postgres"),
        get("user", "admin"),
        get("password", "t6drtfyig7"),
    )
}

fn err_text(e: &postgres::Error) -> String {
    match e.as_db_error() {
        Some(db) => format!("{}: {}", db.severity(), db.message()),
        None => match std::error::Error::source(e) {
            Some(src) => format!("{e}: {src}"),
            None => e.to_string(),
        },
    }
}

pub fn fail(e: postgres::Error) -> ! {
    die(format!("error: {}", err_text(&e)))
}

pub fn connect() -> postgres::Client {
    let mut cfg: postgres::Config = build_dsn()
        .parse()
        .unwrap_or_else(|e: postgres::Error| die(format!("invalid DSN: {}", err_text(&e))));
    // libpq (used by the original psycopg2 tool) honors PGCONNECT_TIMEOUT.
    if let Some(secs) = env::var("PGCONNECT_TIMEOUT")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&s| s > 0)
    {
        cfg.connect_timeout(Duration::from_secs(secs.max(2)));
    }
    cfg.connect(postgres::NoTls).unwrap_or_else(|e| fail(e))
}
