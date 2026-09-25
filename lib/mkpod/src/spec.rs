// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Pure helpers for building Kubernetes documents from resource rows.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

/// Label linking a podman pod to its resource row.
pub const RESOURCE_LABEL: &str = "mkpod.resource-id";

/// A `Pod` row from `public.resources`.
#[derive(Debug, Default)]
pub struct PodRow {
    pub id: String,
    pub api_version: Option<String>,
    pub kind: Option<String>,
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub labels: Option<Value>,
    pub annotations: Option<Value>,
    pub spec: Option<Value>,
}

/// A pod manifest (its resource id is carried in the `mkpod.resource-id` label).
#[derive(Debug)]
pub struct Pod {
    pub doc: Value,
}

impl Pod {
    pub fn name(&self) -> String {
        py_str(&self.doc["metadata"]["name"])
    }
}

/// Python truthiness for JSON values.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `str()` of a JSON value as Python would print it (for scalars).
pub fn py_str(v: &Value) -> String {
    match v {
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::String(s) => s.clone(),
        v => v.to_string(),
    }
}

fn opt_str(s: Option<String>) -> Value {
    s.map_or(Value::Null, Value::String)
}

pub fn pod_from_row(row: PodRow) -> Pod {
    let mut labels = match row.labels {
        Some(Value::Object(m)) if !m.is_empty() => m,
        _ => Map::new(),
    };
    labels.insert(RESOURCE_LABEL.to_string(), Value::String(row.id));
    let mut metadata = Map::new();
    metadata.insert("name".into(), opt_str(row.name));
    metadata.insert("namespace".into(), opt_str(row.namespace));
    metadata.insert("labels".into(), Value::Object(labels));
    if let Some(a) = row.annotations.filter(truthy) {
        metadata.insert("annotations".into(), a);
    }
    let spec = row.spec.filter(truthy).unwrap_or_else(|| json!({}));
    Pod {
        doc: json!({
            "apiVersion": opt_str(row.api_version),
            "kind": opt_str(row.kind),
            "metadata": metadata,
            "spec": spec,
        }),
    }
}

fn items<'a>(v: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    v.get(key).and_then(Value::as_array).into_iter().flatten()
}

fn name_at(v: &Value, key: &str) -> Option<String> {
    v.get(key).filter(|n| truthy(n)).map(py_str)
}

/// ConfigMap names referenced by volumes and container env `valueFrom`.
pub fn referenced_configmaps(pod: &Value) -> BTreeSet<String> {
    let spec = &pod["spec"];
    let mut names = BTreeSet::new();
    for vol in items(spec, "volumes") {
        names.extend(vol.get("configMap").and_then(|c| name_at(c, "name")));
    }
    for c in items(spec, "containers") {
        for env in items(c, "env") {
            let cm = env.get("valueFrom").and_then(|v| v.get("configMapKeyRef"));
            names.extend(cm.and_then(|c| name_at(c, "name")));
        }
    }
    names
}

/// PersistentVolumeClaim names referenced by volumes.
pub fn referenced_pvcs(pod: &Value) -> BTreeSet<String> {
    items(&pod["spec"], "volumes")
        .filter_map(|v| v.get("persistentVolumeClaim"))
        .filter_map(|p| name_at(p, "claimName"))
        .collect()
}

pub fn default_pvc_spec() -> Value {
    json!({
        "accessModes": ["ReadWriteOnce"],
        "resources": {"requests": {"storage": "1Gi"}},
    })
}

pub fn pvc_doc(name: &str, spec: Value) -> Value {
    json!({
        "apiVersion": "v1",
        "kind": "PersistentVolumeClaim",
        "metadata": {"name": name},
        "spec": spec,
    })
}

pub fn configmap_doc(name: &str, data: Value) -> Value {
    json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {"name": name},
        "data": data,
    })
}

/// The `kind: List` document handed to `podman kube play`.
pub fn kube_list(pvcs: Vec<Value>, pod: Value) -> Value {
    let mut docs = pvcs;
    docs.push(pod);
    json!({"apiVersion": "v1", "kind": "List", "items": docs})
}

/// Parse `podman pod ls --format json` into (resource id, pod name) pairs,
/// keeping first-seen order and letting later duplicates win.
pub fn parse_managed(out: &str) -> Vec<(String, String)> {
    let Ok(Value::Array(pods)) = serde_json::from_str::<Value>(out) else {
        return Vec::new();
    };
    let mut managed: Vec<(String, String)> = Vec::new();
    for pod in &pods {
        let Some(id) = pod
            .get("Labels")
            .and_then(|l| l.get(RESOURCE_LABEL))
            .filter(|v| truthy(v))
            .map(py_str)
        else {
            continue;
        };
        let name = pod.get("Name").map_or_else(String::new, py_str);
        match managed.iter_mut().find(|(k, _)| *k == id) {
            Some(entry) => entry.1 = name,
            None => managed.push((id, name)),
        }
    }
    managed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_pod_document() {
        let pod = pod_from_row(PodRow {
            id: "42".into(),
            api_version: Some("v1".into()),
            kind: Some("Pod".into()),
            name: Some("web".into()),
            namespace: Some("main".into()),
            labels: Some(json!({"app": "web"})),
            annotations: Some(json!({})),
            spec: None,
        });
        assert_eq!(pod.name(), "web");
        assert_eq!(
            pod.doc,
            json!({
                "apiVersion": "v1",
                "kind": "Pod",
                "metadata": {
                    "name": "web",
                    "namespace": "main",
                    "labels": {"app": "web", "mkpod.resource-id": "42"},
                },
                "spec": {},
            })
        );
    }

    #[test]
    fn finds_references() {
        let pod = json!({"spec": {
            "volumes": [
                {"name": "a", "configMap": {"name": "cm1"}},
                {"name": "b", "persistentVolumeClaim": {"claimName": "data"}},
                {"name": "c", "configMap": {}},
            ],
            "containers": [{"env": [
                {"name": "X", "valueFrom": {"configMapKeyRef": {"name": "cm2", "key": "k"}}},
                {"name": "Y", "value": "1"},
            ]}],
        }});
        assert_eq!(
            referenced_configmaps(&pod).into_iter().collect::<Vec<_>>(),
            ["cm1", "cm2"]
        );
        assert_eq!(
            referenced_pvcs(&pod).into_iter().collect::<Vec<_>>(),
            ["data"]
        );
        assert!(referenced_pvcs(&json!({"spec": {}})).is_empty());
    }

    #[test]
    fn parses_podman_listing() {
        let out = r#"[
            {"Name": "a", "Labels": {"mkpod.resource-id": "1"}},
            {"Name": "b", "Labels": null},
            {"Name": "c", "Labels": {"mkpod.resource-id": "2"}},
            {"Name": "d", "Labels": {"mkpod.resource-id": "1"}}
        ]"#;
        assert_eq!(
            parse_managed(out),
            [("1".into(), "d".into()), ("2".into(), "c".into())]
        );
        assert!(parse_managed("not json").is_empty());
    }
}
