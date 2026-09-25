// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Resource documents: YAML discovery and parsing, and conversion back.

use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use serde_json::{Map, Number, Value};

/// A resource row as stored in `public.resources`.
pub struct Resource {
    pub api_version: Value,
    pub kind: Value,
    pub name: Value,
    pub namespace: Value,
    pub labels: Option<Value>,
    pub annotations: Option<Value>,
    pub spec: Value,
}

/// Recursively list `.yaml`/`.yml` files, top-down like `os.walk`:
/// a directory's files come before its subdirectories' files, and
/// symlinked directories are not followed. Unreadable dirs are skipped.
pub fn find_yaml_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(dir, &mut out);
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let symlink = entry.file_type().is_ok_and(|t| t.is_symlink());
            if !symlink {
                subdirs.push(path);
            }
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".yaml") || name.ends_with(".yml") {
            out.push(path);
        }
    }
    for d in subdirs {
        walk(&d, out);
    }
}

/// Outcome of parsing one YAML document.
pub enum Parsed {
    /// Not a mapping; silently ignored.
    NotMapping,
    /// Missing apiVersion, kind or metadata.name.
    Incomplete,
    Resource(Box<Resource>),
}

/// Parse every document of a YAML stream. Stops at the first error,
/// keeping the documents parsed before it.
pub fn parse_stream(text: &str) -> (Vec<Parsed>, Option<String>) {
    let mut out = Vec::new();
    for de in serde_yaml::Deserializer::from_str(text) {
        let doc = match serde_yaml::Value::deserialize(de) {
            Ok(v) => to_json(v),
            Err(e) => return (out, Some(e.to_string())),
        };
        match parse_resource(&doc) {
            Ok(p) => out.push(p),
            Err(e) => return (out, Some(e)),
        }
    }
    (out, None)
}

/// Python truthiness of a JSON value.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

pub fn parse_resource(doc: &Value) -> Result<Parsed, String> {
    let Value::Object(doc) = doc else {
        return Ok(Parsed::NotMapping);
    };
    let empty = Value::Object(Map::new());
    let metadata = match doc.get("metadata").unwrap_or(&empty) {
        Value::Object(m) => m,
        _ => return Err("metadata is not a mapping".into()),
    };
    let api_version = doc.get("apiVersion").cloned().unwrap_or(Value::Null);
    let kind = doc.get("kind").cloned().unwrap_or(Value::Null);
    let name = metadata.get("name").cloned().unwrap_or(Value::Null);
    if !truthy(&api_version) || !truthy(&kind) || !truthy(&name) {
        return Ok(Parsed::Incomplete);
    }
    let labels = metadata.get("labels").filter(|v| !v.is_null()).cloned();
    let annotations = metadata
        .get("annotations")
        .filter(|v| !v.is_null())
        .cloned();
    Ok(Parsed::Resource(Box::new(Resource {
        api_version,
        kind,
        name,
        namespace: metadata
            .get("namespace")
            .cloned()
            .unwrap_or_else(|| Value::String("main".into())),
        labels,
        annotations,
        spec: doc.get("spec").cloned().unwrap_or(empty),
    })))
}

/// Convert a YAML value to JSON the way `json.dumps` would serialize
/// the PyYAML-decoded object (non-string keys become strings).
pub fn to_json(v: serde_yaml::Value) -> Value {
    use serde_yaml::Value as Y;
    match v {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(b),
        Y::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
            (Some(i), _, _) => Value::from(i),
            (_, Some(u), _) => Value::from(u),
            (_, _, Some(f)) => Number::from_f64(f).map_or(Value::Null, Value::Number),
            _ => Value::Null,
        },
        Y::String(s) => Value::String(s),
        Y::Sequence(s) => Value::Array(s.into_iter().map(to_json).collect()),
        Y::Mapping(m) => Value::Object(
            m.into_iter()
                .map(|(k, v)| {
                    let k = match to_json(k) {
                        Value::String(s) => s,
                        other => other.to_string(),
                    };
                    (k, to_json(v))
                })
                .collect(),
        ),
        Y::Tagged(t) => to_json(t.value),
    }
}

/// Python `str()` of a scalar, for messages.
pub fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        other => other.to_string(),
    }
}

/// SQL text parameter for a scalar (NULL for null).
pub fn sql_text(v: &Value) -> Option<String> {
    (!v.is_null()).then(|| text(v))
}

/// Rebuild a resource document from a database row.
pub fn resource_to_doc(
    api_version: Option<String>,
    kind: Option<String>,
    name: Option<String>,
    namespace: Option<String>,
    labels: Option<Value>,
    annotations: Option<Value>,
    spec: Option<Value>,
) -> Value {
    let s = |v: Option<String>| v.map_or(Value::Null, Value::String);
    let mut metadata = Map::new();
    metadata.insert("name".into(), s(name));
    metadata.insert("namespace".into(), s(namespace));
    if let Some(l) = labels.filter(truthy) {
        metadata.insert("labels".into(), l);
    }
    if let Some(a) = annotations.filter(truthy) {
        metadata.insert("annotations".into(), a);
    }
    let mut doc = Map::new();
    doc.insert("apiVersion".into(), s(api_version));
    doc.insert("kind".into(), s(kind));
    doc.insert("metadata".into(), Value::Object(metadata));
    if let Some(sp) = spec.filter(truthy) {
        doc.insert("spec".into(), sp);
    }
    Value::Object(doc)
}

/// Normalize a path like Python's `pathlib.PurePosixPath` does when
/// printed: drop `.` components and redundant slashes.
pub fn py_path(p: &Path) -> PathBuf {
    let out: PathBuf = p
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect();
    match out.as_os_str().is_empty() {
        true => PathBuf::from("."),
        false => out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(y: &str) -> (Vec<Parsed>, Option<String>) {
        parse_stream(y)
    }

    #[test]
    fn parses_multi_doc() {
        let (docs, err) = parse(
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: a\n  labels: {x: y}\nspec:\n  k: 1\n---\n- 1\n---\nkind: X\n",
        );
        assert!(err.is_none());
        assert_eq!(docs.len(), 3);
        let Parsed::Resource(r) = &docs[0] else {
            panic!("expected resource")
        };
        assert_eq!(r.namespace, json!("main"));
        assert_eq!(r.labels, Some(json!({"x": "y"})));
        assert!(r.annotations.is_none());
        assert_eq!(r.spec, json!({"k": 1}));
        assert!(matches!(docs[1], Parsed::NotMapping));
        assert!(matches!(docs[2], Parsed::Incomplete));
    }

    #[test]
    fn empty_stream_and_missing_spec() {
        assert!(parse("").0.iter().all(|d| matches!(d, Parsed::NotMapping)));
        let (docs, _) = parse("apiVersion: v1\nkind: A\nmetadata: {name: n, namespace: ns}\n");
        let Parsed::Resource(r) = &docs[0] else {
            panic!("expected resource")
        };
        assert_eq!(r.spec, json!({}));
        assert_eq!(r.namespace, json!("ns"));
    }

    #[test]
    fn non_string_keys() {
        let v: serde_yaml::Value = serde_yaml::from_str("1: a\ntrue: b\n").unwrap();
        assert_eq!(to_json(v), json!({"1": "a", "true": "b"}));
    }

    #[test]
    fn doc_omits_empty_fields() {
        let d = resource_to_doc(
            Some("v1".into()),
            Some("K".into()),
            Some("n".into()),
            Some("main".into()),
            Some(json!({})),
            None,
            Some(json!({"b": 1, "a": 2})),
        );
        let y = serde_yaml::to_string(&d).unwrap();
        assert_eq!(
            y,
            "apiVersion: v1\nkind: K\nmetadata:\n  name: n\n  namespace: main\nspec:\n  a: 2\n  b: 1\n"
        );
    }

    #[test]
    fn python_like_paths() {
        assert_eq!(py_path(Path::new("./out/")), PathBuf::from("out"));
        assert_eq!(py_path(Path::new(".")), PathBuf::from("."));
        assert_eq!(py_path(Path::new("a/./b")), PathBuf::from("a/b"));
    }
}
