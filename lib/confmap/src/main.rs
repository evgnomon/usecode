// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

mod pyfmt;

use std::io::{IsTerminal, Read};
use std::process;

use clap::Parser;
use postgres::{Client, NoTls};
use serde_json::{Map, Value};

use crate::pyfmt::py_str;

const PG_PASSWORD: &str = "t6drtfyig7";
const PG_USER: &str = "admin";
const PG_PORT: u16 = 5432;
const PG_HOST: &str = "localhost";
const PG_DATABASE: &str = "z";

/// Query ConfigMaps from database
///
/// When stdin is not a terminal, its content (stripped) is written to
/// CONFIG_NAME's KEY instead, creating the ConfigMap if needed.
#[derive(Parser)]
#[command(name = "confmap")]
struct Cli {
    /// Name of the ConfigMap to find
    config_name: Option<String>,
    /// Specific key to retrieve from the ConfigMap spec
    key: Option<String>,
    /// Namespace to query
    #[arg(long, default_value = "main")]
    namespace: String,
    /// Tenant to query
    #[arg(long, default_value = "system")]
    tenant: String,
}

/// Print to stdout (as the original tool does) and exit 1.
fn fail(msg: impl std::fmt::Display) -> ! {
    println!("{msg}");
    process::exit(1)
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

fn connect_db() -> Client {
    let mut cfg = postgres::Config::new();
    // libpq (used by the original psycopg2 tool) honors PGCONNECT_TIMEOUT.
    if let Some(secs) = std::env::var("PGCONNECT_TIMEOUT")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&s| s > 0)
    {
        cfg.connect_timeout(std::time::Duration::from_secs(secs.max(2)));
    }
    cfg.host(PG_HOST)
        .port(PG_PORT)
        .user(PG_USER)
        .password(PG_PASSWORD)
        .dbname(PG_DATABASE)
        .connect(NoTls)
        .unwrap_or_else(|e| fail(format!("Error connecting to database: {}", err_text(&e))))
}

fn update_config_map(
    conn: &mut Client,
    config_name: &str,
    key: &str,
    value: &str,
    namespace: &str,
    tenant: &str,
) {
    let res = (|| -> Result<String, postgres::Error> {
        let mut tx = conn.transaction()?;
        let row = tx.query_opt(
            "SELECT id::text AS id_text, spec
            FROM public.resources
            WHERE kind = 'ConfigMap'
                AND name = $1
                AND namespace = $2
                AND tenant = $3",
            &[&config_name, &namespace, &tenant],
        )?;
        let Some(row) = row else {
            let mut spec = Map::new();
            spec.insert(key.into(), Value::String(value.into()));
            tx.execute(
                "INSERT INTO public.resources (api_version, kind, name, namespace, tenant, spec)
                VALUES ($1, $2, $3, $4, $5, $6)",
                &[
                    &"v1",
                    &"ConfigMap",
                    &config_name,
                    &namespace,
                    &tenant,
                    &Value::Object(spec),
                ],
            )?;
            tx.commit()?;
            return Ok(format!(
                "Created ConfigMap '{config_name}' with key '{key}'"
            ));
        };
        let id: String = row.get(0);
        let spec: Option<Value> = row.get(1);
        let mut spec = match spec {
            Some(Value::Object(m)) if !m.is_empty() => m,
            _ => Map::new(),
        };
        spec.insert(key.into(), Value::String(value.into()));
        tx.execute(
            "UPDATE public.resources
            SET spec = $1
            WHERE id::text = $2",
            &[&Value::Object(spec), &id],
        )?;
        tx.commit()?;
        Ok(format!("Updated ConfigMap '{config_name}' key '{key}'"))
    })();
    match res {
        Ok(msg) => println!("{msg}"),
        Err(e) => fail(format!("Error updating database: {}", err_text(&e))),
    }
}

fn list_config_maps(
    conn: &mut Client,
    config_name: Option<&str>,
    key: Option<&str>,
    namespace: &str,
    tenant: &str,
) {
    let mut query = String::from(
        "SELECT
            id::text AS id_text, api_version, kind, name, namespace, tenant,
            labels, annotations, spec
        FROM public.resources
        WHERE kind = 'ConfigMap'
            AND namespace = $1
            AND tenant = $2",
    );
    let mut params: Vec<&(dyn postgres::types::ToSql + Sync)> = vec![&namespace, &tenant];
    let config_name = config_name.filter(|s| !s.is_empty());
    if let Some(name) = &config_name {
        query.push_str(" AND name = $3");
        params.push(name);
    }
    query.push_str(" ORDER BY id;");

    let rows = conn
        .query(query.as_str(), &params)
        .unwrap_or_else(|e| fail(format!("Error querying database: {}", err_text(&e))));

    if rows.is_empty() {
        println!("No ConfigMaps found in database.");
        return;
    }

    let key = key.filter(|s| !s.is_empty());
    if key.is_none() {
        println!("Found {} ConfigMap(s):\n", rows.len());
    }

    for row in &rows {
        let text = |i: usize| -> String {
            row.get::<_, Option<String>>(i)
                .unwrap_or_else(|| "None".into())
        };
        let json = |i: usize| -> Option<Value> { row.get(i) };
        let spec = json(8);

        if let Some(key) = key {
            if let Some(Value::Object(m)) = &spec
                && let Some(v) = m.get(key)
            {
                println!("{}", py_str(v));
            }
            continue;
        }

        println!("ID: {}", text(0));
        println!("  api_version: {}", text(1));
        println!("  kind: {}", text(2));
        println!("  name: {}", text(3));
        println!("  namespace: {}", text(4));
        println!("  tenant: {}", text(5));
        for (title, v) in [
            ("labels", json(6)),
            ("annotations", json(7)),
            ("spec", spec),
        ] {
            if let Some(Value::Object(m)) = v
                && !m.is_empty()
            {
                println!("  {title}:");
                for (k, v) in &m {
                    println!("    {k}: {}", py_str(v));
                }
            }
        }
        println!();
    }
}

fn main() {
    let cli = Cli::parse();
    let mut conn = connect_db();

    if !std::io::stdin().is_terminal() {
        let (Some(name), Some(key)) = (
            cli.config_name.as_deref().filter(|s| !s.is_empty()),
            cli.key.as_deref().filter(|s| !s.is_empty()),
        ) else {
            fail("Error: config_name and key are required when reading from stdin");
        };
        let mut input = Vec::new();
        if let Err(e) = std::io::stdin().read_to_end(&mut input) {
            fail(format!("Error reading stdin: {e}"));
        }
        let value = String::from_utf8_lossy(&input);
        update_config_map(
            &mut conn,
            name,
            key,
            value.trim(),
            &cli.namespace,
            &cli.tenant,
        );
    } else {
        list_config_maps(
            &mut conn,
            cli.config_name.as_deref(),
            cli.key.as_deref(),
            &cli.namespace,
            &cli.tenant,
        );
    }
}
