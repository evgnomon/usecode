// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `dns`: your own authoritative name servers on the cluster, [PowerDNS]
//! (`powerdns/pdns-auth-50`), as two or more instances that share nothing.
//!
//! Each instance `pdns-<n>` is its own deployment with its own SQLite
//! database on its own Hetzner Cloud Volume from
//! [`hcloud_csi`](super::hcloud_csi), so losing one, its volume or its
//! node never touches another. The volumes are spread over the locations
//! with Hetzner Cloud workers, and the instances never share a node. There
//! is no replication between them: [`uc dns`](crate::dns) makes every change
//! on each of them, and `uc dns sync` puts one back in line with another.
//!
//! They answer behind one k3s ServiceLB `LoadBalancer`, `dns/pdns`, on port
//! 53 (UDP and TCP) of every node, like Traefik on 80 and 443: any node
//! passes a query on to any ready instance (`externalTrafficPolicy:
//! Cluster`), wherever it runs. So the name servers to advertise are the
//! nodes your cluster's domain already points at, and where PowerDNS runs
//! doesn't matter. An instance that's down simply gets no queries. Hetzner's
//! own load balancers can't do UDP, so they're not used.
//!
//! The HTTP API has no Service and only answers on the pod's loopback;
//! `uc dns` reaches it with `kubectl port-forward`.
//!
//! [PowerDNS]: https://www.powerdns.com/powerdns-authoritative-server
//!
//! Variables (`-e` or the user config):
//!
//! * `dns_enabled`: on by default, for every cluster; `false` turns the
//!   role off (at the top level for all clusters, or under
//!   `kube.<context>` for one).
//! * `dns_instances`: how many (default: 2, at least 2).
//! * `dns_locations`: where each instance keeps its volume, e.g.
//!   `[fsn1, hel1]` (default: spread over the locations with Hetzner Cloud
//!   workers, the most workers first). A volume never moves, so this only
//!   applies to a new one.
//! * `dns_size`: each volume's size (default: 10Gi, Hetzner's smallest).
//! * `dns_image`: (default: `powerdns/pdns-auth-50:5.0.7`).
//!
//! The API key is made once, at random, and kept in dns/pdns-api.

use crate::configure::ctx::Ctx;
use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::vars::Vars;
use crate::dns::probe;
use crate::kube::cluster::{CONTROL_PLANE, Cluster};
use crate::kube::roles::forgejo::bytes;
use crate::kube::roles::hcloud_csi::{self, GENERIC_CLASS, Layout, PROVIDER, PROVIDER_KEY};
use crate::kube::{apply, get_json, kubectl, var};
use crate::password::{self, Charset};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Arc;

pub const NAMESPACE: &str = "dns";
/// Every instance's pods carry `app.kubernetes.io/name: pdns`.
pub const APP: &str = "pdns";
pub const NAME_LABEL: &str = "app.kubernetes.io/name";
pub const API_SECRET: &str = "pdns-api";
const CONFIG: &str = "pdns-config";
pub const API_PORT: u16 = 8081;
/// PowerDNS listens here inside the pod, so it needs no privileged port;
/// the service answers on 53.
const DNS_PORT: u16 = 5353;
const IMAGE: &str = "powerdns/pdns-auth-50:5.0.7";
/// The image runs as this user; the volume is made writable for it.
const UID: u32 = 953;
const DATA: &str = "/var/lib/powerdns";
const DB: &str = "/var/lib/powerdns/pdns.sqlite3";
const SCHEMA: &str = "/usr/local/share/doc/pdns/schema.sqlite3.sql";
/// The service every instance answers behind.
pub const SERVICE: &str = "pdns";
const HOSTNAME: &str = "kubernetes.io/hostname";
/// The pod annotation that restarts an instance when its configuration
/// changes.
const CONFIG_ANNOTATION: &str = "usecode.dev/config";

/// The `n`th instance (from 1): `pdns-<n>`.
pub fn instance(n: usize) -> String {
    format!("{APP}-{n}")
}

fn claim(n: usize) -> String {
    format!("{}-data", instance(n))
}

/// What the user decided, from the variables.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub enabled: bool,
    pub instances: usize,
    pub size: String,
    pub image: String,
    /// The storage class of each instance's volume, when it is new.
    pub classes: Vec<String>,
    /// The location of each instance's volume, when it is new.
    pub locations: Vec<Option<String>>,
    /// The Hetzner Cloud workers per location, where the instances can run.
    pub workers: BTreeMap<String, Vec<String>>,
    /// What has to be fixed before anything is installed.
    pub problems: Vec<String>,
}

impl Settings {
    pub fn load(vars: &Vars, layout: &Layout) -> Result<Settings> {
        let locations: Vec<String> = var(vars, "dns_locations")?.unwrap_or_default();
        let mut s = Settings {
            enabled: var(vars, "dns_enabled")?.unwrap_or(true),
            instances: var(vars, "dns_instances")?.unwrap_or(2),
            size: var(vars, "dns_size")?.unwrap_or_else(|| "10Gi".into()),
            image: var(vars, "dns_image")?.unwrap_or_else(|| IMAGE.into()),
            ..Settings::default()
        };
        s.place(layout, &locations);
        Ok(s)
    }

    pub fn count(&self) -> usize {
        self.instances
    }

    /// Spreads the volumes over the locations with Hetzner Cloud workers,
    /// the busiest first, unless `asked` names them.
    fn place(&mut self, layout: &Layout, asked: &[String]) {
        for n in layout.nodes.iter().filter(|n| !n.robot && !n.control_plane) {
            if let Some(loc) = &n.location {
                self.workers
                    .entry(loc.clone())
                    .or_default()
                    .push(n.name.clone());
            }
        }
        let mut ranked: Vec<&String> = self.workers.keys().collect();
        // The most workers first, the primary location on a tie.
        ranked.sort_by_key(|l| {
            (
                std::cmp::Reverse(self.workers[*l].len()),
                layout.primary.as_ref() != Some(*l),
            )
        });
        let known = self.workers.keys().cloned().collect::<Vec<_>>().join(", ");
        if !asked.is_empty() && asked.len() != self.count() {
            self.problems.push(format!(
                "dns_locations has {} locations for {} instances (dns_instances)",
                asked.len(),
                self.count()
            ));
        }
        for i in 0..self.count() {
            let location = match asked.get(i) {
                Some(raw) => {
                    let found = hcloud_csi::location(raw).filter(|l| self.workers.contains_key(l));
                    if found.is_none() {
                        self.problems.push(format!(
                            "dns_locations has '{raw}', which has no Hetzner Cloud worker; pick \
                             from: {known}"
                        ));
                    }
                    found
                }
                None => ranked.get(i % ranked.len().max(1)).map(|l| l.to_string()),
            };
            self.classes.push(
                location
                    .as_deref()
                    .map_or_else(|| GENERIC_CLASS.to_string(), |l| layout.class(l)),
            );
            self.locations.push(location);
        }
        // The instances never share a node, so each location needs a worker
        // for every instance kept there.
        let mut per_location: BTreeMap<&str, usize> = BTreeMap::new();
        for loc in self.locations.iter().flatten() {
            *per_location.entry(loc).or_default() += 1;
        }
        for (loc, want) in per_location {
            let have = self.workers.get(loc).map_or(0, Vec::len);
            if have < want {
                self.problems.push(format!(
                    "{want} instances keep their volumes in {loc}, which has {have} Hetzner \
                     Cloud worker(s), and two instances never share a node: add a worker \
                     there, or spread them with dns_locations"
                ));
            }
        }
        if self.workers.is_empty() {
            self.problems
                .push("PowerDNS runs on Hetzner Cloud workers only, and there is none".into());
        }
    }

    /// The configuration every instance shares.
    pub fn config(&self) -> Value {
        // After the image's own _api.conf, which opens the API to everyone.
        let conf = format!(
            "local-address=0.0.0.0:{DNS_PORT}\n\
             webserver-allow-from=127.0.0.1,::1\n\
             default-ttl=3600\n"
        );
        json!({
            "apiVersion": "v1",
            "kind": "ConfigMap",
            "metadata": {"name": CONFIG, "namespace": NAMESPACE},
            "data": {"uc.conf": conf},
        })
    }

    /// Instance `n` (from 1): its claim of `size` in `class` and its
    /// deployment.
    pub fn manifest(&self, n: usize, size: &str, class: &str) -> Value {
        let name = instance(n);
        let meta = |name: &str| json!({"name": name, "namespace": NAMESPACE});
        let labels = json!({"app": name, NAME_LABEL: APP});
        // A changed configuration restarts the instances; a mounted file
        // alone would not change under a running PowerDNS.
        let config = Sha256::digest(self.config().to_string().as_bytes());
        let config: String = config[..8].iter().map(|b| format!("{b:02x}")).collect();
        let pvc = json!({
            "apiVersion": "v1",
            "kind": "PersistentVolumeClaim",
            "metadata": meta(&claim(n)),
            "spec": {
                "accessModes": ["ReadWriteOnce"],
                "storageClassName": class,
                "resources": {"requests": {"storage": size}},
            },
        });
        let data = json!({"name": "data", "mountPath": DATA});
        // The image makes its database at build time, which the volume
        // hides; a new volume gets the schema here.
        let init = format!("test -s {DB} || sqlite3 {DB} < {SCHEMA}");
        let affinity = json!({
            // Workers only, and Hetzner Cloud ones, which volumes reach.
            "nodeAffinity": {"requiredDuringSchedulingIgnoredDuringExecution": {
                "nodeSelectorTerms": [{"matchExpressions": [
                    {"key": CONTROL_PLANE, "operator": "DoesNotExist"},
                    {"key": PROVIDER_KEY, "operator": "In", "values": [PROVIDER]},
                ]}]
            }},
            // Never two instances on one node.
            "podAntiAffinity": {"requiredDuringSchedulingIgnoredDuringExecution": [{
                "labelSelector": {"matchLabels": {NAME_LABEL: APP}},
                "topologyKey": HOSTNAME,
            }]},
        });
        let deployment = json!({
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "metadata": {"name": name, "namespace": NAMESPACE, "labels": labels},
            "spec": {
                "replicas": 1,
                // A Hetzner Volume is attached to one node at a time, and
                // SQLite wants one writer.
                "strategy": {"type": "Recreate"},
                "selector": {"matchLabels": {"app": name}},
                "template": {
                    "metadata": {"labels": labels, "annotations": {CONFIG_ANNOTATION: config}},
                    "spec": {
                        "affinity": affinity,
                        "securityContext": {"fsGroup": UID},
                        "initContainers": [{
                            "name": "schema",
                            "image": self.image,
                            "command": ["sh", "-c", init],
                            "volumeMounts": [data],
                        }],
                        "containers": [{
                            "name": APP,
                            "image": self.image,
                            "env": [{
                                "name": "PDNS_AUTH_API_KEY",
                                "valueFrom": {"secretKeyRef": {"name": API_SECRET, "key": "api-key"}},
                            }],
                            "ports": [
                                {"name": "dns", "containerPort": DNS_PORT, "protocol": "UDP"},
                                {"name": "dns-tcp", "containerPort": DNS_PORT, "protocol": "TCP"},
                                {"name": "api", "containerPort": API_PORT, "protocol": "TCP"},
                            ],
                            "volumeMounts": [
                                data,
                                {"name": "config", "mountPath": "/etc/powerdns/pdns.d/uc.conf",
                                 "subPath": "uc.conf", "readOnly": true},
                            ],
                            "readinessProbe": {"tcpSocket": {"port": DNS_PORT}},
                            "resources": {
                                "requests": {"cpu": "50m", "memory": "64Mi"},
                                "limits": {"memory": "512Mi"},
                            },
                        }],
                        "volumes": [
                            {"name": "data", "persistentVolumeClaim": {"claimName": claim(n)}},
                            {"name": "config", "configMap": {"name": CONFIG}},
                        ],
                    },
                },
            },
        });
        json!({
            "apiVersion": "v1",
            "kind": "List",
            "items": [pvc, deployment],
        })
    }
}

/// The service every instance answers behind, on every node.
pub fn service() -> Value {
    json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {"name": SERVICE, "namespace": NAMESPACE, "labels": {NAME_LABEL: APP}},
        "spec": {
            "type": "LoadBalancer",
            // Any node passes queries on to any ready instance, wherever
            // it runs.
            "externalTrafficPolicy": "Cluster",
            "selector": {NAME_LABEL: APP},
            "ports": [
                {"name": "dns", "port": 53, "targetPort": DNS_PORT, "protocol": "UDP"},
                {"name": "dns-tcp", "port": 53, "targetPort": DNS_PORT, "protocol": "TCP"},
            ],
        },
    })
}

pub fn tasks(plan: &mut Plan, cluster: &Arc<Cluster>, vars: &Vars, layout: &Layout) -> Result<()> {
    let settings = Arc::new(Settings::load(vars, layout)?);
    let start = plan.tasks().len();

    let s = settings.clone();
    plan.add(
        Task::new("dns/report", "Plan the name servers: instances and volumes")
            .tags(&["dns"])
            .run(move |ctx| async move { report(&ctx, &s) }),
    );

    let c = cluster.clone();
    let namespace = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {"name": NAMESPACE},
    })
    .to_string();
    plan.add(
        Task::new("dns/namespace", "Create the dns namespace")
            .tags(&["dns"])
            .after(["dns/report"])
            .run(move |ctx| async move { apply(&ctx, &c, &namespace).await }),
    );

    let c = cluster.clone();
    plan.add(
        Task::new("dns/api-key", "Keep the PowerDNS API key in dns/pdns-api")
            .tags(&["dns"])
            .after(["dns/namespace"])
            .run(move |ctx| async move { api_key(&ctx, &c).await }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "dns/install",
            "Run each PowerDNS instance on its own Hetzner Volume",
        )
        .tags(&["dns"])
        .after([
            "dns/api-key",
            // The volumes need the driver and the storage classes.
            "hcloud_csi/storage-classes",
        ])
        .run(move |ctx| async move { install(&ctx, &c, &s).await }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "dns/verify",
            "Check every instance runs and the nodes answer on port 53",
        )
        .tags(&["dns"])
        .after(["dns/install"])
        .run(move |ctx| async move { verify(&ctx, &c, &s).await }),
    );

    plan.gate(
        start,
        settings.enabled,
        "dns_enabled is false; drop it from your config to run the name servers",
    );
    Ok(())
}

fn report(ctx: &Ctx, s: &Settings) -> Result<Outcome> {
    let mut problems = s.problems.clone();
    if s.count() < 2 {
        problems.push(format!(
            "dns_instances is {}: name servers come in twos at least",
            s.count()
        ));
    }
    for n in 1..=s.count() {
        ctx.note(&format!(
            "{}: {} volume in {} ({})",
            instance(n),
            s.size,
            s.locations[n - 1].as_deref().unwrap_or("?"),
            s.classes[n - 1],
        ));
    }
    if s.locations
        .iter()
        .flatten()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        == 1
        && s.count() > 1
    {
        ctx.note(&format!(
            "warning: every volume is in {}: losing that location loses every instance; add \
             workers in another location to spread them",
            s.locations[0].as_deref().unwrap_or("?")
        ));
    }
    ctx.note(&format!(
        "PowerDNS {}, behind {NAMESPACE}/{SERVICE} on port 53 of every node",
        s.image
    ));
    if problems.is_empty() {
        return Ok(Outcome::Ok);
    }
    bail!("fix these first:\n- {}", problems.join("\n- "))
}

async fn api_key(ctx: &Ctx, cluster: &Cluster) -> Result<Outcome> {
    let found = kubectl(ctx, cluster)
        .args(["-n", NAMESPACE, "get", "secret", API_SECRET, "-o", "name"])
        .any_code()
        .read_only()
        .output()
        .await?;
    if found.success() {
        return Ok(Outcome::Ok);
    }
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    let key = password::generate(
        40,
        Charset {
            letters: true,
            digits: true,
            symbols: false,
        },
    )?;
    // `create`, not `apply`: apply would keep a copy of the key in the
    // last-applied annotation.
    let secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {"name": API_SECRET, "namespace": NAMESPACE},
        "stringData": {"api-key": key},
    });
    kubectl(ctx, cluster)
        .args(["create", "-f", "-"])
        .stdin(secret.to_string())
        .output()
        .await?;
    Ok(Outcome::Changed)
}

/// Instance `n`'s claim as it is: its size and class, if it exists.
async fn current_claim(ctx: &Ctx, cluster: &Cluster, n: usize) -> Result<Option<(String, String)>> {
    let out = kubectl(ctx, cluster)
        .args(["-n", NAMESPACE, "get", "pvc", &claim(n)])
        .args([
            "-o",
            "jsonpath={.spec.resources.requests.storage} {.spec.storageClassName}",
        ])
        .any_code()
        .read_only()
        .output()
        .await?;
    if !out.success() {
        return Ok(None);
    }
    let mut parts = out.stdout.split_whitespace();
    Ok(Some((
        parts.next().unwrap_or_default().to_string(),
        parts.next().unwrap_or_default().to_string(),
    )))
}

async fn install(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    // kubectl diff can't look into a namespace that isn't there yet.
    if ctx.check() {
        let ns = kubectl(ctx, cluster)
            .args(["get", "namespace", NAMESPACE, "-o", "name"])
            .any_code()
            .read_only()
            .output()
            .await?;
        if !ns.success() {
            return Ok(Outcome::Changed);
        }
    }
    let mut outcome = apply(ctx, cluster, &s.config().to_string()).await?;
    outcome = outcome.and(apply(ctx, cluster, &service().to_string()).await?);
    let mut changed = Vec::new();
    for n in 1..=s.count() {
        // A claim can't shrink, and its class (its location) can't change.
        let (size, class) = match current_claim(ctx, cluster, n).await? {
            Some((size, class)) => {
                if bytes(&size) > bytes(&s.size) {
                    ctx.note(&format!(
                        "{} keeps its {size}: a volume grows, never shrinks",
                        claim(n)
                    ));
                }
                let size = if bytes(&size) > bytes(&s.size) {
                    size
                } else {
                    s.size.clone()
                };
                (size, class)
            }
            None => (s.size.clone(), s.classes[n - 1].clone()),
        };
        let manifest = s.manifest(n, &size, &class).to_string();
        let done = apply(ctx, cluster, &manifest).await?;
        if done != Outcome::Ok {
            changed.push(n);
        }
        outcome = outcome.and(done);
    }
    if ctx.check() {
        return Ok(outcome);
    }
    // A new volume is attached, or a new pod starts, before it answers.
    for n in changed {
        let rollout = kubectl(ctx, cluster)
            .args(["-n", NAMESPACE, "rollout", "status"])
            .arg(format!("deployment/{}", instance(n)))
            .arg("--timeout=600s")
            .output()
            .await;
        if let Err(err) = rollout {
            bail!("{err:#}\n\n{}", logs(ctx, cluster, n).await);
        }
    }
    Ok(outcome)
}

/// The tail of an instance's logs and its claim's events, for a failure
/// message.
async fn logs(ctx: &Ctx, cluster: &Cluster, n: usize) -> String {
    let deployment = format!("deployment/{}", instance(n));
    let claim = claim(n);
    let mut text = String::new();
    for args in [
        vec!["logs", deployment.as_str(), "--all-containers", "--tail=30"],
        vec!["describe", "pvc", claim.as_str()],
    ] {
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

/// The addresses a service answers on.
pub fn addresses(service: &Value) -> Vec<IpAddr> {
    service["status"]["loadBalancer"]["ingress"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| i["ip"].as_str()?.parse().ok())
        .collect()
}

async fn verify(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    if ctx.check() && ctx.deps_changed() {
        return Ok(Outcome::Skipped("checked once it is installed".into()));
    }
    let mut problems = Vec::new();
    for n in 1..=s.count() {
        let name = instance(n);
        let rollout = kubectl(ctx, cluster)
            .args(["-n", NAMESPACE, "rollout", "status"])
            .arg(format!("deployment/{name}"))
            .arg("--timeout=300s")
            .any_code()
            .read_only()
            .output()
            .await?;
        if !rollout.success() {
            problems.push(format!(
                "deployment/{name} is not ready: {}\n{}",
                rollout.combined().trim(),
                logs(ctx, cluster, n).await
            ));
            continue;
        }
        let claim = get_json(ctx, cluster, &["-n", NAMESPACE, "get", "pvc", &claim(n)]).await?;
        if claim["status"]["phase"] != "Bound" {
            problems.push(format!(
                "the claim of {name} is {}, not Bound",
                claim["status"]["phase"].as_str().unwrap_or("unknown")
            ));
        }
        let pods = get_json(
            ctx,
            cluster,
            &["-n", NAMESPACE, "get", "pods", "-l", &format!("app={name}")],
        )
        .await?;
        if let Some(node) = pods["items"][0]["spec"]["nodeName"].as_str() {
            ctx.log(&format!("{name} runs on {node}"));
        }
    }
    let service = get_json(ctx, cluster, &["-n", NAMESPACE, "get", "service", SERVICE]).await?;
    let ips = addresses(&service);
    if ips.is_empty() {
        problems.push(format!(
            "port 53 answers nowhere yet (kubectl -n kube-system get pods -l \
             svccontroller.k3s.cattle.io/svcname={SERVICE} -o wide)"
        ));
    }
    for ip in &ips {
        match probe(*ip).await {
            Ok(()) => ctx.log(&format!("{ip} answers on port 53")),
            Err(err) => problems.push(format!(
                "{ip} doesn't answer on port 53: {err:#}; the host firewall doesn't stand in \
                 the way (ServiceLB's traffic is forwarded, never INPUT), so look at the \
                 svclb-pdns pods (kubectl -n kube-system get pods -o wide | grep svclb-pdns) \
                 or a Hetzner Cloud Firewall on the project"
            )),
        }
    }
    if !problems.is_empty() {
        bail!("{}", problems.join("\n"));
    }
    ctx.note(
        "add a zone with `uc dns zone add example.com`; it names the workers as its name \
         servers and prints what to give your registrar",
    );
    Ok(Outcome::Ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kube::roles::hcloud_csi::{HNode, Source};

    fn hnode(name: &str, loc: &str) -> HNode {
        HNode {
            name: name.into(),
            control_plane: false,
            location: Some(loc.into()),
            source: Source::Label,
            robot: false,
            needs_label: false,
            taints: Vec::new(),
        }
    }

    fn layout(workers: &[(&str, &str)]) -> Layout {
        Layout {
            nodes: workers.iter().map(|(n, l)| hnode(n, l)).collect(),
            primary: Some("fsn1".into()),
            ..Layout::default()
        }
    }

    fn settings(l: &Layout, locations: &[&str]) -> Settings {
        let mut s = Settings {
            enabled: true,
            instances: 2,
            size: "10Gi".into(),
            image: IMAGE.into(),
            ..Settings::default()
        };
        let asked: Vec<String> = locations.iter().map(|l| l.to_string()).collect();
        s.place(l, &asked);
        s
    }

    fn item<'a>(list: &'a Value, kind: &str) -> &'a Value {
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["kind"] == kind)
            .unwrap()
    }

    #[test]
    fn spreads_the_volumes_over_locations() {
        let s = settings(&layout(&[("w1", "hel1"), ("w2", "fsn1")]), &[]);
        assert!(s.problems.is_empty(), "{:?}", s.problems);
        // The primary location first on a tie.
        assert_eq!(s.classes, ["hcloud-volumes", "hcloud-volumes-hel1"]);
        let s = settings(
            &layout(&[("w1", "hel1"), ("w2", "hel1"), ("w3", "fsn1")]),
            &[],
        );
        assert_eq!(s.classes, ["hcloud-volumes-hel1", "hcloud-volumes"]);
    }

    #[test]
    fn one_location_needs_a_worker_per_instance() {
        let s = settings(&layout(&[("w1", "fsn1")]), &[]);
        assert!(s.problems[0].contains("2 instances keep their volumes in fsn1"));
        let s = settings(&layout(&[("w1", "fsn1"), ("w2", "fsn1")]), &[]);
        assert!(s.problems.is_empty(), "{:?}", s.problems);
    }

    #[test]
    fn the_locations_can_be_chosen() {
        let l = layout(&[("w1", "fsn1"), ("w2", "fsn1"), ("w3", "hel1")]);
        let s = settings(&l, &["hel1", "fsn1"]);
        assert_eq!(s.classes, ["hcloud-volumes-hel1", "hcloud-volumes"]);
        let s = settings(&l, &["nbg1", "fsn1"]);
        assert!(s.problems[0].contains("'nbg1'"));
    }

    #[test]
    fn each_instance_has_its_own_volume() {
        let s = settings(&layout(&[("w1", "fsn1"), ("w2", "hel1")]), &[]);
        let one = s.manifest(1, "10Gi", "hcloud-volumes");
        let two = s.manifest(2, "10Gi", "hcloud-volumes-hel1");
        assert_eq!(
            item(&one, "PersistentVolumeClaim")["metadata"]["name"],
            "pdns-1-data"
        );
        assert_eq!(
            item(&two, "PersistentVolumeClaim")["metadata"]["name"],
            "pdns-2-data"
        );
        let volumes = |m: &Value| {
            item(m, "Deployment")["spec"]["template"]["spec"]["volumes"][0]["persistentVolumeClaim"]
                ["claimName"]
                .clone()
        };
        assert_eq!(volumes(&one), "pdns-1-data");
        assert_eq!(volumes(&two), "pdns-2-data");
    }

    #[test]
    fn every_instance_answers_behind_one_service() {
        let service = service();
        assert_eq!(service["spec"]["type"], "LoadBalancer");
        assert_eq!(service["spec"]["externalTrafficPolicy"], "Cluster");
        assert_eq!(service["spec"]["selector"][NAME_LABEL], APP);
        let s = settings(&layout(&[("w1", "fsn1"), ("w2", "hel1")]), &[]);
        let m = s.manifest(2, "10Gi", "hcloud-volumes-hel1");
        assert_eq!(
            item(&m, "Deployment")["spec"]["template"]["metadata"]["labels"][NAME_LABEL],
            APP
        );
        let ports: Vec<_> = service["spec"]["ports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["port"].as_u64().unwrap(), p["protocol"].as_str().unwrap()))
            .collect();
        assert_eq!(ports, [(53, "UDP"), (53, "TCP")]);
    }

    #[test]
    fn instances_never_share_a_node() {
        let s = settings(&layout(&[("w1", "fsn1"), ("w2", "hel1")]), &[]);
        let m = s.manifest(1, "10Gi", "hcloud-volumes");
        let spec = &item(&m, "Deployment")["spec"]["template"]["spec"];
        let anti = &spec["affinity"]["podAntiAffinity"]["requiredDuringSchedulingIgnoredDuringExecution"]
            [0];
        assert_eq!(anti["labelSelector"]["matchLabels"][NAME_LABEL], APP);
        assert_eq!(anti["topologyKey"], HOSTNAME);
        assert_eq!(spec["securityContext"]["fsGroup"], UID);
        assert_eq!(
            item(&m, "Deployment")["spec"]["strategy"]["type"],
            "Recreate"
        );
    }
}
