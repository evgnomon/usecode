// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Python-compatible rendering of BSON values: `json.dumps(..., indent=2)`
//! with the original tool's serializer, and `str()` / `repr()` as pymongo
//! would produce them.

use std::fmt::Write;

use chrono::DateTime;
use mongodb::bson::{Bson, Document};
use serde_json::Value;

// ── JSON input → BSON ──────────────────────────────────────────────────────────

/// Convert parsed JSON to BSON the way pymongo encodes Python objects.
pub fn to_bson(v: &Value) -> Result<Bson, String> {
    Ok(match v {
        Value::Null => Bson::Null,
        Value::Bool(b) => Bson::Boolean(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                match i32::try_from(i) {
                    Ok(small) => Bson::Int32(small),
                    Err(_) => Bson::Int64(i),
                }
            } else if n.is_u64() {
                return Err("MongoDB can only handle up to 8-byte ints".into());
            } else {
                Bson::Double(n.as_f64().unwrap_or(f64::NAN))
            }
        }
        Value::String(s) => Bson::String(s.clone()),
        Value::Array(a) => Bson::Array(a.iter().map(to_bson).collect::<Result<_, _>>()?),
        Value::Object(m) => {
            let mut d = Document::new();
            for (k, v) in m {
                d.insert(k.clone(), to_bson(v)?);
            }
            Bson::Document(d)
        }
    })
}

/// Convert JSON that must be an object into a BSON document.
pub fn to_doc(v: &Value, what: &str) -> Result<Document, String> {
    match to_bson(v)? {
        Bson::Document(d) => Ok(d),
        _ => Err(format!("{what} must be an instance of dict")),
    }
}

// ── Python truthiness ──────────────────────────────────────────────────────────

pub fn truthy(v: &Bson) -> bool {
    match v {
        Bson::Null | Bson::Undefined => false,
        Bson::Boolean(b) => *b,
        Bson::Int32(i) => *i != 0,
        Bson::Int64(i) => *i != 0,
        Bson::Double(f) => *f != 0.0,
        Bson::String(s) | Bson::Symbol(s) | Bson::JavaScriptCode(s) => !s.is_empty(),
        Bson::Document(d) => !d.is_empty(),
        Bson::Array(a) => !a.is_empty(),
        Bson::Binary(b) => !b.bytes.is_empty(),
        _ => true,
    }
}

/// Numeric BSON value as f64 (dbStats sizes may be int or double).
pub fn as_f64(v: &Bson) -> Option<f64> {
    match v {
        Bson::Int32(i) => Some(f64::from(*i)),
        Bson::Int64(i) => Some(*i as f64),
        Bson::Double(f) => Some(*f),
        _ => None,
    }
}

// ── float repr ────────────────────────────────────────────────────────────────

/// Python `repr(float)`.
pub fn float_repr(f: f64) -> String {
    if f.is_nan() {
        return "nan".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf" } else { "-inf" }.into();
    }
    // Rust's LowerExp yields the shortest round-trip digits, e.g. "-1.25e-7".
    let s = format!("{f:e}");
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let (neg, mant) = match mant.strip_prefix('-') {
        Some(m) => (true, m),
        None => (false, mant),
    };
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if !(-4..16).contains(&exp) {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let sign = if exp < 0 { '-' } else { '+' };
        let _ = write!(out, "e{sign}{:02}", exp.abs());
    } else if exp < 0 {
        out.push_str("0.");
        out.push_str(&"0".repeat((-exp - 1) as usize));
        out.push_str(&digits);
    } else {
        let int_len = exp as usize + 1;
        if digits.len() <= int_len {
            out.push_str(&digits);
            out.push_str(&"0".repeat(int_len - digits.len()));
            out.push_str(".0");
        } else {
            out.push_str(&digits[..int_len]);
            out.push('.');
            out.push_str(&digits[int_len..]);
        }
    }
    out
}

fn datetime_parts(ms: i64) -> Option<chrono::NaiveDateTime> {
    DateTime::from_timestamp_millis(ms).map(|d| d.naive_utc())
}

/// Python `datetime.isoformat()` (sep `T`) or `str()` (sep space).
fn datetime_iso(ms: i64, sep: char) -> String {
    let Some(dt) = datetime_parts(ms) else {
        return ms.to_string();
    };
    let micros = dt.and_utc().timestamp_subsec_micros();
    let mut s = dt.format(&format!("%Y-%m-%d{sep}%H:%M:%S")).to_string();
    if micros != 0 {
        let _ = write!(s, ".{micros:06}");
    }
    s
}

fn datetime_repr(ms: i64) -> String {
    use chrono::{Datelike, Timelike};
    let Some(dt) = datetime_parts(ms) else {
        return ms.to_string();
    };
    let micros = dt.and_utc().timestamp_subsec_micros();
    let mut s = format!(
        "datetime.datetime({}, {}, {}, {}, {}",
        dt.year(),
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute()
    );
    if dt.second() != 0 || micros != 0 {
        let _ = write!(s, ", {}", dt.second());
    }
    if micros != 0 {
        let _ = write!(s, ", {micros}");
    }
    s.push(')');
    s
}

// ── JSON output ───────────────────────────────────────────────────────────────

fn json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ' '..='~' | '\u{7f}' => out.push(c),
            _ => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out.push('"');
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn not_serializable(class: &str) -> String {
    format!("Type <class '{class}'> not serializable")
}

fn json_value(
    out: &mut String,
    v: &Bson,
    indent: Option<usize>,
    level: usize,
) -> Result<(), String> {
    let newline = |out: &mut String, lvl: usize| {
        if let Some(n) = indent {
            out.push('\n');
            out.push_str(&" ".repeat(n * lvl));
        }
    };
    match v {
        Bson::Null | Bson::Undefined => out.push_str("null"),
        Bson::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        Bson::Int32(i) => out.push_str(&i.to_string()),
        Bson::Int64(i) => out.push_str(&i.to_string()),
        Bson::Double(f) => {
            if f.is_nan() {
                out.push_str("NaN");
            } else if f.is_infinite() {
                out.push_str(if *f > 0.0 { "Infinity" } else { "-Infinity" });
            } else {
                out.push_str(&float_repr(*f));
            }
        }
        Bson::String(s) | Bson::Symbol(s) | Bson::JavaScriptCode(s) => json_string(out, s),
        Bson::JavaScriptCodeWithScope(c) => json_string(out, &c.code),
        Bson::ObjectId(o) => json_string(out, &o.to_hex()),
        Bson::Binary(b) => json_string(out, &hex(&b.bytes)),
        Bson::DateTime(d) => json_string(out, &datetime_iso(d.timestamp_millis(), 'T')),
        Bson::Array(a) => {
            if a.is_empty() {
                out.push_str("[]");
                return Ok(());
            }
            out.push('[');
            for (i, item) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                    if indent.is_none() {
                        out.push(' ');
                    }
                }
                newline(out, level + 1);
                json_value(out, item, indent, level + 1)?;
            }
            newline(out, level);
            out.push(']');
        }
        Bson::Document(d) => json_doc(out, d, indent, level)?,
        Bson::Decimal128(_) => return Err(not_serializable("bson.decimal128.Decimal128")),
        Bson::RegularExpression(_) => return Err(not_serializable("bson.regex.Regex")),
        Bson::Timestamp(_) => return Err(not_serializable("bson.timestamp.Timestamp")),
        Bson::MaxKey => return Err(not_serializable("bson.max_key.MaxKey")),
        Bson::MinKey => return Err(not_serializable("bson.min_key.MinKey")),
        Bson::DbPointer(_) => return Err(not_serializable("bson.dbref.DBRef")),
    }
    Ok(())
}

fn json_doc(
    out: &mut String,
    d: &Document,
    indent: Option<usize>,
    level: usize,
) -> Result<(), String> {
    if d.is_empty() {
        out.push_str("{}");
        return Ok(());
    }
    out.push('{');
    for (i, (k, v)) in d.iter().enumerate() {
        if i > 0 {
            out.push(',');
            if indent.is_none() {
                out.push(' ');
            }
        }
        if let Some(n) = indent {
            out.push('\n');
            out.push_str(&" ".repeat(n * (level + 1)));
        }
        json_string(out, k);
        out.push_str(": ");
        json_value(out, v, indent, level + 1)?;
    }
    if let Some(n) = indent {
        out.push('\n');
        out.push_str(&" ".repeat(n * level));
    }
    out.push('}');
    Ok(())
}

/// Python `json.dumps(v, default=json_serializer, indent=2)`.
pub fn dumps(v: &Bson) -> Result<String, String> {
    let mut out = String::new();
    json_value(&mut out, v, Some(2), 0)?;
    Ok(out)
}

/// Python `json.dumps(v)` (compact, default separators).
pub fn dumps_compact(v: &Bson) -> String {
    let mut out = String::new();
    let _ = json_value(&mut out, v, None, 0);
    out
}

// ── str() / repr() ─────────────────────────────────────────────────────────────

fn quoted(s: &str, bytes: bool) -> String {
    let q = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::new();
    if bytes {
        out.push('b');
    }
    out.push(q);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c == '\u{7f}' || (bytes && (c as u32) > 0x7f) => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c if c.is_control() => {
                let n = c as u32;
                if n <= 0xff {
                    let _ = write!(out, "\\x{n:02x}");
                } else {
                    let _ = write!(out, "\\u{n:04x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

fn bytes_repr(b: &[u8]) -> String {
    // Each byte maps to the char with the same code point (latin-1).
    let s: String = b.iter().map(|&x| x as char).collect();
    quoted(&s, true)
}

/// Python `repr()` of the value pymongo decodes a BSON value into.
pub fn repr(v: &Bson) -> String {
    match v {
        Bson::String(s) | Bson::Symbol(s) => quoted(s, false),
        Bson::JavaScriptCode(s) => format!("Code({}, None)", quoted(s, false)),
        Bson::JavaScriptCodeWithScope(c) => format!(
            "Code({}, {})",
            quoted(&c.code, false),
            repr(&Bson::Document(c.scope.clone()))
        ),
        Bson::ObjectId(o) => format!("ObjectId('{}')", o.to_hex()),
        Bson::DateTime(d) => datetime_repr(d.timestamp_millis()),
        Bson::Binary(b) => {
            let sub = u8::from(b.subtype);
            if sub == 0 {
                bytes_repr(&b.bytes)
            } else {
                format!("Binary({}, {sub})", bytes_repr(&b.bytes))
            }
        }
        Bson::Decimal128(d) => format!("Decimal128('{d}')"),
        Bson::Array(a) => {
            let items: Vec<String> = a.iter().map(repr).collect();
            format!("[{}]", items.join(", "))
        }
        Bson::Document(d) => {
            let items: Vec<String> = d
                .iter()
                .map(|(k, v)| format!("{}: {}", quoted(k, false), repr(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
        other => py_str(other),
    }
}

/// Python `str()` of the value pymongo decodes a BSON value into.
pub fn py_str(v: &Bson) -> String {
    match v {
        Bson::Null | Bson::Undefined => "None".into(),
        Bson::Boolean(b) => if *b { "True" } else { "False" }.into(),
        Bson::Int32(i) => i.to_string(),
        Bson::Int64(i) => i.to_string(),
        Bson::Double(f) => float_repr(*f),
        Bson::String(s) | Bson::Symbol(s) | Bson::JavaScriptCode(s) => s.clone(),
        Bson::JavaScriptCodeWithScope(c) => c.code.clone(),
        Bson::ObjectId(o) => o.to_hex(),
        Bson::DateTime(d) => datetime_iso(d.timestamp_millis(), ' '),
        Bson::Binary(b) => bytes_repr(&b.bytes),
        Bson::Decimal128(d) => d.to_string(),
        Bson::RegularExpression(r) => {
            format!(
                "Regex({}, {})",
                quoted(&r.pattern, false),
                regex_flags(&r.options)
            )
        }
        Bson::Timestamp(t) => format!("Timestamp({}, {})", t.time, t.increment),
        Bson::MaxKey => "MaxKey()".into(),
        Bson::MinKey => "MinKey()".into(),
        Bson::DbPointer(p) => format!("{p:?}"),
        Bson::Array(_) | Bson::Document(_) => repr(v),
    }
}

/// Python `re` flag bits as pymongo's `Regex.flags`.
fn regex_flags(opts: &str) -> u32 {
    opts.chars()
        .map(|c| match c {
            'i' => 2,
            'l' => 4,
            'm' => 8,
            's' => 16,
            'u' => 32,
            'x' => 64,
            _ => 0,
        })
        .sum()
}

// ── table output ──────────────────────────────────────────────────────────────

fn cell(doc: &Document, col: &str) -> String {
    match doc.get(col) {
        None | Some(Bson::Null) | Some(Bson::Undefined) => "null".into(),
        Some(v) => py_str(v),
    }
}

/// Render documents as a table with headers from the first document.
/// Returns `None` when there is nothing to show.
pub fn table(docs: &[Document]) -> Option<String> {
    let first = docs.first()?;
    let headers: Vec<&str> = first.keys().map(String::as_str).collect();
    let rows: Vec<Vec<String>> = docs
        .iter()
        .map(|d| headers.iter().map(|h| cell(d, h)).collect())
        .collect();
    let widths: Vec<usize> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            rows.iter()
                .map(|r| r[i].chars().count())
                .fold(h.chars().count(), usize::max)
        })
        .collect();
    let pad = |s: &str, w: usize| format!("{s}{}", " ".repeat(w.saturating_sub(s.chars().count())));
    let mut out = String::new();
    let head: Vec<String> = headers
        .iter()
        .zip(&widths)
        .map(|(h, w)| pad(h, *w))
        .collect();
    out.push_str(&head.join(" | "));
    out.push('\n');
    let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    out.push_str(&sep.join("-|-"));
    for r in &rows {
        out.push('\n');
        let line: Vec<String> = r.iter().zip(&widths).map(|(c, w)| pad(c, *w)).collect();
        out.push_str(&line.join(" | "));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::bson::{doc, oid::ObjectId};

    #[test]
    fn floats() {
        assert_eq!(float_repr(1.0), "1.0");
        assert_eq!(float_repr(0.0), "0.0");
        assert_eq!(float_repr(-0.0), "-0.0");
        assert_eq!(float_repr(1.5), "1.5");
        assert_eq!(float_repr(0.1), "0.1");
        assert_eq!(float_repr(123456.789), "123456.789");
        assert_eq!(float_repr(1e16), "1e+16");
        assert_eq!(float_repr(1.5e16), "1.5e+16");
        assert_eq!(float_repr(1e15), "1000000000000000.0");
        assert_eq!(float_repr(0.0001), "0.0001");
        assert_eq!(float_repr(0.00001), "1e-05");
        assert_eq!(float_repr(-2.5e-7), "-2.5e-07");
        assert_eq!(float_repr(1e100), "1e+100");
    }

    #[test]
    fn json_dumps() {
        let d = doc! {"a": 1, "b": [1.0, "é"], "c": {}, "d": [], "e": Bson::Null};
        assert_eq!(
            dumps(&Bson::Document(d)).unwrap(),
            "{\n  \"a\": 1,\n  \"b\": [\n    1.0,\n    \"\\u00e9\"\n  ],\n  \"c\": {},\n  \"d\": [],\n  \"e\": null\n}"
        );
        assert_eq!(dumps(&Bson::Array(vec![])).unwrap(), "[]");
        assert_eq!(
            dumps_compact(&Bson::Document(doc! {"error": "x\"y", "n": [1, 2]})),
            "{\"error\": \"x\\\"y\", \"n\": [1, 2]}"
        );
        assert!(dumps(&Bson::MinKey).is_err());
        assert_eq!(dumps(&Bson::Double(f64::NAN)).unwrap(), "NaN");
    }

    #[test]
    fn json_emoji_surrogates() {
        assert_eq!(
            dumps(&Bson::String("😀".into())).unwrap(),
            "\"\\ud83d\\ude00\""
        );
    }

    #[test]
    fn datetimes() {
        let dt = Bson::DateTime(mongodb::bson::DateTime::from_millis(1_700_000_000_123));
        assert_eq!(dumps(&dt).unwrap(), "\"2023-11-14T22:13:20.123000\"");
        assert_eq!(py_str(&dt), "2023-11-14 22:13:20.123000");
        let dt0 = Bson::DateTime(mongodb::bson::DateTime::from_millis(1_700_000_000_000));
        assert_eq!(dumps(&dt0).unwrap(), "\"2023-11-14T22:13:20\"");
        assert_eq!(repr(&dt0), "datetime.datetime(2023, 11, 14, 22, 13, 20)");
    }

    #[test]
    fn strs() {
        let oid = ObjectId::parse_str("65a1b2c3d4e5f6a7b8c9d0e1").unwrap();
        assert_eq!(py_str(&Bson::ObjectId(oid)), "65a1b2c3d4e5f6a7b8c9d0e1");
        assert_eq!(py_str(&Bson::Boolean(true)), "True");
        assert_eq!(py_str(&Bson::String("it's".into())), "it's");
        let d = doc! {"a": "x", "b": [1, true, Bson::Null], "c": "it's", "_id": oid};
        assert_eq!(
            py_str(&Bson::Document(d)),
            "{'a': 'x', 'b': [1, True, None], 'c': \"it's\", '_id': ObjectId('65a1b2c3d4e5f6a7b8c9d0e1')}"
        );
    }

    #[test]
    fn json_to_bson() {
        let v: Value = serde_json::from_str(r#"{"a": 1, "b": 3000000000, "c": 1.0}"#).unwrap();
        assert_eq!(
            to_bson(&v).unwrap(),
            Bson::Document(doc! {"a": 1i32, "b": 3_000_000_000i64, "c": 1.0})
        );
    }

    #[test]
    fn tables() {
        let docs = vec![doc! {"a": 1, "bb": "x"}, doc! {"a": 22}];
        assert_eq!(
            table(&docs).unwrap(),
            "a  | bb  \n---|-----\n1  | x   \n22 | null"
        );
        assert!(table(&[]).is_none());
    }
}
