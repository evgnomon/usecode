// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Reading package properties from `.deb.json` / `.deb.jsonc`.

use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{Map, Value};

use crate::log;
use crate::sys;

const JSONC: &str = "/usr/local/bin/jsonc";

/// Parsed top-level object of a `.deb.json` file (empty when absent).
#[derive(Debug, Default)]
pub struct DebJson(Map<String, Value>);

impl DebJson {
    pub fn parse(content: &str) -> Result<Self, serde_json::Error> {
        match serde_json::from_str::<Value>(content)? {
            Value::Object(m) => Ok(DebJson(m)),
            _ => Ok(DebJson::default()),
        }
    }

    /// Equivalent of `jq -r '.key // empty'`, with empty strings mapped to `None`.
    pub fn get(&self, key: &str) -> Option<String> {
        let s = match self.0.get(key)? {
            Value::Null | Value::Bool(false) => return None,
            Value::String(s) => s.clone(),
            v @ (Value::Array(_) | Value::Object(_)) => serde_json::to_string_pretty(v).ok()?,
            v => v.to_string(),
        };
        (!s.is_empty()).then_some(s)
    }
}

/// Load `file` if it is a regular file; `.jsonc` files are piped through
/// `/usr/local/bin/jsonc` first.
pub fn load(file: &str) -> DebJson {
    if !Path::new(file).is_file() {
        return DebJson::default();
    }
    log::info(&format!("Reading package properties from {file}"));
    let content = match file.ends_with(".jsonc") {
        true => strip_comments(file),
        false => std::fs::read_to_string(file)
            .unwrap_or_else(|e| log::die(&format!("Cannot read {file}: {e}"))),
    };
    DebJson::parse(&content).unwrap_or_else(|e| {
        eprintln!("mkdeb: cannot parse {file}: {e}");
        DebJson::default()
    })
}

fn strip_comments(file: &str) -> String {
    if !sys::is_executable(Path::new(JSONC)) {
        log::die(&format!("{JSONC} is required to parse {file}."));
    }
    let input =
        std::fs::File::open(file).unwrap_or_else(|e| log::die(&format!("Cannot read {file}: {e}")));
    let out = sys::output(Command::new(JSONC).stdin(input).stderr(Stdio::inherit()));
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jq_like_values() {
        let j = DebJson::parse(
            r#"{"name":"x","depends":"","n":1.5,"t":true,"f":false,"z":null,"a":[1]}"#,
        )
        .unwrap();
        assert_eq!(j.get("name").as_deref(), Some("x"));
        assert_eq!(j.get("depends"), None);
        assert_eq!(j.get("missing"), None);
        assert_eq!(j.get("n").as_deref(), Some("1.5"));
        assert_eq!(j.get("t").as_deref(), Some("true"));
        assert_eq!(j.get("f"), None);
        assert_eq!(j.get("z"), None);
        assert_eq!(j.get("a").as_deref(), Some("[\n  1\n]"));
    }

    #[test]
    fn non_object_is_empty() {
        assert_eq!(DebJson::parse("[1]").unwrap().get("name"), None);
        assert!(DebJson::parse("{").is_err());
    }
}
