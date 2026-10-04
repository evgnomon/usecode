// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `hcloud_csi`: PersistentVolumeClaims on Hetzner Cloud Volumes, through
//! the official CSI driver (the `hcloud/hcloud-csi` chart).
//!
//! The driver only runs on nodes labeled `provider=hetzner`, so nodes from
//! other providers can join the cluster without breaking it. Every Hetzner
//! Cloud node takes volumes, control-plane nodes too.
//!
//! A deployment only names one storage class, `hcloud-volumes`, and needs
//! nothing else: the class keeps its volumes in the primary location, and
//! the scheduler keeps each pod with its volume, so on a Hetzner node in
//! that location. Other locations get `hcloud-volumes-<location>`. None of
//! the classes is the default: k3s keeps `local-path` the default.
//!
//! Variables (`-e` or the user config):
//!
//! * `hcloud_primary_location`: where `hcloud-volumes` puts volumes
//!   (default: the location with the most Hetzner nodes).
//! * `hcloud_csi_version`: the chart version.
//!
//! A node's location is its `hetzner-location` label, which everything else
//! reads. A node without it is looked up once in the Hetzner Cloud API (its
//! server by name, else by IP) and gets the label.
//! The token is the one `uc vm` uses (`HCLOUD_TOKEN`, else `hetzner.prod` in
//! the secrets), else the one already in kube-system/hcloud. It goes
//! straight into the cluster: it is never written to a file.

use crate::configure::ctx::Ctx;
use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::copy;
use crate::configure::vars::Vars;
use crate::kube::cluster::{Cluster, DEFAULT_CLASS, Taint};
use crate::kube::{apply, files_dir, get_json, helm, kubectl, var};
use crate::vm::{self, HetznerServer};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

pub const PROVIDER_KEY: &str = "provider";
pub const PROVIDER: &str = "hetzner";
/// Where a node's location is recorded when no other label has it.
pub const LOCATION_LABEL: &str = "hetzner-location";
/// The topology key the driver puts on every node and volume.
pub const TOPOLOGY_KEY: &str = "csi.hetzner.cloud/location";
const ROOT_SERVER_LABEL: &str = "instance.hetzner.cloud/is-root-server";
const PROVIDED_BY_LABEL: &str = "instance.hetzner.cloud/provided-by";
/// The locations Hetzner Cloud Volumes exist in.
const LOCATIONS: &[&str] = &["fsn1", "nbg1", "hel1", "ash", "hil", "sin"];

const NAMESPACE: &str = "kube-system";
const RELEASE: &str = "hcloud-csi";
const REPO: &str = "hcloud";
const REPO_URL: &str = "https://charts.hetzner.cloud";
const CHART: &str = "hcloud/hcloud-csi";
/// The chart version this role was written against.
const CHART_VERSION: &str = "2.23.0";
const SECRET: &str = "hcloud";
pub const GENERIC_CLASS: &str = "hcloud-volumes";
pub const DRIVER: &str = "csi.hetzner.cloud";

const TEST_NAMESPACE: &str = "csi-test";
const TEST_NAME: &str = "csi-test";
const TEST_FILE: &str = "/data/usecode-csi-test";

/// What the user decided, from the variables.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub primary: Option<String>,
    pub version: String,
}

impl Settings {
    fn load(vars: &Vars) -> Result<Settings> {
        Ok(Settings {
            primary: var(vars, "hcloud_primary_location")?,
            version: var(vars, "hcloud_csi_version")?.unwrap_or_else(|| CHART_VERSION.into()),
        })
    }
}

/// Where a node's location came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The node's `hetzner-location` label.
    Label,
    /// The Hetzner Cloud API, for a node without the label; it gets it.
    Api,
    Unknown,
}

/// A `provider=hetzner` node as the role sees it.
#[derive(Debug, Clone)]
pub struct HNode {
    pub name: String,
    pub control_plane: bool,
    pub location: Option<String>,
    pub source: Source,
    /// Not a server of the Hetzner Cloud project (a dedicated server, or
    /// another project's), so volumes can't reach it.
    pub robot: bool,
    /// The `hetzner-location` label is missing; the API knows it.
    pub needs_label: bool,
    pub taints: Vec<Taint>,
}

/// The location plan, worked out before any task runs.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub settings: Settings,
    pub nodes: Vec<HNode>,
    /// Nodes without `provider=hetzner`, which the driver leaves alone.
    pub others: Vec<String>,
    /// The Hetzner Cloud nodes per location; one storage class each.
    pub classes: BTreeMap<String, Vec<String>>,
    pub primary: Option<String>,
    pub warnings: Vec<String>,
    /// What has to be fixed before anything is installed.
    pub problems: Vec<String>,
}

/// `fsn1` from `fsn1`, `fsn1-dc14` or `FSN1`, if it is a volume location.
pub fn location(raw: &str) -> Option<String> {
    let loc = raw.trim().split('-').next()?.to_ascii_lowercase();
    LOCATIONS.contains(&loc.as_str()).then_some(loc)
}

impl Layout {
    /// `servers` are the project's servers from the Hetzner Cloud API, or
    /// why they could not be listed; then the nodes' labels have to do.
    pub fn new(
        cluster: &Cluster,
        settings: Settings,
        servers: impl FnOnce() -> Result<Vec<HetznerServer>, String>,
    ) -> Layout {
        let mut layout = Layout {
            settings,
            ..Layout::default()
        };
        layout.others = cluster
            .nodes
            .iter()
            .filter(|n| n.label(PROVIDER_KEY) != Some(PROVIDER))
            .map(|n| n.name.clone())
            .collect();
        let hetzner: Vec<_> = cluster
            .nodes
            .iter()
            .filter(|n| n.label(PROVIDER_KEY) == Some(PROVIDER))
            .collect();

        // The label is where a node's location lives; the API is only asked
        // for the nodes that don't carry it yet, and they get it.
        let dedicated = |n: &crate::kube::cluster::Node| {
            n.label(ROOT_SERVER_LABEL) == Some("true")
                || n.label(PROVIDED_BY_LABEL) == Some("robot")
        };
        let unlabeled = hetzner
            .iter()
            .any(|n| !dedicated(n) && n.label(LOCATION_LABEL).is_none());
        let servers = if unlabeled { servers() } else { Ok(Vec::new()) };

        for n in hetzner {
            let (location, source, robot) = match (n.label(LOCATION_LABEL), &servers) {
                _ if dedicated(n) => (None, Source::Unknown, true),
                (Some(l), _) => {
                    if location(l).is_none() {
                        layout.problems.push(format!(
                            "{}: {LOCATION_LABEL}={l} is not a volume location ({}); \
                             `kubectl label node {} {LOCATION_LABEL}-` and run again to \
                             ask the API",
                            n.name,
                            LOCATIONS.join(", "),
                            n.name
                        ));
                    }
                    (location(l), Source::Label, false)
                }
                // Matched by name, else by address: the node may be named
                // differently from its server.
                (None, Ok(servers)) => match servers
                    .iter()
                    .find(|s| s.name == n.name)
                    .or_else(|| {
                        servers
                            .iter()
                            .find(|s| s.ipv4.as_ref().is_some_and(|ip| n.addresses.contains(ip)))
                    })
                    .and_then(|s| location(&s.location))
                {
                    Some(l) => (Some(l), Source::Api, false),
                    None => (None, Source::Unknown, true),
                },
                (None, Err(_)) => (None, Source::Unknown, false),
            };
            if robot {
                layout.warnings.push(format!(
                    "{} is not a server of this Hetzner Cloud project (a dedicated server, or \
                     another project's): volumes can't attach there and the driver skips it",
                    n.name
                ));
            }
            layout.nodes.push(HNode {
                name: n.name.clone(),
                control_plane: n.control_plane,
                needs_label: source == Source::Api,
                location,
                source,
                robot,
                taints: n.taints.clone(),
            });
        }

        let unknown: Vec<&str> = layout
            .nodes
            .iter()
            .filter(|n| !n.robot && n.location.is_none())
            .map(|n| n.name.as_str())
            .collect();
        if let (false, Err(why)) = (unknown.is_empty(), &servers) {
            layout.problems.push(format!(
                "{} has no {LOCATION_LABEL} label and the Hetzner Cloud API can't be asked \
                 for it ({why}). Set HCLOUD_TOKEN, or label it yourself: kubectl label node \
                 NODE {LOCATION_LABEL}=fsn1",
                unknown.join(", ")
            ));
        }

        for n in &layout.nodes {
            if let (false, Some(loc)) = (n.robot, &n.location) {
                layout
                    .classes
                    .entry(loc.clone())
                    .or_default()
                    .push(n.name.clone());
            }
        }

        if layout.nodes.is_empty() {
            layout.problems.push(format!(
                "no node is labeled {PROVIDER_KEY}={PROVIDER}, so the driver would run nowhere"
            ));
        } else if layout.classes.is_empty() && layout.problems.is_empty() {
            layout
                .problems
                .push("no node is a Hetzner Cloud server, which volumes need".into());
        }
        for (loc, nodes) in &layout.classes {
            if nodes.len() < 2 {
                layout.warnings.push(format!(
                    "{loc} has one node ({}): if it goes down, its pods have nowhere to fail \
                     over to, since a volume never leaves {loc}",
                    nodes[0]
                ));
            }
        }

        layout.primary =
            match &layout.settings.primary {
                Some(p) => match location(p).filter(|l| layout.classes.contains_key(l)) {
                    Some(l) => Some(l),
                    None => {
                        layout.problems.push(format!(
                        "hcloud_primary_location '{p}' has no Hetzner Cloud node; pick one of: {}",
                        layout.classes.keys().cloned().collect::<Vec<_>>().join(", ")
                    ));
                        None
                    }
                },
                None => {
                    let most = layout.classes.values().map(Vec::len).max().unwrap_or(0);
                    let top: Vec<&String> = layout
                        .classes
                        .iter()
                        .filter(|(_, n)| n.len() == most)
                        .map(|(l, _)| l)
                        .collect();
                    if top.len() > 1 {
                        layout.warnings.push(format!(
                            "{} have as many nodes each; {} is primary, \
                         -e hcloud_primary_location=... picks another",
                            top.iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(" and "),
                            top[0]
                        ));
                    }
                    top.first().map(|l| l.to_string())
                }
            };
        layout
    }

    /// The storage class of a location: plain `hcloud-volumes` for the
    /// primary one, which is all a deployment needs to know.
    pub fn class(&self, location: &str) -> String {
        if self.primary.as_deref() == Some(location) {
            GENERIC_CLASS.to_string()
        } else {
            format!("{GENERIC_CLASS}-{location}")
        }
    }

    fn location_of(&self, node: &str) -> Option<&str> {
        self.nodes
            .iter()
            .find(|n| n.name == node)
            .and_then(|n| n.location.as_deref())
    }

    /// The taints the driver's pods tolerate: those on Hetzner nodes that
    /// keep pods off. The node plugin gets them all so it runs on every
    /// Hetzner node; the controller only when no Hetzner node is untainted.
    fn tolerations(&self) -> (Vec<Value>, Vec<Value>) {
        let blocking = |t: &&Taint| t.effect == "NoSchedule" || t.effect == "NoExecute";
        let nodes: Vec<&HNode> = self.nodes.iter().filter(|n| !n.robot).collect();
        let taints: BTreeSet<&Taint> = nodes
            .iter()
            .flat_map(|n| n.taints.iter().filter(blocking))
            .collect();
        let node: Vec<Value> = taints
            .iter()
            .map(|t| match &t.value {
                Some(v) => {
                    json!({"key": t.key, "operator": "Equal", "value": v, "effect": t.effect})
                }
                None => json!({"key": t.key, "operator": "Exists", "effect": t.effect}),
            })
            .collect();
        let free = nodes.iter().any(|n| !n.taints.iter().any(|t| blocking(&t)));
        let controller = if free { Vec::new() } else { node.clone() };
        (node, controller)
    }

    /// The chart values: the driver on `provider=hetzner` nodes only. The
    /// chart's own class could land volumes in any location, so it makes
    /// none; [`storage_classes`](Layout::storage_classes) does.
    pub fn values(&self) -> Value {
        let (node_tolerations, controller_tolerations) = self.tolerations();
        let mut controller = json!({"nodeSelector": {PROVIDER_KEY: PROVIDER}});
        let mut node = json!({
            "affinity": {"nodeAffinity": {"requiredDuringSchedulingIgnoredDuringExecution": {
                "nodeSelectorTerms": [{"matchExpressions": [
                    // The chart's own rules: no dedicated servers.
                    {"key": ROOT_SERVER_LABEL, "operator": "NotIn", "values": ["true"]},
                    {"key": PROVIDED_BY_LABEL, "operator": "NotIn", "values": ["robot"]},
                    {"key": PROVIDER_KEY, "operator": "In", "values": [PROVIDER]},
                ]}]
            }}}
        });
        if !controller_tolerations.is_empty() {
            controller["tolerations"] = Value::Array(controller_tolerations);
        }
        if !node_tolerations.is_empty() {
            node["tolerations"] = Value::Array(node_tolerations);
        }
        json!({
            "controller": controller,
            "node": node,
            "storageClasses": [],
        })
    }

    /// One storage class per location, each keeping its volumes there.
    pub fn storage_classes(&self) -> Value {
        let items: Vec<Value> = self
            .classes
            .keys()
            .map(|loc| {
                json!({
                    "apiVersion": "storage.k8s.io/v1",
                    "kind": "StorageClass",
                    "metadata": {
                        "name": self.class(loc),
                        "annotations": {DEFAULT_CLASS: "false"},
                        "labels": {"app.kubernetes.io/managed-by": "usecode"},
                    },
                    "provisioner": DRIVER,
                    "reclaimPolicy": "Delete",
                    "volumeBindingMode": "WaitForFirstConsumer",
                    "allowVolumeExpansion": true,
                    "allowedTopologies": [{"matchLabelExpressions": [
                        {"key": TOPOLOGY_KEY, "values": [loc]}
                    ]}],
                })
            })
            .collect();
        json!({"apiVersion": "v1", "kind": "List", "items": items})
    }

    /// What `hcloud_csi/report` prints: the inspection and the plan.
    pub fn report(&self, cluster: &Cluster) -> Vec<String> {
        let mut lines = vec![format!("context {} at {}", cluster.context, cluster.server)];
        let width = self.nodes.iter().map(|n| n.name.len()).max().unwrap_or(0);
        for n in &self.nodes {
            let role = if n.control_plane {
                "control-plane"
            } else {
                "worker"
            };
            let loc = match (&n.location, &n.source) {
                (Some(l), Source::Label) => l.clone(),
                (Some(l), Source::Api) => {
                    format!("{l} (from the Hetzner Cloud API, gets the label)")
                }
                _ => "location unknown".into(),
            };
            let mut extra = Vec::new();
            if n.robot {
                extra.push("not in the Hetzner Cloud project".to_string());
            }
            if !n.taints.is_empty() {
                let t: Vec<String> = n
                    .taints
                    .iter()
                    .map(|t| {
                        format!(
                            "{}{}:{}",
                            t.key,
                            t.value
                                .as_ref()
                                .map(|v| format!("={v}"))
                                .unwrap_or_default(),
                            t.effect
                        )
                    })
                    .collect();
                extra.push(format!("taints {}", t.join(",")));
            }
            let extra = if extra.is_empty() {
                String::new()
            } else {
                format!("  [{}]", extra.join("; "))
            };
            lines.push(format!("  {:<width$}  {role:<13}  {loc}{extra}", n.name));
        }
        if !self.others.is_empty() {
            lines.push(format!(
                "not Hetzner, the driver stays off: {}",
                self.others.join(", ")
            ));
        }
        for (loc, nodes) in &self.classes {
            let primary = if self.primary.as_deref() == Some(loc) {
                ", primary"
            } else {
                ""
            };
            lines.push(format!(
                "{loc}: {} node(s) ({}) -> storage class {}{primary}",
                nodes.len(),
                nodes.join(", "),
                self.class(loc)
            ));
        }
        let classes: Vec<String> = cluster
            .storage_classes
            .iter()
            .map(|c| format!("{}{}", c.name, if c.default { " (default)" } else { "" }))
            .collect();
        lines.push(format!("storage classes now: {}", or_none(&classes)));
        let claims: Vec<String> = cluster
            .claims
            .iter()
            .map(|c| format!("{}/{} ({}, {})", c.namespace, c.name, c.class, c.phase))
            .collect();
        lines.push(format!("claims now (left alone): {}", or_none(&claims)));
        lines.extend(self.warnings.iter().map(|w| format!("warning: {w}")));
        lines
    }
}

fn or_none(items: &[String]) -> String {
    if items.is_empty() {
        "none".into()
    } else {
        items.join(", ")
    }
}

/// The project's servers, with the token `uc vm` uses, else the one the
/// cluster already keeps in kube-system/hcloud.
fn hetzner_servers(cluster: &Cluster) -> Result<Vec<HetznerServer>, String> {
    let token = match vm::hetzner_token() {
        Ok(token) => token,
        Err(err) => cluster_token(cluster).ok_or_else(|| format!("{err:#}"))?,
    };
    vm::hetzner_servers(&token).map_err(|e| format!("{e:#}"))
}

/// The token in kube-system/hcloud, if the cluster has it.
fn cluster_token(cluster: &Cluster) -> Option<String> {
    use base64::Engine;
    let out = std::process::Command::new("kubectl")
        .args([
            "--context",
            &cluster.context,
            "-n",
            NAMESPACE,
            "get",
            "secret",
            SECRET,
        ])
        .args(["-o", "jsonpath={.data.token}"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let token = base64::engine::general_purpose::STANDARD
        .decode(out.stdout.trim_ascii())
        .ok()?;
    String::from_utf8(token)
        .ok()
        .filter(|t| !t.trim().is_empty())
}

pub fn tasks(plan: &mut Plan, cluster: &Arc<Cluster>, vars: &Vars) -> Result<()> {
    let settings = Settings::load(vars)?;
    let layout = Arc::new(Layout::new(cluster, settings, || hetzner_servers(cluster)));
    let dir = files_dir(vars, cluster);
    let values_file = dir.join("hcloud-csi-values.yaml");
    let classes_file = dir.join("hcloud-storageclasses.yaml");
    let header = format!(
        "# Written by `uc kube configure -t hcloud_csi` for context {}.\n\
         # It is rewritten on every run: change the variables, not this file.\n",
        cluster.context
    );
    let values_yaml = format!("{header}{}", serde_yaml::to_string(&layout.values())?);
    let classes_yaml = format!(
        "{header}{}",
        layout.storage_classes()["items"]
            .as_array()
            .map(|items| items
                .iter()
                .map(serde_yaml::to_string)
                .collect::<Result<Vec<_>, _>>())
            .transpose()?
            .unwrap_or_default()
            .join("---\n")
    );

    let (c, l) = (cluster.clone(), layout.clone());
    plan.add(
        Task::new(
            "hcloud_csi/report",
            "Inspect the nodes and plan the volume locations",
        )
        .tags(&["always", "storage"])
        .run(move |ctx| async move {
            for line in l.report(&c) {
                ctx.note(&line);
            }
            if l.problems.is_empty() {
                return Ok(Outcome::Ok);
            }
            bail!("fix these first:\n- {}", l.problems.join("\n- "))
        }),
    );

    let c = cluster.clone();
    let to_label: Vec<(String, String)> = layout
        .nodes
        .iter()
        .filter(|n| n.needs_label)
        .filter_map(|n| Some((n.name.clone(), n.location.clone()?)))
        .collect();
    plan.add(
        Task::new(
            "hcloud_csi/locations",
            "Label nodes with their Hetzner location",
        )
        .tags(&["storage"])
        .after(["hcloud_csi/report"])
        .when(
            !to_label.is_empty(),
            "every node has its hetzner-location label",
        )
        .run(move |ctx| async move {
            let mut outcome = Outcome::Ok;
            for (node, loc) in to_label {
                let step = kubectl(&ctx, &c)
                    .args(["label", "node", &node])
                    .arg(format!("{LOCATION_LABEL}={loc}"))
                    .run_step()
                    .await?;
                outcome = outcome.and(step);
            }
            Ok(outcome)
        }),
    );

    let c = cluster.clone();
    plan.add(
        Task::new(
            "hcloud_csi/token",
            "Keep the Hetzner Cloud API token in kube-system/hcloud",
        )
        .tags(&["storage"])
        .after(["hcloud_csi/report"])
        .run(move |ctx| async move { token(&ctx, &c).await }),
    );

    let (vf, cf) = (values_file.clone(), classes_file.clone());
    let (vy, cy) = (values_yaml, classes_yaml.clone());
    plan.add(
        Task::new(
            "hcloud_csi/files",
            "Write the chart values and the storage classes",
        )
        .tags(&["storage"])
        .after(["hcloud_csi/report"])
        .run(move |ctx| async move {
            let values = copy::content(&ctx, &vy, &vf, Some(0o644), false).await?;
            let classes = copy::content(&ctx, &cy, &cf, Some(0o644), false).await?;
            ctx.log(&format!("{} and {}", vf.display(), cf.display()));
            Ok(values.and(classes))
        }),
    );

    let version = layout.settings.version.clone();
    plan.add(
        Task::new(
            "hcloud_csi/repo",
            "Add the Hetzner chart repository to helm",
        )
        .tags(&["storage"])
        .run(move |ctx| async move { repo(&ctx, &version).await }),
    );

    let (c, l, vf) = (cluster.clone(), layout.clone(), values_file);
    plan.add(
        Task::new(
            "hcloud_csi/install",
            "Install the Hetzner CSI driver with helm",
        )
        .tags(&["storage"])
        .after([
            "hcloud_csi/locations",
            "hcloud_csi/token",
            "hcloud_csi/files",
            "hcloud_csi/repo",
        ])
        .run(move |ctx| async move { install(&ctx, &c, &l, &vf).await }),
    );

    let c = cluster.clone();
    let manifest = serde_json::to_string(&layout.storage_classes())?;
    plan.add(
        Task::new(
            "hcloud_csi/storage-classes",
            "Apply one storage class per location",
        )
        .tags(&["storage"])
        .after(["hcloud_csi/install"])
        .run(move |ctx| async move {
            apply(&ctx, &c, &manifest).await.map_err(|e| {
                anyhow!(
                    "{e:#}\nA storage class's parameters and topologies can't change in place; \
                         delete the class and run again (its volumes keep working)"
                )
            })
        }),
    );

    let (c, l) = (cluster.clone(), layout.clone());
    plan.add(
        Task::new(
            "hcloud_csi/verify",
            "Check the driver runs where it should and reports locations",
        )
        .tags(&["storage"])
        .after(["hcloud_csi/install", "hcloud_csi/storage-classes"])
        .run(move |ctx| async move { verify(&ctx, &c, &l).await }),
    );

    let (c, l) = (cluster.clone(), layout.clone());
    plan.add(
        Task::new(
            "hcloud_csi/smoke",
            "Smoke test: a 10Gi volume in the primary location",
        )
        .tags(&["never", "smoke"])
        .after(["hcloud_csi/report", "hcloud_csi/verify"])
        .run(move |ctx| async move { smoke(&ctx, &c, &l).await }),
    );

    let (c, l) = (cluster.clone(), layout.clone());
    plan.add(
        Task::new(
            "hcloud_csi/failover",
            "Smoke test: move the pod to another node, keep its data",
        )
        .tags(&["never", "failover"])
        .after(["hcloud_csi/smoke"])
        .run(move |ctx| async move { failover(&ctx, &c, &l).await }),
    );

    let c = cluster.clone();
    plan.add(
        Task::new(
            "hcloud_csi/smoke-clean",
            "Delete the smoke test and its Hetzner Volume",
        )
        .tags(&["never", "smoke-clean"])
        .after(["hcloud_csi/failover"])
        .run(move |ctx| async move {
            if ctx.check() {
                return Ok(Outcome::Changed);
            }
            let out = kubectl(&ctx, &c)
                .args([
                    "delete",
                    "namespace",
                    TEST_NAMESPACE,
                    "--ignore-not-found",
                    "--timeout=5m",
                ])
                .output()
                .await?;
            Ok(Outcome::changed(out.stdout.contains("deleted")))
        }),
    );
    Ok(())
}

async fn token(ctx: &Ctx, cluster: &Cluster) -> Result<Outcome> {
    let found = kubectl(ctx, cluster)
        .args(["-n", NAMESPACE, "get", "secret", SECRET, "-o", "name"])
        .any_code()
        .read_only()
        .output()
        .await?;
    if found.success() {
        return Ok(Outcome::Ok);
    }
    let token = vm::hetzner_token().unwrap_or_default();
    let token = token.trim();
    if token.is_empty() {
        let how = "Create a Read & Write API token in the Hetzner Cloud console (your project, \
                   Security, API tokens) and hand it over in the environment, not in a file:\n  \
                   read -rs HCLOUD_TOKEN && export HCLOUD_TOKEN && uc kube configure";
        if ctx.check() {
            ctx.note(&format!(
                "the secret {NAMESPACE}/{SECRET} is missing. {how}"
            ));
            return Ok(Outcome::Changed);
        }
        bail!("the secret {NAMESPACE}/{SECRET} is missing. {how}");
    }
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    // `create`, not `apply`: apply would keep a copy of the token in the
    // last-applied annotation.
    let secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {"name": SECRET, "namespace": NAMESPACE},
        "stringData": {"token": token},
    });
    kubectl(ctx, cluster)
        .args(["create", "-f", "-"])
        .stdin(secret.to_string())
        .output()
        .await?;
    Ok(Outcome::Changed)
}

async fn repo(ctx: &Ctx, version: &str) -> Result<Outcome> {
    let list = ctx
        .cmd("helm")
        .args(["repo", "list", "-o", "json"])
        .any_code()
        .read_only()
        .output()
        .await?;
    // helm fails when there are no repositories at all.
    let repos: Vec<Value> = if list.success() {
        serde_json::from_str(&list.stdout)?
    } else {
        Vec::new()
    };
    let mut outcome = Outcome::Ok;
    match repos.iter().find(|r| r["name"] == REPO) {
        Some(r) if r["url"].as_str().map(|u| u.trim_end_matches('/')) == Some(REPO_URL) => {}
        Some(r) => bail!(
            "helm repository '{REPO}' points at {}, not {REPO_URL}",
            r["url"]
        ),
        None => {
            outcome = ctx
                .cmd("helm")
                .args(["repo", "add", REPO, REPO_URL])
                .run_step()
                .await?;
            if ctx.check() {
                return Ok(outcome);
            }
        }
    }
    let found = ctx
        .cmd("helm")
        .args(["search", "repo", CHART, "--version", version, "-o", "json"])
        .any_code()
        .read_only()
        .output()
        .await?;
    let known = serde_json::from_str::<Vec<Value>>(&found.stdout).is_ok_and(|v| !v.is_empty());
    if !known {
        let update = ctx
            .cmd("helm")
            .args(["repo", "update", REPO])
            .run_step()
            .await?;
        outcome = outcome.and(update);
    }
    Ok(outcome)
}

async fn install(
    ctx: &Ctx,
    cluster: &Cluster,
    layout: &Layout,
    values_file: &Path,
) -> Result<Outcome> {
    let version = &layout.settings.version;
    let list = helm(ctx, cluster)
        .args([
            "list",
            "-n",
            NAMESPACE,
            "--filter",
            &format!("^{RELEASE}$"),
            "-o",
            "json",
        ])
        .read_only()
        .output()
        .await?;
    let releases: Vec<Value> = serde_json::from_str(&list.stdout)?;
    let deployed = releases.first().is_some_and(|r| {
        r["status"] == "deployed" && r["chart"] == format!("hcloud-csi-{version}")
    });
    if deployed {
        let got = helm(ctx, cluster)
            .args(["get", "values", RELEASE, "-n", NAMESPACE, "-o", "json"])
            .read_only()
            .output()
            .await?;
        let got: Value = serde_json::from_str(&got.stdout)?;
        if got == layout.values() {
            return Ok(Outcome::Ok);
        }
    }
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    let installed = helm(ctx, cluster)
        .args([
            "upgrade",
            "--install",
            RELEASE,
            CHART,
            "-n",
            NAMESPACE,
            "--version",
            version,
        ])
        .arg("-f")
        .arg(values_file.display().to_string())
        .args(["--wait", "--timeout", "5m"])
        .output()
        .await;
    if let Err(err) = installed {
        bail!("{err:#}\n\n{}", driver_logs(ctx, cluster, &[]).await);
    }
    Ok(Outcome::Changed)
}

/// The tail of the driver's logs, for a failure message: the controller's
/// and those of the named node plugin pods.
async fn driver_logs(ctx: &Ctx, cluster: &Cluster, node_pods: &[String]) -> String {
    let mut targets = vec![format!("deployment/{RELEASE}-controller")];
    targets.extend(node_pods.iter().map(|p| format!("pod/{p}")));
    let mut text = String::new();
    for target in targets {
        let out = kubectl(ctx, cluster)
            .args([
                "-n",
                NAMESPACE,
                "logs",
                &target,
                "-c",
                "hcloud-csi-driver",
                "--tail=30",
            ])
            .any_code()
            .read_only()
            .output()
            .await;
        if let Ok(out) = out {
            text.push_str(&format!(
                "logs of {target} (hcloud-csi-driver):\n{}\n",
                out.combined().trim_end()
            ));
        }
    }
    text
}

async fn verify(ctx: &Ctx, cluster: &Cluster, layout: &Layout) -> Result<Outcome> {
    if ctx.check() && ctx.deps_changed() {
        return Ok(Outcome::Skipped("checked once it is installed".into()));
    }
    let mut problems = Vec::new();
    for workload in [
        format!("deployment/{RELEASE}-controller"),
        format!("daemonset/{RELEASE}-node"),
    ] {
        let out = kubectl(ctx, cluster)
            .args([
                "-n",
                NAMESPACE,
                "rollout",
                "status",
                &workload,
                "--timeout=180s",
            ])
            .any_code()
            .read_only()
            .output()
            .await?;
        if !out.success() {
            problems.push(format!(
                "{workload} is not ready: {}",
                out.combined().trim()
            ));
        }
    }

    let expected: BTreeSet<&str> = layout
        .nodes
        .iter()
        .filter(|n| !n.robot)
        .map(|n| n.name.as_str())
        .collect();
    let pods = get_json(
        ctx,
        cluster,
        &[
            "-n",
            NAMESPACE,
            "get",
            "pods",
            "-l",
            &format!("app.kubernetes.io/instance={RELEASE},app.kubernetes.io/component=node"),
        ],
    )
    .await?;
    let mut running = BTreeSet::new();
    let mut failing_pods = Vec::new();
    for pod in pods["items"].as_array().into_iter().flatten() {
        let node = pod["spec"]["nodeName"].as_str().unwrap_or_default();
        let name = pod["metadata"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if !expected.contains(node) {
            problems.push(format!(
                "the node plugin runs on {node}, which is not a Hetzner Cloud node"
            ));
        }
        if pod["status"]["phase"] == "Running" {
            running.insert(node.to_string());
        } else {
            failing_pods.push(name);
        }
    }
    for node in &expected {
        if !running.contains(*node) {
            problems.push(format!("the node plugin is not running on {node}"));
        }
    }

    let csinodes = get_json(ctx, cluster, &["get", "csinodes"]).await?;
    let nodes = get_json(ctx, cluster, &["get", "nodes"]).await?;
    for node in &expected {
        let csinode = csinodes["items"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|n| n["metadata"]["name"] == *node);
        let registered = csinode.is_some_and(|n| {
            n["spec"]["drivers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|d| {
                    d["name"] == DRIVER
                        && d["topologyKeys"]
                            .as_array()
                            .is_some_and(|k| k.iter().any(|k| k == TOPOLOGY_KEY))
                })
        });
        if !registered {
            problems.push(format!(
                "{node}: the driver has not registered with its {TOPOLOGY_KEY} topology"
            ));
            continue;
        }
        let reported = nodes["items"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|n| n["metadata"]["name"] == *node)
            .and_then(|n| {
                n["metadata"]["labels"][TOPOLOGY_KEY]
                    .as_str()
                    .map(str::to_string)
            });
        let planned = layout.location_of(node);
        if reported.as_deref() != planned {
            problems.push(format!(
                "{node}: the driver reports location {}, the plan says {}",
                reported.as_deref().unwrap_or("none"),
                planned.unwrap_or("none")
            ));
        }
    }

    let classes = get_json(ctx, cluster, &["get", "storageclasses"]).await?;
    let mut defaults = Vec::new();
    for class in classes["items"].as_array().into_iter().flatten() {
        let name = class["metadata"]["name"].as_str().unwrap_or_default();
        if class["metadata"]["annotations"][DEFAULT_CLASS] == "true" {
            defaults.push(name.to_string());
            if name.starts_with(GENERIC_CLASS) {
                problems.push(format!("{name} is a default storage class; it must not be"));
            }
        }
        if name.starts_with(GENERIC_CLASS) && name != GENERIC_CLASS {
            let loc = name
                .trim_start_matches(GENERIC_CLASS)
                .trim_start_matches('-');
            if !layout.classes.contains_key(loc) {
                ctx.note(&format!(
                    "{name} is left from an earlier plan; no node for volumes is in {loc} now"
                ));
            }
        }
    }
    let had_local_default = cluster
        .storage_classes
        .iter()
        .any(|c| c.name == "local-path" && c.default);
    if had_local_default && !defaults.iter().any(|d| d == "local-path") {
        problems.push("local-path is no longer the default storage class".into());
    }
    for loc in layout.classes.keys() {
        let name = layout.class(loc);
        if !classes["items"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|c| c["metadata"]["name"] == name.as_str())
        {
            problems.push(format!("the storage class {name} is missing"));
        }
    }

    if problems.is_empty() {
        ctx.log(&format!(
            "driver on {}, default class: {}",
            running.iter().cloned().collect::<Vec<_>>().join(", "),
            or_none(&defaults)
        ));
        return Ok(Outcome::Ok);
    }
    let logs = driver_logs(ctx, cluster, &failing_pods).await;
    bail!("{}\n\n{logs}", problems.join("\n"))
}

/// The smoke test pod: nothing but the claim, as any deployment would be.
fn test_pod() -> Value {
    json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {"name": TEST_NAME, "namespace": TEST_NAMESPACE},
        "spec": {
            "terminationGracePeriodSeconds": 0,
            "containers": [{
                "name": "busybox",
                "image": "busybox:1.37",
                "command": ["sleep", "86400"],
                "volumeMounts": [{"name": "data", "mountPath": "/data"}],
            }],
            "volumes": [{"name": "data", "persistentVolumeClaim": {"claimName": TEST_NAME}}],
        },
    })
}

/// Waits for the test pod and returns the node it runs on.
async fn test_pod_node(ctx: &Ctx, cluster: &Cluster) -> Result<String> {
    let ready = kubectl(ctx, cluster)
        .args([
            "-n",
            TEST_NAMESPACE,
            "wait",
            "--for=condition=Ready",
            &format!("pod/{TEST_NAME}"),
            "--timeout=300s",
        ])
        .any_code()
        .output()
        .await?;
    if !ready.success() {
        let describe = kubectl(ctx, cluster)
            .args(["-n", TEST_NAMESPACE, "describe", "pvc,pod"])
            .any_code()
            .read_only()
            .output()
            .await?;
        bail!(
            "the test pod did not become ready:\n{}",
            describe.combined().trim_end()
        );
    }
    let pod = get_json(
        ctx,
        cluster,
        &["-n", TEST_NAMESPACE, "get", "pod", TEST_NAME],
    )
    .await?;
    Ok(pod["spec"]["nodeName"]
        .as_str()
        .unwrap_or_default()
        .to_string())
}

async fn exec(ctx: &Ctx, cluster: &Cluster, script: &str) -> Result<String> {
    let out = kubectl(ctx, cluster)
        .args([
            "-n",
            TEST_NAMESPACE,
            "exec",
            TEST_NAME,
            "--",
            "sh",
            "-c",
            script,
        ])
        .output()
        .await?;
    Ok(out.stdout.trim().to_string())
}

async fn smoke(ctx: &Ctx, cluster: &Cluster, layout: &Layout) -> Result<Outcome> {
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    let primary = layout
        .primary
        .as_deref()
        .ok_or_else(|| anyhow!("no primary location"))?;
    let class = layout.class(primary);
    let manifest = json!({"apiVersion": "v1", "kind": "List", "items": [
        {"apiVersion": "v1", "kind": "Namespace", "metadata": {"name": TEST_NAMESPACE}},
        {
            "apiVersion": "v1",
            "kind": "PersistentVolumeClaim",
            "metadata": {"name": TEST_NAME, "namespace": TEST_NAMESPACE},
            "spec": {
                "accessModes": ["ReadWriteOnce"],
                "storageClassName": class,
                "resources": {"requests": {"storage": "10Gi"}},
            },
        },
        test_pod(),
    ]});
    apply(ctx, cluster, &manifest.to_string()).await?;
    let node = test_pod_node(ctx, cluster).await?;

    let claim = get_json(
        ctx,
        cluster,
        &["-n", TEST_NAMESPACE, "get", "pvc", TEST_NAME],
    )
    .await?;
    if claim["status"]["phase"] != "Bound" {
        bail!("the claim is {}, not Bound", claim["status"]["phase"]);
    }
    let volume = claim["spec"]["volumeName"].as_str().unwrap_or_default();
    let pv = get_json(ctx, cluster, &["get", "pv", volume]).await?;
    let pinned: Vec<&str> = pv["spec"]["nodeAffinity"]["required"]["nodeSelectorTerms"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|t| t["matchExpressions"].as_array().into_iter().flatten())
        .filter(|e| e["key"] == TOPOLOGY_KEY)
        .flat_map(|e| e["values"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    if pinned != [primary] {
        bail!("volume {volume} is pinned to {pinned:?}, not {primary}");
    }
    let at = layout.location_of(&node).unwrap_or("an unknown location");
    if at != primary {
        bail!("the test pod runs on {node} in {at}, not in {primary}");
    }
    let stamp = format!("usecode {}", std::process::id());
    let read = exec(
        ctx,
        cluster,
        &format!("echo '{stamp}' > {TEST_FILE} && sync && cat {TEST_FILE}"),
    )
    .await?;
    if read != stamp {
        bail!("wrote '{stamp}' to {TEST_FILE}, read back '{read}'");
    }
    ctx.note(&format!(
        "{volume} ({class}) is bound in {primary}, mounted on {node}, and reads back what was written. \
         Next: -t failover moves the pod, -t smoke-clean deletes the test and its Hetzner Volume"
    ));
    Ok(Outcome::Changed)
}

async fn failover(ctx: &Ctx, cluster: &Cluster, layout: &Layout) -> Result<Outcome> {
    let primary = layout.primary.as_deref().unwrap_or_default();
    let nodes = layout.classes.get(primary).map_or(0, Vec::len);
    if nodes < 2 {
        return Ok(Outcome::Skipped(format!(
            "{primary} has {nodes} node(s) for volumes; failover needs 2"
        )));
    }
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    let from = test_pod_node(ctx, cluster).await?;
    let before = exec(ctx, cluster, &format!("cat {TEST_FILE}")).await?;
    let node = get_json(ctx, cluster, &["get", "node", &from]).await?;
    let was_cordoned = node["spec"]["unschedulable"] == true;

    kubectl(ctx, cluster)
        .args(["cordon", &from])
        .output()
        .await?;
    let moved = async {
        kubectl(ctx, cluster)
            .args([
                "-n",
                TEST_NAMESPACE,
                "delete",
                "pod",
                TEST_NAME,
                "--wait=true",
                "--timeout=180s",
            ])
            .output()
            .await?;
        apply(ctx, cluster, &test_pod().to_string()).await?;
        let to = test_pod_node(ctx, cluster).await?;
        let after = exec(ctx, cluster, &format!("cat {TEST_FILE}")).await?;
        anyhow::Ok((to, after))
    }
    .await;
    if !was_cordoned {
        kubectl(ctx, cluster)
            .args(["uncordon", &from])
            .output()
            .await?;
    }
    let (to, after) = moved?;
    let at = layout.location_of(&to).unwrap_or("an unknown location");
    if to == from || at != primary {
        bail!("the pod went to {to} in {at}; expected another node in {primary}");
    }
    if after != before {
        bail!("the data changed on the way: '{before}' before, '{after}' after");
    }
    ctx.note(&format!(
        "moved from {from} to {to} in {primary}; the data came along"
    ));
    Ok(Outcome::Changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kube::cluster::{CONTROL_PLANE, Node};

    fn node(name: &str, cp: bool, labels: &[(&str, &str)]) -> Node {
        let mut l: BTreeMap<String, String> = labels
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        l.insert(PROVIDER_KEY.into(), PROVIDER.into());
        if cp {
            l.insert(CONTROL_PLANE.into(), "true".into());
        }
        Node {
            name: name.into(),
            control_plane: cp,
            labels: l,
            taints: Vec::new(),
            addresses: Vec::new(),
        }
    }

    fn cluster(nodes: Vec<Node>) -> Cluster {
        Cluster {
            context: "test".into(),
            nodes,
            ..Cluster::default()
        }
    }

    fn no_api() -> Result<Vec<HetznerServer>, String> {
        Err("no token".into())
    }

    fn api_unused() -> Result<Vec<HetznerServer>, String> {
        panic!("labeled nodes must not ask the API")
    }

    #[test]
    fn normalizes_locations() {
        assert_eq!(location("fsn1-dc14").as_deref(), Some("fsn1"));
        assert_eq!(location("HEL1").as_deref(), Some("hel1"));
        assert_eq!(location("eu-central"), None);
    }

    #[test]
    fn plans_one_class_per_location() {
        let c = cluster(vec![
            node("cp-1", true, &[(LOCATION_LABEL, "fsn1")]),
            node("w-1", false, &[(LOCATION_LABEL, "fsn1")]),
            node("w-2", false, &[(LOCATION_LABEL, "fsn1")]),
            node("w-3", false, &[(LOCATION_LABEL, "hel1")]),
        ]);
        let l = Layout::new(&c, Settings::default(), api_unused);
        assert!(l.problems.is_empty(), "{:?}", l.problems);
        assert_eq!(l.classes.keys().collect::<Vec<_>>(), ["fsn1", "hel1"]);
        assert_eq!(l.classes["fsn1"], ["cp-1", "w-1", "w-2"]);
        assert_eq!(l.primary.as_deref(), Some("fsn1"));
        assert_eq!(l.class("fsn1"), "hcloud-volumes");
        assert_eq!(l.class("hel1"), "hcloud-volumes-hel1");
        assert!(
            l.warnings
                .iter()
                .any(|w| w.starts_with("hel1 has one node"))
        );
    }

    #[test]
    fn unknown_locations_stop_the_plan() {
        let c = cluster(vec![node("w-1", false, &[])]);
        let l = Layout::new(&c, Settings::default(), no_api);
        assert!(
            l.problems
                .iter()
                .any(|p| p.starts_with("w-1 has no hetzner-location label"))
        );
    }

    fn server(name: &str, location: &str, ip: &str) -> HetznerServer {
        HetznerServer {
            name: name.into(),
            location: location.into(),
            ipv4: Some(ip.into()),
        }
    }

    #[test]
    fn the_api_fills_in_missing_labels_by_name_or_address() {
        let mut renamed = node("k8s-a", false, &[]);
        renamed.addresses = vec!["10.0.0.2".into(), "1.2.3.4".into()];
        let labeled = node("w-0", false, &[(LOCATION_LABEL, "fsn1")]);
        let c = cluster(vec![
            node("w-1", false, &[]),
            renamed,
            node("ax", false, &[]),
            labeled,
        ]);
        let servers = vec![
            server("w-1", "nbg1", "5.6.7.8"),
            server("a", "hel1", "1.2.3.4"),
            server("w-0", "hel1", "9.9.9.9"),
        ];
        let l = Layout::new(&c, Settings::default(), || Ok(servers));
        assert!(l.problems.is_empty(), "{:?}", l.problems);
        let by_name: BTreeMap<&str, &HNode> =
            l.nodes.iter().map(|n| (n.name.as_str(), n)).collect();
        assert_eq!(by_name["w-1"].location.as_deref(), Some("nbg1"));
        assert_eq!(by_name["k8s-a"].location.as_deref(), Some("hel1"));
        assert!(by_name["w-1"].needs_label && by_name["k8s-a"].needs_label);
        assert_eq!(
            by_name["w-0"].location.as_deref(),
            Some("fsn1"),
            "the label wins"
        );
        assert!(!by_name["w-0"].needs_label);
        assert!(by_name["ax"].robot, "not in the project");
    }

    #[test]
    fn control_plane_nodes_take_volumes() {
        let c = cluster(vec![node("cp-1", true, &[(LOCATION_LABEL, "fsn1")])]);
        let l = Layout::new(&c, Settings::default(), no_api);
        assert!(l.problems.is_empty(), "{:?}", l.problems);
        assert_eq!(l.primary.as_deref(), Some("fsn1"));
    }

    #[test]
    fn robots_are_left_out() {
        let c = cluster(vec![
            node("w-1", false, &[(LOCATION_LABEL, "fsn1")]),
            node("ax", false, &[(ROOT_SERVER_LABEL, "true")]),
        ]);
        let l = Layout::new(&c, Settings::default(), no_api);
        assert!(l.problems.is_empty(), "{:?}", l.problems);
        assert_eq!(l.classes["fsn1"], ["w-1"]);
    }

    #[test]
    fn values_keep_the_driver_on_hetzner() {
        let mut tainted = node("w-1", false, &[(LOCATION_LABEL, "fsn1")]);
        tainted.taints.push(Taint {
            key: "dedicated".into(),
            value: Some("db".into()),
            effect: "NoSchedule".into(),
        });
        let c = cluster(vec![
            tainted,
            node("w-2", false, &[(LOCATION_LABEL, "fsn1")]),
        ]);
        let v = Layout::new(&c, Settings::default(), no_api).values();
        assert_eq!(v["controller"]["nodeSelector"][PROVIDER_KEY], PROVIDER);
        assert!(v["controller"].get("tolerations").is_none());
        assert_eq!(v["node"]["tolerations"][0]["value"], "db");
        let terms = &v["node"]["affinity"]["nodeAffinity"]["requiredDuringSchedulingIgnoredDuringExecution"]
            ["nodeSelectorTerms"][0]["matchExpressions"];
        assert_eq!(terms.as_array().unwrap().len(), 3);
        assert_eq!(v["storageClasses"], json!([]));
    }

    #[test]
    fn storage_classes_are_pinned_to_their_location() {
        let c = cluster(vec![node("w-1", false, &[(LOCATION_LABEL, "fsn1")])]);
        let classes = Layout::new(&c, Settings::default(), no_api).storage_classes();
        let sc = &classes["items"][0];
        assert_eq!(sc["metadata"]["name"], "hcloud-volumes");
        assert_eq!(sc["metadata"]["annotations"][DEFAULT_CLASS], "false");
        assert_eq!(
            sc["allowedTopologies"][0]["matchLabelExpressions"][0]["values"][0],
            "fsn1"
        );
    }

    #[test]
    fn a_wrong_primary_is_a_problem() {
        let c = cluster(vec![node("w-1", false, &[(LOCATION_LABEL, "fsn1")])]);
        let s = Settings {
            primary: Some("hel1".into()),
            ..Settings::default()
        };
        assert!(!Layout::new(&c, s, no_api).problems.is_empty());
    }
}
