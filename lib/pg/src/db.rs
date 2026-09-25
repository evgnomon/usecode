use std::process;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat, Utc};
use postgres::types::{FromSql, Type};
use postgres::{Client, Column, NoTls, Row};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde_json::{Map, Value, json};

use crate::config;

/// Print a message to stderr and exit with status 1.
pub fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("{msg}");
    process::exit(1)
}

/// Human-readable error text, preferring the server's message.
pub fn err_text(e: &postgres::Error) -> String {
    match e.as_db_error() {
        Some(db) => format!("{}: {}", db.severity(), db.message()),
        None => match std::error::Error::source(e) {
            Some(src) => format!("{e}: {src}"),
            None => e.to_string(),
        },
    }
}

/// Connect to `dbname`, or to $PGDATABASE / the configured database.
pub fn connect(dbname: Option<&str>) -> Client {
    let dbname = dbname.map_or_else(config::database, str::to_string);
    let port = config::port();
    let port: u16 = port
        .parse()
        .unwrap_or_else(|_| die(format!("Connection failed: invalid port '{port}'")));
    let connect_timeout = config::env_or("PGCONNECT_TIMEOUT", "10")
        .parse()
        .unwrap_or(10);
    postgres::Config::new()
        .host(&config::host())
        .port(port)
        .user(&config::user())
        .password(config::password())
        .dbname(&dbname)
        .connect_timeout(Duration::from_secs(connect_timeout))
        .connect(NoTls)
        .unwrap_or_else(|e| die(format!("Connection failed: {}", err_text(&e))))
}

pub fn ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

pub fn qualified(schema: &str, name: &str) -> String {
    format!("{}.{}", ident(schema), ident(name))
}

pub fn table_exists(
    client: &mut Client,
    schema: &str,
    table: &str,
) -> Result<bool, postgres::Error> {
    let row = client.query_one(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables \
         WHERE table_schema = $1 AND table_name = $2)",
        &[&schema, &table],
    )?;
    Ok(row.get(0))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn get<'a, T: FromSql<'a>>(row: &'a Row, i: usize, f: impl Fn(T) -> Value) -> Value {
    match row.try_get::<_, Option<T>>(i) {
        Ok(Some(v)) => f(v),
        _ => Value::Null,
    }
}

fn get_array<'a, T: FromSql<'a>>(row: &'a Row, i: usize, f: impl Fn(T) -> Value) -> Value {
    get::<Vec<Option<T>>>(row, i, |v| {
        Value::Array(v.into_iter().map(|x| x.map_or(Value::Null, &f)).collect())
    })
}

fn decimal(d: Decimal) -> Value {
    d.to_f64().map_or(Value::Null, |f| json!(f))
}

fn timestamp(t: NaiveDateTime) -> Value {
    json!(t.format("%Y-%m-%dT%H:%M:%S%.f").to_string())
}

fn timestamptz(t: DateTime<Utc>) -> Value {
    json!(t.to_rfc3339_opts(SecondsFormat::AutoSi, false))
}

fn string(s: String) -> Value {
    Value::String(s)
}

/// Convert column `i` of `row` to JSON, mirroring psycopg2 + json.dumps.
fn value(row: &Row, i: usize, col: &Column) -> Value {
    let t = col.type_();
    match *t {
        Type::BOOL => get::<bool>(row, i, |v| json!(v)),
        Type::INT2 => get::<i16>(row, i, |v| json!(v)),
        Type::INT4 => get::<i32>(row, i, |v| json!(v)),
        Type::INT8 => get::<i64>(row, i, |v| json!(v)),
        Type::OID => get::<u32>(row, i, |v| json!(v)),
        Type::FLOAT4 => get::<f32>(row, i, |v| json!(v)),
        Type::FLOAT8 => get::<f64>(row, i, |v| json!(v)),
        Type::NUMERIC => get::<Decimal>(row, i, decimal),
        Type::JSON | Type::JSONB => get::<Value>(row, i, |v| v),
        Type::UUID => get::<uuid::Uuid>(row, i, |v| json!(v.to_string())),
        Type::BYTEA => get::<Vec<u8>>(row, i, |v| json!(hex(&v))),
        Type::DATE => get::<NaiveDate>(row, i, |v| json!(v.to_string())),
        Type::TIME => get::<NaiveTime>(row, i, |v| json!(v.format("%H:%M:%S%.f").to_string())),
        Type::TIMESTAMP => get::<NaiveDateTime>(row, i, timestamp),
        Type::TIMESTAMPTZ => get::<DateTime<Utc>>(row, i, timestamptz),
        Type::BOOL_ARRAY => get_array::<bool>(row, i, |v| json!(v)),
        Type::INT2_ARRAY => get_array::<i16>(row, i, |v| json!(v)),
        Type::INT4_ARRAY => get_array::<i32>(row, i, |v| json!(v)),
        Type::INT8_ARRAY => get_array::<i64>(row, i, |v| json!(v)),
        Type::FLOAT4_ARRAY => get_array::<f32>(row, i, |v| json!(v)),
        Type::FLOAT8_ARRAY => get_array::<f64>(row, i, |v| json!(v)),
        Type::NUMERIC_ARRAY => get_array::<Decimal>(row, i, decimal),
        Type::JSON_ARRAY | Type::JSONB_ARRAY => get_array::<Value>(row, i, |v| v),
        Type::UUID_ARRAY => get_array::<uuid::Uuid>(row, i, |v| json!(v.to_string())),
        Type::TIMESTAMP_ARRAY => get_array::<NaiveDateTime>(row, i, timestamp),
        Type::TIMESTAMPTZ_ARRAY => get_array::<DateTime<Utc>>(row, i, timestamptz),
        Type::TEXT_ARRAY | Type::VARCHAR_ARRAY | Type::NAME_ARRAY | Type::BPCHAR_ARRAY => {
            get_array::<String>(row, i, string)
        }
        _ => get::<String>(row, i, string),
    }
}

/// Whether `value` can decode `t` from the binary protocol. Other types
/// (interval, inet, enums, ...) must be fetched as text, see `text_value`.
pub fn binary_supported(t: &Type) -> bool {
    const TYPES: &[Type] = &[
        Type::BOOL,
        Type::INT2,
        Type::INT4,
        Type::INT8,
        Type::OID,
        Type::FLOAT4,
        Type::FLOAT8,
        Type::NUMERIC,
        Type::JSON,
        Type::JSONB,
        Type::UUID,
        Type::BYTEA,
        Type::DATE,
        Type::TIME,
        Type::TIMESTAMP,
        Type::TIMESTAMPTZ,
        Type::BOOL_ARRAY,
        Type::INT2_ARRAY,
        Type::INT4_ARRAY,
        Type::INT8_ARRAY,
        Type::FLOAT4_ARRAY,
        Type::FLOAT8_ARRAY,
        Type::NUMERIC_ARRAY,
        Type::JSON_ARRAY,
        Type::JSONB_ARRAY,
        Type::UUID_ARRAY,
        Type::TIMESTAMP_ARRAY,
        Type::TIMESTAMPTZ_ARRAY,
        Type::TEXT_ARRAY,
        Type::VARCHAR_ARRAY,
        Type::NAME_ARRAY,
        Type::BPCHAR_ARRAY,
    ];
    TYPES.contains(t) || <String as FromSql>::accepts(t)
}

/// Convert a text-protocol value to JSON, typed when the column type is known.
pub fn text_value(t: Option<&Type>, s: &str) -> Value {
    let parsed = match t {
        Some(&Type::BOOL) => Some(json!(s == "t")),
        Some(&(Type::INT2 | Type::INT4 | Type::INT8 | Type::OID)) => {
            s.parse::<i64>().ok().map(|v| json!(v))
        }
        Some(&(Type::FLOAT4 | Type::FLOAT8 | Type::NUMERIC)) => {
            s.parse::<f64>().ok().map(|v| json!(v))
        }
        Some(&(Type::JSON | Type::JSONB)) => serde_json::from_str(s).ok(),
        _ => None,
    };
    parsed.unwrap_or_else(|| json!(s))
}

/// Rows as JSON objects keyed by column name, in column order.
pub fn rows_json(rows: &[Row]) -> Vec<Map<String, Value>> {
    rows.iter()
        .map(|row| {
            row.columns()
                .iter()
                .enumerate()
                .map(|(i, c)| (c.name().to_string(), value(row, i, c)))
                .collect()
        })
        .collect()
}
