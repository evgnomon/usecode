// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! What `uc kube configure` knows about the cluster before any task runs,
//! read with kubectl from the context it targets: the counterpart of the
//! machine [`Facts`](crate::configure::facts::Facts).

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::process::{Command, Stdio};

/// The label control-plane nodes carry.
pub const CONTROL_PLANE: &str = "node-role.kubernetes.io/control-plane";

/// The annotation that makes a storage class the default.
pub const DEFAULT_CLASS: &str = "storageclass.kubernetes.io/is-default-class";

#[derive(Debug, Clone, Default)]
pub struct Cluster {
    /// The kubeconfig context every kubectl and helm call is pinned to, so
    /// switching contexts during a run cannot redirect it.
    pub context: String,
    /// The API server of that context.
    pub server: String,
    pub nodes: Vec<Node>,
    pub storage_classes: Vec<StorageClass>,
    pub claims: Vec<Claim>,
}

#[derive(Debug, Clone, Default)]
pub struct Node {
    pub name: String,
    pub control_plane: bool,
    pub labels: BTreeMap<String, String>,
    pub taints: Vec<Taint>,
    /// Its internal and external IPs, as the kubelet reports them.
    pub addresses: Vec<String>,
}

impl Node {
    pub fn label(&self, key: &str) -> Option<&str> {
        self.labels.get(key).map(String::as_str)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(default)]
pub struct Taint {
    pub key: String,
    pub value: Option<String>,
    pub effect: String,
}

#[derive(Debug, Clone, Default)]
pub struct StorageClass {
    pub name: String,
    pub provisioner: String,
    pub default: bool,
}

/// A PersistentVolumeClaim, for the report.
#[derive(Debug, Clone, Default)]
pub struct Claim {
    pub namespace: String,
    pub name: String,
    pub class: String,
    pub phase: String,
}

impl Cluster {
    /// Reads the cluster behind `context`, or kubectl's current context.
    pub fn load(context: Option<&str>) -> Result<Cluster> {
        let context = match context {
            Some(c) => c.to_string(),
            None => kubectl(&["config", "current-context"])
                .context(
                    "kubectl has no current context; pass --context or `uc kube connect HOST`",
                )?
                .trim()
                .to_string(),
        };
        let server = kubectl(&[
            "config",
            "view",
            "--minify",
            "--context",
            &context,
            "-o",
            "jsonpath={.clusters[0].cluster.server}",
        ])?
        .trim()
        .to_string();
        let get = |kind: &str, all: bool| -> Result<String> {
            let mut args = vec!["--context", &context, "get", kind, "-o", "json"];
            if all {
                args.push("-A");
            }
            kubectl(&args).with_context(|| format!("reading the {kind} of context {context}"))
        };
        Ok(Cluster {
            nodes: parse_nodes(&get("nodes", false)?)?,
            storage_classes: parse_storage_classes(&get("storageclasses", false)?)?,
            claims: parse_claims(&get("persistentvolumeclaims", true)?)?,
            context,
            server,
        })
    }
}

/// Runs kubectl and returns its stdout, failing with its stderr.
fn kubectl(args: &[&str]) -> Result<String> {
    let out = Command::new("kubectl")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .context("running kubectl; is it installed?")?;
    if !out.status.success() {
        bail!(
            "`kubectl {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[derive(Deserialize)]
struct List<T> {
    items: Vec<T>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Meta {
    name: String,
    namespace: String,
    labels: BTreeMap<String, String>,
    annotations: BTreeMap<String, String>,
}

fn items<T: DeserializeOwned>(json: &str) -> Result<Vec<T>> {
    Ok(serde_json::from_str::<List<T>>(json)?.items)
}

pub fn parse_nodes(json: &str) -> Result<Vec<Node>> {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Spec {
        taints: Vec<Taint>,
    }
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Address {
        address: String,
    }
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Status {
        addresses: Vec<Address>,
    }
    #[derive(Deserialize)]
    struct Raw {
        metadata: Meta,
        #[serde(default)]
        spec: Spec,
        #[serde(default)]
        status: Status,
    }
    let mut nodes: Vec<Node> = items::<Raw>(json)?
        .into_iter()
        .map(|n| Node {
            control_plane: n.metadata.labels.contains_key(CONTROL_PLANE),
            name: n.metadata.name,
            labels: n.metadata.labels,
            taints: n.spec.taints,
            addresses: n.status.addresses.into_iter().map(|a| a.address).collect(),
        })
        .collect();
    nodes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(nodes)
}

fn parse_storage_classes(json: &str) -> Result<Vec<StorageClass>> {
    #[derive(Deserialize)]
    struct Raw {
        metadata: Meta,
        #[serde(default)]
        provisioner: String,
    }
    Ok(items::<Raw>(json)?
        .into_iter()
        .map(|s| StorageClass {
            default: s
                .metadata
                .annotations
                .get(DEFAULT_CLASS)
                .map(String::as_str)
                == Some("true"),
            name: s.metadata.name,
            provisioner: s.provisioner,
        })
        .collect())
}

fn parse_claims(json: &str) -> Result<Vec<Claim>> {
    #[derive(Deserialize, Default)]
    #[serde(default, rename_all = "camelCase")]
    struct Spec {
        storage_class_name: String,
    }
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Status {
        phase: String,
    }
    #[derive(Deserialize)]
    struct Raw {
        metadata: Meta,
        #[serde(default)]
        spec: Spec,
        #[serde(default)]
        status: Status,
    }
    Ok(items::<Raw>(json)?
        .into_iter()
        .map(|c| Claim {
            namespace: c.metadata.namespace,
            name: c.metadata.name,
            class: c.spec.storage_class_name,
            phase: c.status.phase,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nodes_with_roles_and_taints() {
        let json = r#"{"items":[
          {"metadata":{"name":"w","labels":{"provider":"hetzner"}},"spec":{}},
          {"metadata":{"name":"cp","labels":{"node-role.kubernetes.io/control-plane":"true"}},
           "spec":{"taints":[{"key":"k","effect":"NoSchedule"}]}}]}"#;
        let nodes = parse_nodes(json).unwrap();
        assert_eq!(nodes[0].name, "cp");
        assert!(nodes[0].control_plane);
        assert_eq!(nodes[0].taints[0].effect, "NoSchedule");
        assert_eq!(nodes[1].label("provider"), Some("hetzner"));
    }

    #[test]
    fn reads_the_default_storage_class() {
        let json = r#"{"items":[{"metadata":{"name":"local-path","annotations":
          {"storageclass.kubernetes.io/is-default-class":"true"}},"provisioner":"rancher.io/local-path"}]}"#;
        let classes = parse_storage_classes(json).unwrap();
        assert!(classes[0].default);
    }
}
