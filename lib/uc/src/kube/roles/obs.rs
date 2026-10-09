// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `obs`: metrics, logs and alerts for the cluster, with no Prometheus
//! server and no UI: everything is queryable over the Kubernetes API, so an
//! agent with kubectl can read it.
//!
//! ```text
//! node-exporter      ─┐
//! kube-state-metrics ─┼─► VictoriaMetrics ◄── vmalert ──► your webhook
//! your apps /metrics ─┘   (scrapes + stores)
//!
//! container logs ──► victoria-logs-collector ──► VictoriaLogs
//! ```
//!
//! Six helm releases in the `obs` namespace, from the `vm` and
//! `prometheus-community` chart repositories. VictoriaMetrics and
//! VictoriaLogs keep their data on Hetzner Cloud Volumes from
//! [`hcloud_csi`](super::hcloud_csi), in two different locations, so losing
//! a location loses metrics or logs, never both. Metrics go to the primary
//! location (`hcloud-volumes`), logs to the next location with the most
//! Hetzner Cloud workers (`hcloud-volumes-<location>`). Each store can move
//! to any Hetzner worker in its location and keep its data. They run on
//! worker nodes only; the two collectors run on every node. Removing a
//! release leaves its volume; only deleting the claim deletes it.
//!
//! VictoriaMetrics scrapes the apiserver, the kubelets (cAdvisor too), and
//! every Service or Pod with the `prometheus.io/scrape: "true"` and
//! `prometheus.io/port` annotations, so your own apps only need those two.
//!
//! Variables (`-e` or the user config):
//!
//! * `obs_enabled`: set it to true to have the role. Without it the role
//!   does nothing.
//! * `obs_metrics_size`, `obs_logs_size`: the volume sizes (default: 10Gi,
//!   Hetzner's smallest). They can grow later, never shrink.
//! * `obs_metrics_retention`, `obs_logs_retention`: how long to keep them
//!   (default: 30d and 14d). Logs also make room by themselves when their
//!   volume is 80% full.
//! * `obs_metrics_location`, `obs_logs_location`: where each store keeps
//!   its volume, e.g. `fsn1` and `hel1`. Both need a Hetzner Cloud worker.
//!   With a single such location, set `obs_logs_location` to it to keep
//!   both stores there; the role won't do that on its own.
//! * `obs_alert_webhook`: where vmalert sends firing alerts, as
//!   Alertmanager would get them: a POST of a JSON list of alerts to
//!   `<url>/api/v2/alerts`. Without it, alerts are only evaluated, and kept
//!   in VictoriaMetrics as the `ALERTS` series.
//! * `obs_alert_groups`: more vmalert rule groups, in the Prometheus rules
//!   format, next to the built-in `cluster` group.

use crate::configure::ctx::Ctx;
use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::copy;
use crate::configure::vars::Vars;
use crate::kube::cluster::{CONTROL_PLANE, Cluster};
use crate::kube::roles::forgejo::bytes;
use crate::kube::roles::hcloud_csi::{
    self, DRIVER, GENERIC_CLASS, Layout, PROVIDER, PROVIDER_KEY, TOPOLOGY_KEY,
};
use crate::kube::{apply, files_dir, get_json, helm, helm_repo, kubectl, unset, var};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const NAMESPACE: &str = "obs";
const VM_REPO: &str = "vm";
const VM_REPO_URL: &str = "https://victoriametrics.github.io/helm-charts/";
const PROM_REPO: &str = "prometheus-community";
const PROM_REPO_URL: &str = "https://prometheus-community.github.io/helm-charts";

const METRICS: &str = "vmsingle";
const METRICS_CLAIM: &str = "vmsingle-data";
const METRICS_PORT: u16 = 8428;
const LOGS: &str = "vlsingle";
const LOGS_CLAIM: &str = "vlsingle-data";
const LOGS_PORT: u16 = 9428;
const ALERTS_PORT: u16 = 8880;

/// One helm release of the stack.
#[derive(Debug, Clone)]
pub struct Release {
    /// The release name, also its resources' name (`fullnameOverride`).
    pub name: &'static str,
    /// `<repo>/<chart>`.
    pub chart: &'static str,
    /// The chart version this role was written against.
    pub version: &'static str,
    /// The task that installs it, `obs/<step>`.
    pub step: &'static str,
    pub title: &'static str,
    /// Whether it runs on every node (a DaemonSet) rather than once.
    pub daemon: bool,
}

pub const RELEASES: &[Release] = &[
    Release {
        name: "node-exporter",
        chart: "prometheus-community/prometheus-node-exporter",
        version: "4.59.0",
        step: "node-exporter",
        title: "Run node-exporter on every node",
        daemon: true,
    },
    Release {
        name: "kube-state-metrics",
        chart: "prometheus-community/kube-state-metrics",
        version: "8.6.0",
        step: "kube-state-metrics",
        title: "Run kube-state-metrics",
        daemon: false,
    },
    Release {
        name: METRICS,
        chart: "vm/victoria-metrics-single",
        version: "0.48.0",
        step: "metrics",
        title: "Run VictoriaMetrics on its Hetzner Volume",
        daemon: false,
    },
    Release {
        name: "vmalert",
        chart: "vm/victoria-metrics-alert",
        version: "0.50.0",
        step: "alerts",
        title: "Run vmalert against VictoriaMetrics",
        daemon: false,
    },
    Release {
        name: LOGS,
        chart: "vm/victoria-logs-single",
        version: "0.13.10",
        step: "logs",
        title: "Run VictoriaLogs on its Hetzner Volume",
        daemon: false,
    },
    Release {
        name: "vl-collector",
        chart: "vm/victoria-logs-collector",
        version: "0.3.8",
        step: "log-collector",
        title: "Collect container logs on every node",
        daemon: true,
    },
];

impl Release {
    /// The chart's name, as `helm list` shows it with its version.
    fn chart_name(&self) -> &str {
        self.chart.split_once('/').map_or(self.chart, |(_, c)| c)
    }
}

/// What the user decided, from the variables.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub enabled: bool,
    pub metrics_size: String,
    pub logs_size: String,
    pub metrics_retention: String,
    pub logs_retention: String,
    pub placement: Placement,
    pub webhook: Option<String>,
    pub alert_groups: Vec<Value>,
}

/// Where each store keeps its volume, from the hcloud_csi [`Layout`].
#[derive(Debug, Clone, Default)]
pub struct Placement {
    pub metrics: Option<String>,
    pub logs: Option<String>,
    pub metrics_class: String,
    pub logs_class: String,
    /// The Hetzner Cloud workers per location, where the stores can run.
    pub workers: BTreeMap<String, Vec<String>>,
    /// What has to be fixed before anything is installed.
    pub problems: Vec<String>,
}

impl Placement {
    /// Metrics in `metrics` (default: the primary location), logs in `logs`
    /// (default: the location with the most workers after that one).
    pub fn new(layout: &Layout, metrics: Option<&str>, logs: Option<&str>) -> Placement {
        let mut p = Placement::default();
        for n in layout.nodes.iter().filter(|n| !n.robot && !n.control_plane) {
            if let Some(loc) = &n.location {
                p.workers
                    .entry(loc.clone())
                    .or_default()
                    .push(n.name.clone());
            }
        }
        let known = p.workers.keys().cloned().collect::<Vec<_>>().join(", ");
        let mut asked = |var: &str, raw: Option<&str>| -> Option<Option<String>> {
            let raw = raw?;
            match hcloud_csi::location(raw).filter(|l| p.workers.contains_key(l)) {
                Some(l) => Some(Some(l)),
                None => {
                    p.problems.push(format!(
                        "{var} '{raw}' has no Hetzner Cloud worker; pick one of: {known}"
                    ));
                    Some(None)
                }
            }
        };
        let metrics_asked = asked("obs_metrics_location", metrics);
        let logs_asked = asked("obs_logs_location", logs);
        // The locations with the most workers first, then by name.
        let mut ranked: Vec<&String> = p.workers.keys().collect();
        ranked.sort_by_key(|l| std::cmp::Reverse(p.workers[*l].len()));
        p.metrics = metrics_asked.unwrap_or_else(|| {
            layout
                .primary
                .clone()
                .filter(|l| p.workers.contains_key(l))
                .or_else(|| ranked.first().map(|l| l.to_string()))
        });
        p.logs = logs_asked.unwrap_or_else(|| {
            ranked
                .iter()
                .find(|l| Some(l.as_str()) != p.metrics.as_deref())
                .map(|l| l.to_string())
        });
        if logs.is_none()
            && p.logs.is_none()
            && let Some(m) = &p.metrics
        {
            p.problems.push(format!(
                "{m} is the only location with Hetzner Cloud workers, and logs go to another \
                 one than metrics: add a worker in another location, or keep both in {m} \
                 with obs_logs_location: {m}"
            ));
        }
        if p.workers.is_empty() {
            p.problems.push(
                "VictoriaMetrics and VictoriaLogs run on Hetzner Cloud workers only, and \
                 there is none"
                    .into(),
            );
        }
        let class = |loc: &Option<String>| {
            loc.as_deref()
                .map_or_else(|| GENERIC_CLASS.to_string(), |l| layout.class(l))
        };
        p.metrics_class = class(&p.metrics);
        p.logs_class = class(&p.logs);
        p
    }
}

impl Settings {
    pub fn load(vars: &Vars, layout: &Layout) -> Result<Settings> {
        let webhook: Option<String> = var(vars, "obs_alert_webhook")?;
        let webhook = webhook
            .map(|w| w.trim().trim_end_matches('/').to_string())
            .filter(|w| !w.is_empty());
        if let Some(w) = &webhook
            && !w.starts_with("http://")
            && !w.starts_with("https://")
        {
            bail!("obs_alert_webhook must be an http:// or https:// URL, not {w}");
        }
        Ok(Settings {
            enabled: var(vars, "obs_enabled")?.unwrap_or(false),
            metrics_size: var(vars, "obs_metrics_size")?.unwrap_or_else(|| "10Gi".into()),
            logs_size: var(vars, "obs_logs_size")?.unwrap_or_else(|| "10Gi".into()),
            metrics_retention: var(vars, "obs_metrics_retention")?.unwrap_or_else(|| "30d".into()),
            logs_retention: var(vars, "obs_logs_retention")?.unwrap_or_else(|| "14d".into()),
            placement: Placement::new(
                layout,
                var::<String>(vars, "obs_metrics_location")?.as_deref(),
                var::<String>(vars, "obs_logs_location")?.as_deref(),
            ),
            webhook,
            alert_groups: var(vars, "obs_alert_groups")?.unwrap_or_default(),
        })
    }

    /// The two claims, at these sizes.
    pub fn claims(&self, metrics_size: &str, logs_size: &str) -> Value {
        let claim = |name: &str, class: &str, size: &str| {
            json!({
                "apiVersion": "v1",
                "kind": "PersistentVolumeClaim",
                "metadata": {"name": name, "namespace": NAMESPACE},
                "spec": {
                    "accessModes": ["ReadWriteOnce"],
                    "storageClassName": class,
                    "resources": {"requests": {"storage": size}},
                },
            })
        };
        json!({
            "apiVersion": "v1",
            "kind": "List",
            "items": [
                claim(METRICS_CLAIM, &self.placement.metrics_class, metrics_size),
                claim(LOGS_CLAIM, &self.placement.logs_class, logs_size),
            ],
        })
    }

    /// The chart values of `release`.
    pub fn values(&self, release: &Release) -> Value {
        // Workers only, and Hetzner Cloud ones, which volumes reach.
        let on_volume_workers = json!({"nodeAffinity": {"requiredDuringSchedulingIgnoredDuringExecution": {
            "nodeSelectorTerms": [{"matchExpressions": [
                {"key": CONTROL_PLANE, "operator": "DoesNotExist"},
                {"key": PROVIDER_KEY, "operator": "In", "values": [PROVIDER]},
            ]}]
        }}});
        let scrape = |port: u16| json!({"prometheus.io/scrape": "true", "prometheus.io/port": port.to_string()});
        // The exporter charts annotate their Service for scraping already;
        // the default scrape config also wants the port.
        let port = |port: u16| json!({"prometheus.io/port": port.to_string()});
        let metrics_url = format!("http://{METRICS}:{METRICS_PORT}");
        match release.name {
            "node-exporter" => json!({
                "fullnameOverride": release.name,
                // Control planes too, whatever their taints.
                "tolerations": [{"operator": "Exists"}],
                "service": {"annotations": port(9100)},
            }),
            "kube-state-metrics" => json!({
                "fullnameOverride": release.name,
                "service": {"annotations": port(8080)},
            }),
            METRICS => json!({
                "server": {
                    "fullnameOverride": METRICS,
                    // A Deployment on our own claim, which outlives the release.
                    "mode": "deployment",
                    "retentionPeriod": self.metrics_retention,
                    "persistentVolume": {"enabled": true, "existingClaim": METRICS_CLAIM},
                    "scrape": {"enabled": true},
                    "affinity": on_volume_workers,
                    "resources": {
                        "requests": {"cpu": "100m", "memory": "256Mi"},
                        "limits": {"memory": "1Gi"},
                    },
                },
            }),
            "vmalert" => {
                let mut server = json!({
                    "fullnameOverride": release.name,
                    "datasource": {"url": metrics_url},
                    // Alert state survives restarts, and firing alerts are
                    // queryable as the ALERTS series.
                    "remoteWrite": {"url": metrics_url},
                    "remoteRead": {"url": metrics_url},
                    "service": {"annotations": scrape(ALERTS_PORT)},
                    "config": {"alerts": {"groups": self.rule_groups()}},
                    "resources": {
                        "requests": {"cpu": "20m", "memory": "64Mi"},
                        "limits": {"memory": "256Mi"},
                    },
                });
                match &self.webhook {
                    Some(url) => server["notifier"] = json!({"url": url}),
                    None => server["extraArgs"] = json!({"notifier.blackhole": true}),
                }
                json!({"server": server})
            }
            LOGS => json!({
                "server": {
                    "fullnameOverride": LOGS,
                    "mode": "deployment",
                    "retentionPeriod": self.logs_retention,
                    // Old logs go before the volume fills up.
                    "retentionMaxDiskUsagePercent": 80,
                    "persistentVolume": {"enabled": true, "existingClaim": LOGS_CLAIM},
                    "affinity": on_volume_workers,
                    "service": {"annotations": scrape(LOGS_PORT)},
                    "resources": {
                        "requests": {"cpu": "50m", "memory": "128Mi"},
                        "limits": {"memory": "1Gi"},
                    },
                },
            }),
            "vl-collector" => json!({
                "fullnameOverride": release.name,
                "remoteWrite": [{"url": format!("http://{LOGS}:{LOGS_PORT}")}],
                "tolerations": [{"operator": "Exists"}],
            }),
            other => unreachable!("no values for {other}"),
        }
    }

    /// The built-in `cluster` group, then `obs_alert_groups`.
    pub fn rule_groups(&self) -> Vec<Value> {
        let rule = |alert: &str, expr: &str, wait: &str, summary: &str| {
            json!({
                "alert": alert,
                "expr": expr,
                "for": wait,
                "labels": {"severity": "warning"},
                "annotations": {"summary": summary},
            })
        };
        let cluster = json!({
            "name": "cluster",
            "rules": [
                rule("TargetDown", "up == 0", "5m",
                    "{{ $labels.job }} target {{ $labels.instance }} is down"),
                rule("NodeNotReady",
                    "kube_node_status_condition{condition=\"Ready\",status=\"true\"} == 0", "5m",
                    "node {{ $labels.node }} is not ready"),
                rule("NodeDiskFilling",
                    "node_filesystem_avail_bytes{fstype!~\"tmpfs|overlay|squashfs\"} \
                     / node_filesystem_size_bytes < 0.1", "10m",
                    "{{ $labels.mountpoint }} on {{ $labels.instance }} has less than 10% free"),
                rule("NodeMemoryLow",
                    "node_memory_MemAvailable_bytes / node_memory_MemTotal_bytes < 0.1", "10m",
                    "{{ $labels.instance }} has less than 10% memory available"),
                rule("VolumeFilling",
                    "kubelet_volume_stats_available_bytes / kubelet_volume_stats_capacity_bytes < 0.1",
                    "10m",
                    "volume {{ $labels.namespace }}/{{ $labels.persistentvolumeclaim }} has less than 10% free"),
                rule("PodCrashLooping",
                    "increase(kube_pod_container_status_restarts_total[15m]) > 3", "0m",
                    "{{ $labels.namespace }}/{{ $labels.pod }} ({{ $labels.container }}) keeps restarting"),
                rule("PodNotRunning",
                    "sum by (namespace, pod) (kube_pod_status_phase{phase=~\"Pending|Unknown|Failed\"}) > 0",
                    "15m",
                    "{{ $labels.namespace }}/{{ $labels.pod }} has not been running for 15 minutes"),
                rule("DeploymentUnavailable",
                    "kube_deployment_status_replicas_available < kube_deployment_spec_replicas", "15m",
                    "{{ $labels.namespace }}/{{ $labels.deployment }} is missing replicas"),
            ],
        });
        std::iter::once(cluster)
            .chain(self.alert_groups.iter().cloned())
            .collect()
    }
}

pub fn tasks(plan: &mut Plan, cluster: &Arc<Cluster>, vars: &Vars, layout: &Layout) -> Result<()> {
    let settings = Arc::new(Settings::load(vars, layout)?);
    let start = plan.tasks().len();
    let dir = files_dir(vars, cluster);
    let header = format!(
        "# Written by `uc kube configure -t obs` for context {}.\n\
         # It is rewritten on every run: change the variables, not this file.\n",
        cluster.context
    );
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    for r in RELEASES {
        let yaml = serde_yaml::to_string(&settings.values(r))?;
        files.push((values_file(&dir, r), format!("{header}{yaml}")));
    }

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "obs/report",
            "Plan metrics, logs and alerts: volumes and webhook",
        )
        .tags(&["obs"])
        .run(move |ctx| async move { report(&ctx, &c, &s).await }),
    );

    let c = cluster.clone();
    let namespace = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {"name": NAMESPACE},
    })
    .to_string();
    plan.add(
        Task::new("obs/namespace", "Create the obs namespace")
            .tags(&["obs"])
            .after(["obs/report"])
            .run(move |ctx| async move { apply(&ctx, &c, &namespace).await }),
    );

    plan.add(
        Task::new(
            "obs/repos",
            "Add the VictoriaMetrics and prometheus-community chart repositories to helm",
        )
        .tags(&["obs"])
        .run(move |ctx| async move {
            // One after the other: both write helm's repositories.yaml.
            let mut outcome = Outcome::Ok;
            for (name, url) in [(VM_REPO, VM_REPO_URL), (PROM_REPO, PROM_REPO_URL)] {
                let r = RELEASES
                    .iter()
                    .find(|r| r.chart.starts_with(&format!("{name}/")))
                    .expect("a release from each repository");
                outcome = outcome.and(helm_repo(&ctx, name, url, r.chart, r.version).await?);
            }
            Ok(outcome)
        }),
    );

    plan.add(
        Task::new("obs/files", "Write the chart values")
            .tags(&["obs"])
            .after(["obs/report"])
            .run(move |ctx| async move {
                let mut outcome = Outcome::Ok;
                for (path, content) in &files {
                    outcome =
                        outcome.and(copy::content(&ctx, content, path, Some(0o644), false).await?);
                }
                ctx.log(&dir.join("obs-*-values.yaml").display().to_string());
                Ok(outcome)
            }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "obs/volumes",
            "Claim the Hetzner Volumes for metrics and logs",
        )
        .tags(&["obs"])
        .after([
            "obs/namespace",
            // The volumes need the driver and the storage class.
            "hcloud_csi/storage-classes",
        ])
        .run(move |ctx| async move { volumes(&ctx, &c, &s).await }),
    );

    let base = files_dir(vars, cluster);
    for r in RELEASES {
        let mut after = vec!["obs/namespace", "obs/repos", "obs/files"];
        match r.name {
            METRICS | LOGS => after.push("obs/volumes"),
            // Its datasource has to answer before it starts evaluating.
            "vmalert" => after.push("obs/metrics"),
            // Nowhere to send logs before VictoriaLogs is there.
            "vl-collector" => after.push("obs/logs"),
            _ => {}
        }
        let (c, s, vf) = (cluster.clone(), settings.clone(), values_file(&base, r));
        let release = r.clone();
        plan.add(
            Task::new(format!("obs/{}", r.step), r.title)
                .tags(&["obs"])
                .after(after)
                .run(move |ctx| async move { install(&ctx, &c, &s, &release, &vf).await }),
        );
    }

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "obs/verify",
            "Check metrics and logs come in and stay on their Hetzner Volumes",
        )
        .tags(&["obs"])
        .after(RELEASES.iter().map(|r| format!("obs/{}", r.step)))
        .run(move |ctx| async move { verify(&ctx, &c, &s).await }),
    );

    plan.gate(
        start,
        settings.enabled,
        &unset(cluster, "obs_enabled", "true"),
    );
    Ok(())
}

fn values_file(dir: &Path, release: &Release) -> PathBuf {
    dir.join(format!("obs-{}-values.yaml", release.name))
}

async fn report(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    let p = &s.placement;
    let at = |loc: &Option<String>| loc.as_deref().unwrap_or("?").to_string();
    ctx.note(&format!(
        "metrics: {} in {} ({}), kept {}",
        s.metrics_size,
        at(&p.metrics),
        p.metrics_class,
        s.metrics_retention
    ));
    ctx.note(&format!(
        "logs: {} in {} ({}), kept {}",
        s.logs_size,
        at(&p.logs),
        p.logs_class,
        s.logs_retention
    ));
    if p.metrics.is_some() && p.metrics == p.logs {
        ctx.note(&format!(
            "warning: metrics and logs are both in {}, as obs_logs_location asks: losing \
             that location loses both",
            at(&p.metrics)
        ));
    }
    ctx.note(&match &s.webhook {
        Some(url) => format!("firing alerts go to {url}/api/v2/alerts"),
        None => "obs_alert_webhook is not set: alerts are evaluated and kept as the ALERTS \
                 series, but sent nowhere"
            .into(),
    });

    let mut problems = p.problems.clone();
    for (store, loc) in [("metrics", &p.metrics), ("logs", &p.logs)] {
        let Some(loc) = loc else { continue };
        match p.workers[loc].as_slice() {
            [only] => ctx.note(&format!(
                "warning: {only} is the only worker in {loc}: if it goes down, {store} wait \
                 for it, since the volume never leaves {loc}"
            )),
            workers => ctx.note(&format!(
                "{store} run on the workers in {loc}: {}",
                workers.join(", ")
            )),
        }
    }

    // A claim's class, and so its location, can't change.
    for (claim, class, var) in [
        (METRICS_CLAIM, &p.metrics_class, "obs_metrics_location"),
        (LOGS_CLAIM, &p.logs_class, "obs_logs_location"),
    ] {
        if let Some(old) = cluster
            .claims
            .iter()
            .find(|c| c.namespace == NAMESPACE && c.name == claim)
            && &old.class != class
        {
            problems.push(format!(
                "{NAMESPACE}/{claim} is in {}, not {class}, and a volume can't move: set \
                 {var} to its location, or delete the claim (and its data) to start over",
                old.class
            ));
        }
    }

    if problems.is_empty() {
        return Ok(Outcome::Ok);
    }
    bail!("fix these first:\n- {}", problems.join("\n- "))
}

async fn volumes(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    // A claim can't shrink: one bigger than asked for keeps its size.
    let mut sizes = Vec::new();
    for (claim, wanted, var) in [
        (METRICS_CLAIM, &s.metrics_size, "obs_metrics_size"),
        (LOGS_CLAIM, &s.logs_size, "obs_logs_size"),
    ] {
        let out = kubectl(ctx, cluster)
            .args(["-n", NAMESPACE, "get", "pvc", claim])
            .args(["-o", "jsonpath={.spec.resources.requests.storage}"])
            .any_code()
            .read_only()
            .output()
            .await?;
        let current = out.stdout.trim().to_string();
        if out.success() && bytes(&current) > bytes(wanted) {
            ctx.note(&format!(
                "{claim} keeps its {current}: a volume grows, never shrinks, so {var}={wanted} \
                 only applies to a new one"
            ));
            sizes.push(current);
        } else {
            sizes.push(wanted.clone());
        }
    }
    apply(ctx, cluster, &s.claims(&sizes[0], &sizes[1]).to_string()).await
}

async fn install(
    ctx: &Ctx,
    cluster: &Cluster,
    s: &Settings,
    release: &Release,
    values_file: &Path,
) -> Result<Outcome> {
    let values = s.values(release);
    let list = helm(ctx, cluster)
        .args(["list", "-n", NAMESPACE, "--filter"])
        .arg(format!("^{}$", release.name))
        .args(["-o", "json"])
        .read_only()
        .output()
        .await?;
    let releases: Vec<Value> = serde_json::from_str(&list.stdout)?;
    let deployed = releases.first().is_some_and(|r| {
        r["status"] == "deployed"
            && r["chart"] == format!("{}-{}", release.chart_name(), release.version)
    });
    if deployed {
        let got = helm(ctx, cluster)
            .args(["get", "values", release.name, "-n", NAMESPACE, "-o", "json"])
            .read_only()
            .output()
            .await?;
        let got: Value = serde_json::from_str(&got.stdout)?;
        if got == values {
            return Ok(Outcome::Ok);
        }
    }
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    let installed = helm(ctx, cluster)
        .args(["upgrade", "--install", release.name, release.chart])
        .args(["-n", NAMESPACE, "--version", release.version])
        .arg("-f")
        .arg(values_file.display().to_string())
        // The first start of a store attaches a new volume.
        .args(["--wait", "--timeout", "10m"])
        .output()
        .await;
    if let Err(err) = installed {
        bail!("{err:#}\n\n{}", logs(ctx, cluster, release).await);
    }
    Ok(Outcome::Changed)
}

/// The kind and name of what runs `release`.
fn workload(release: &Release) -> String {
    let kind = if release.daemon {
        "daemonset"
    } else {
        "deployment"
    };
    format!("{kind}/{}", release.name)
}

/// The tail of a release's logs, and its claim's events for a store, for a
/// failure message.
async fn logs(ctx: &Ctx, cluster: &Cluster, release: &Release) -> String {
    let target = workload(release);
    let mut runs = vec![vec![
        "logs",
        target.as_str(),
        "--all-containers",
        "--tail=30",
    ]];
    match release.name {
        METRICS => runs.push(vec!["describe", "pvc", METRICS_CLAIM]),
        LOGS => runs.push(vec!["describe", "pvc", LOGS_CLAIM]),
        _ => {}
    }
    let mut text = String::new();
    for args in runs {
        let out = kubectl(ctx, cluster)
            .args(["-n", NAMESPACE])
            .args(args.iter().copied())
            .any_code()
            .read_only()
            .output()
            .await;
        if let Ok(out) = out {
            text.push_str(&format!(
                "kubectl {}:\n{}\n",
                args.join(" "),
                out.combined().trim_end()
            ));
        }
    }
    text
}

/// The path under the apiserver that reaches `service`'s `port`, so a
/// query needs nothing but kubectl.
fn proxy(service: &str, port: u16, path: &str) -> String {
    format!("/api/v1/namespaces/{NAMESPACE}/services/{service}:{port}/proxy{path}")
}

async fn verify(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    if ctx.check() && ctx.deps_changed() {
        return Ok(Outcome::Skipped("checked once it is installed".into()));
    }
    let mut problems = Vec::new();
    for r in RELEASES {
        let rollout = kubectl(ctx, cluster)
            .args(["-n", NAMESPACE, "rollout", "status", &workload(r)])
            .arg("--timeout=300s")
            .any_code()
            .read_only()
            .output()
            .await?;
        if !rollout.success() {
            problems.push(format!(
                "{} is not ready: {}",
                workload(r),
                rollout.combined().trim()
            ));
        }
    }

    let p = &s.placement;
    for (claim_name, want) in [
        (METRICS_CLAIM, &p.metrics_class),
        (LOGS_CLAIM, &p.logs_class),
    ] {
        let claim = get_json(ctx, cluster, &["-n", NAMESPACE, "get", "pvc", claim_name]).await?;
        let class = claim["spec"]["storageClassName"]
            .as_str()
            .unwrap_or_default();
        if class != want {
            problems.push(format!(
                "the claim {claim_name} is in class {class}, not {want}"
            ));
        }
        if claim["status"]["phase"] != "Bound" {
            problems.push(format!(
                "the claim {claim_name} is {}, not Bound",
                claim["status"]["phase"].as_str().unwrap_or("unknown")
            ));
        } else if let Some(pv) = claim["spec"]["volumeName"].as_str() {
            let volume = get_json(ctx, cluster, &["get", "pv", pv]).await?;
            let driver = volume["spec"]["csi"]["driver"].as_str().unwrap_or("none");
            if driver != DRIVER {
                problems.push(format!("the volume {pv} comes from {driver}, not {DRIVER}"));
            } else {
                // The location the driver created the volume in.
                let location = volume["spec"]["nodeAffinity"]["required"]["nodeSelectorTerms"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|t| t["matchExpressions"].as_array().into_iter().flatten())
                    .find(|e| e["key"] == TOPOLOGY_KEY)
                    .and_then(|e| e["values"][0].as_str())
                    .unwrap_or("?");
                ctx.log(&format!(
                    "{claim_name} on {pv} in {location} ({})",
                    volume["spec"]["capacity"]["storage"]
                        .as_str()
                        .unwrap_or("?")
                ));
            }
        }
    }

    let raw = |path: String| {
        kubectl(ctx, cluster)
            .args(["get", "--raw", &path])
            .any_code()
            .read_only()
            .output()
    };
    let query = proxy(
        METRICS,
        METRICS_PORT,
        "/api/v1/query?query=count%20by%20(job)%20(up%20%3D%3D%201)",
    );
    let up = raw(query).await?;
    let jobs: Vec<String> = serde_json::from_str::<Value>(&up.stdout)
        .ok()
        .and_then(|v| v["data"]["result"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .map(|r| {
            format!(
                "{} ({})",
                r["metric"]["job"].as_str().unwrap_or("?"),
                r["value"][1].as_str().unwrap_or("?")
            )
        })
        .collect();
    if jobs.is_empty() {
        // The first scrape can take a moment after a fresh install.
        ctx.note(&format!(
            "VictoriaMetrics has no targets up yet: {}",
            up.combined().trim()
        ));
    } else {
        ctx.log(&format!("scraping {}", jobs.join(", ")));
    }

    let logs_query = proxy(
        LOGS,
        LOGS_PORT,
        "/select/logsql/query?query=_time%3A5m&limit=1",
    );
    let recent = raw(logs_query).await?;
    if !recent.success() || recent.stdout.trim().is_empty() {
        ctx.note(&format!(
            "VictoriaLogs has no logs from the last 5 minutes yet: {}",
            recent.combined().trim()
        ));
    } else {
        ctx.log("container logs are coming in");
    }

    if !problems.is_empty() {
        bail!("{}", problems.join("\n"));
    }
    ctx.note(&format!(
        "query metrics: kubectl get --raw '{}'",
        proxy(METRICS, METRICS_PORT, "/api/v1/query?query=up")
    ));
    ctx.note(&format!(
        "query logs: kubectl get --raw '{}'",
        proxy(
            LOGS,
            LOGS_PORT,
            "/select/logsql/query?query=_time:5m%20error&limit=20"
        )
    ));
    Ok(Outcome::Ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(webhook: Option<&str>) -> Settings {
        Settings {
            enabled: true,
            metrics_size: "10Gi".into(),
            logs_size: "10Gi".into(),
            metrics_retention: "30d".into(),
            logs_retention: "14d".into(),
            placement: Placement::new(&layout(&[("w1", "fsn1"), ("w2", "hel1")]), None, None),
            webhook: webhook.map(str::to_string),
            alert_groups: Vec::new(),
        }
    }

    /// Hetzner Cloud workers at these locations, and a control plane in
    /// fsn1, the primary location.
    fn layout(workers: &[(&str, &str)]) -> Layout {
        let node = |name: &str, loc: &str, control_plane: bool| hcloud_csi::HNode {
            name: name.into(),
            control_plane,
            location: Some(loc.into()),
            source: hcloud_csi::Source::Label,
            robot: false,
            needs_label: false,
            taints: Vec::new(),
        };
        let mut nodes = vec![node("cp1", "fsn1", true)];
        nodes.extend(workers.iter().map(|(n, l)| node(n, l, false)));
        Layout {
            nodes,
            primary: Some("fsn1".into()),
            ..Layout::default()
        }
    }

    #[test]
    fn metrics_and_logs_go_to_different_locations() {
        let l = layout(&[
            ("w1", "fsn1"),
            ("w2", "hel1"),
            ("w3", "nbg1"),
            ("w4", "nbg1"),
        ]);
        let p = Placement::new(&l, None, None);
        assert_eq!(p.metrics.as_deref(), Some("fsn1"));
        assert_eq!(p.metrics_class, "hcloud-volumes");
        // The busiest location after the primary.
        assert_eq!(p.logs.as_deref(), Some("nbg1"));
        assert_eq!(p.logs_class, "hcloud-volumes-nbg1");
        assert!(p.problems.is_empty());
    }

    #[test]
    fn the_locations_can_be_chosen() {
        let l = layout(&[("w1", "fsn1"), ("w2", "hel1")]);
        let p = Placement::new(&l, Some("hel1"), Some("FSN1"));
        assert_eq!(p.metrics_class, "hcloud-volumes-hel1");
        assert_eq!(p.logs_class, "hcloud-volumes");
        let p = Placement::new(&l, Some("nbg1"), None);
        assert!(p.problems[0].contains("obs_metrics_location 'nbg1'"));
    }

    #[test]
    fn one_location_only_when_asked() {
        let l = layout(&[("w1", "fsn1"), ("w2", "fsn1")]);
        let p = Placement::new(&l, None, None);
        assert!(p.logs.is_none());
        assert!(p.problems[0].contains("obs_logs_location: fsn1"));
        let p = Placement::new(&l, None, Some("fsn1"));
        assert_eq!(p.logs.as_deref(), Some("fsn1"));
        assert!(p.problems.is_empty());
    }

    #[test]
    fn control_planes_hold_no_store() {
        // cp1 is in fsn1, but no worker is.
        let p = Placement::new(&layout(&[("w1", "hel1"), ("w2", "nbg1")]), None, None);
        assert_eq!(p.metrics.as_deref(), Some("hel1"));
        assert_eq!(p.metrics_class, "hcloud-volumes-hel1");
        assert_eq!(p.logs.as_deref(), Some("nbg1"));
    }

    fn release(name: &str) -> &'static Release {
        RELEASES.iter().find(|r| r.name == name).unwrap()
    }

    #[test]
    fn keeps_the_stores_on_hetzner_volumes() {
        let s = settings(None);
        let claims = s.claims("10Gi", "20Gi");
        let items = claims["items"].as_array().unwrap();
        assert_eq!(items[0]["metadata"]["name"], METRICS_CLAIM);
        assert_eq!(items[1]["spec"]["resources"]["requests"]["storage"], "20Gi");
        assert_eq!(items[0]["spec"]["storageClassName"], "hcloud-volumes");
        assert_eq!(items[1]["spec"]["storageClassName"], "hcloud-volumes-hel1");
        for (name, claim) in [(METRICS, METRICS_CLAIM), (LOGS, LOGS_CLAIM)] {
            let server = &s.values(release(name))["server"];
            assert_eq!(server["mode"], "deployment");
            assert_eq!(server["persistentVolume"]["existingClaim"], claim);
        }
    }

    #[test]
    fn stores_run_on_hetzner_workers_only() {
        let s = settings(None);
        for name in [METRICS, LOGS] {
            let exprs = &s.values(release(name))["server"]["affinity"]["nodeAffinity"]["requiredDuringSchedulingIgnoredDuringExecution"]
                ["nodeSelectorTerms"][0]["matchExpressions"];
            let exprs = exprs.as_array().unwrap();
            assert!(exprs.contains(&json!({"key": CONTROL_PLANE, "operator": "DoesNotExist"})));
            assert!(
                exprs.contains(
                    &json!({"key": PROVIDER_KEY, "operator": "In", "values": [PROVIDER]})
                )
            );
        }
    }

    #[test]
    fn collectors_run_everywhere() {
        let s = settings(None);
        for r in RELEASES.iter().filter(|r| r.daemon) {
            assert_eq!(s.values(r)["tolerations"], json!([{"operator": "Exists"}]));
        }
    }

    #[test]
    fn alerts_go_to_the_webhook_when_there_is_one() {
        let off = settings(None).values(release("vmalert"));
        assert_eq!(off["server"]["extraArgs"]["notifier.blackhole"], true);
        assert!(off["server"].get("notifier").is_none());
        let on = settings(Some("http://agent:8080")).values(release("vmalert"));
        assert_eq!(on["server"]["notifier"]["url"], "http://agent:8080");
        assert!(on["server"].get("extraArgs").is_none());
    }

    #[test]
    fn extra_rule_groups_follow_the_built_in_one() {
        let mut s = settings(None);
        s.alert_groups = vec![json!({"name": "mine", "rules": []})];
        let groups = s.rule_groups();
        assert_eq!(groups[0]["name"], "cluster");
        assert_eq!(groups[1]["name"], "mine");
    }

    #[test]
    fn every_release_has_values() {
        let s = settings(None);
        for r in RELEASES {
            assert!(s.values(r).is_object(), "{}", r.name);
        }
    }
}
