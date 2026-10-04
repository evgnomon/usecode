// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `forgejo`: your own Git forge on the cluster, through the official chart
//! (`oci://code.forgejo.org/forgejo-helm/forgejo`).
//!
//! Everything Forgejo keeps (repositories, its SQLite database, LFS,
//! packages, avatars) lives on one Hetzner Cloud Volume from
//! [`hcloud_csi`](super::hcloud_csi), in the `hcloud-volumes` class, so the
//! pod can move to any Hetzner worker in the primary location and keep its
//! data. It runs on worker nodes only, never on a control plane. The volume
//! survives `helm uninstall`; only deleting the claim
//! deletes it.
//!
//! The web UI is on `https://<forgejo_host>` through k3s's Traefik, with a
//! Let's Encrypt certificate from the `letsencrypt` ClusterIssuer that
//! `lib/infra/bootstrap.sh` sets up. With `forgejo_dns_target` the role
//! keeps the name a CNAME to it on Cloudflare, with `cf`; otherwise point it
//! at the workers yourself (`lib/infra/dns.sh <forgejo_host>`).
//!
//! Variables (`-e` or the user config):
//!
//! * `forgejo_host`: the name Forgejo answers on, e.g. `git.example.com`.
//!   Without it the role does nothing.
//! * `forgejo_size`: the volume size (default: 10Gi, Hetzner's smallest). It
//!   can grow later, never shrink: a bigger volume keeps its size.
//! * `forgejo_storage_class`: (default: `hcloud-volumes`).
//! * `forgejo_admin`, `forgejo_admin_email`: the admin login (default:
//!   `usecode`, `admin@<forgejo_host>`).
//! * `forgejo_ssh_port`: serve git over SSH on this port of every node, e.g.
//!   2222 (default: off; clone over HTTPS). Not 22, which the nodes' own
//!   sshd has.
//! * `forgejo_dns_target`: a name that already points at the workers, e.g.
//!   `example.com` (as `lib/infra/dns.sh` leaves it); `forgejo_host` becomes
//!   a CNAME to it, so it follows the workers too. Needs
//!   `CLOUDFLARE_API_TOKEN` (`source ~/.bashrc.d/cloudflare.sh`).
//! * `forgejo_dns_zone`: the Cloudflare zone of `forgejo_host` (default: the
//!   host without its first label).
//! * `forgejo_version`: the chart version.
//!
//! The admin password is made once, at random, and kept in
//! forgejo/forgejo-admin: the secret is the source of truth, so changing it
//! is editing the secret and restarting the deployment.

use crate::configure::ctx::Ctx;
use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::copy;
use crate::configure::vars::Vars;
use crate::kube::cluster::{CONTROL_PLANE, Cluster, Node};
use crate::kube::roles::hcloud_csi::{
    DRIVER, GENERIC_CLASS, LOCATION_LABEL, PROVIDER, PROVIDER_KEY, TOPOLOGY_KEY,
};
use crate::kube::{apply, files_dir, get_json, helm, kubectl, var};
use crate::password::{self, Charset};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const NAMESPACE: &str = "forgejo";
const RELEASE: &str = "forgejo";
const CHART: &str = "oci://code.forgejo.org/forgejo-helm/forgejo";
/// The chart version this role was written against (Forgejo 15).
const CHART_VERSION: &str = "17.1.7";
const CLAIM: &str = "forgejo-data";
const ADMIN_SECRET: &str = "forgejo-admin";
const TLS_SECRET: &str = "forgejo-tls";
const ISSUER: &str = "letsencrypt";
/// The port SSH listens on inside the rootless image.
const SSH_LISTEN_PORT: u16 = 2222;

/// What the user decided, from the variables.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub host: Option<String>,
    pub size: String,
    pub class: String,
    pub admin: String,
    pub admin_email: String,
    pub ssh_port: Option<u16>,
    pub version: String,
    /// The CNAME target for `host`, when the role manages its record.
    pub dns_target: Option<String>,
    pub dns_zone: String,
}

impl Settings {
    pub fn load(vars: &Vars) -> Result<Settings> {
        let host: Option<String> = var(vars, "forgejo_host")?;
        let host = host
            .map(|h| h.trim().trim_end_matches('.').to_lowercase())
            .filter(|h| !h.is_empty());
        let name = |v: Option<String>| {
            v.map(|n| n.trim().trim_end_matches('.').to_lowercase())
                .filter(|n| !n.is_empty())
        };
        let dns_target = name(var(vars, "forgejo_dns_target")?);
        if dns_target.is_some() && dns_target == host {
            bail!("forgejo_dns_target can't be forgejo_host itself");
        }
        let dns_zone = name(var(vars, "forgejo_dns_zone")?).unwrap_or_else(|| {
            let host = host.as_deref().unwrap_or_default();
            host.split_once('.')
                .map_or(host, |(_, zone)| zone)
                .to_string()
        });
        let ssh_port: Option<u16> = var(vars, "forgejo_ssh_port")?;
        if ssh_port == Some(22) {
            bail!("forgejo_ssh_port can't be 22: the nodes' own sshd listens there");
        }
        Ok(Settings {
            admin_email: var(vars, "forgejo_admin_email")?
                .unwrap_or_else(|| format!("admin@{}", host.as_deref().unwrap_or("localhost"))),
            host,
            size: var(vars, "forgejo_size")?.unwrap_or_else(|| "10Gi".into()),
            class: var(vars, "forgejo_storage_class")?.unwrap_or_else(|| GENERIC_CLASS.into()),
            admin: var(vars, "forgejo_admin")?.unwrap_or_else(|| "usecode".into()),
            ssh_port: ssh_port.filter(|p| *p != 0),
            version: var(vars, "forgejo_version")?.unwrap_or_else(|| CHART_VERSION.into()),
            dns_target,
            dns_zone,
        })
    }

    /// The chart values.
    pub fn values(&self) -> Value {
        let host = self.host.as_deref().unwrap_or_default();
        let mut server = json!({
            "DOMAIN": host,
            "ROOT_URL": format!("https://{host}/"),
            "SSH_DOMAIN": host,
            "SSH_LISTEN_PORT": SSH_LISTEN_PORT,
        });
        match self.ssh_port {
            Some(port) => server["SSH_PORT"] = json!(port),
            None => server["DISABLE_SSH"] = json!(true),
        }
        let mut values = json!({
            "persistence": {
                "enabled": true,
                "create": true,
                "claimName": CLAIM,
                "size": self.size,
                "storageClass": self.class,
                "accessModes": ["ReadWriteOnce"],
                "annotations": {"helm.sh/resource-policy": "keep"},
            },
            // A Hetzner Volume is attached to one node at a time.
            "strategy": {"type": "Recreate"},
            // Workers only, and Hetzner Cloud ones, which volumes reach.
            "affinity": {"nodeAffinity": {"requiredDuringSchedulingIgnoredDuringExecution": {
                "nodeSelectorTerms": [{"matchExpressions": [
                    {"key": CONTROL_PLANE, "operator": "DoesNotExist"},
                    {"key": PROVIDER_KEY, "operator": "In", "values": [PROVIDER]},
                ]}]
            }}},
            "gitea": {
                "admin": {
                    "existingSecret": ADMIN_SECRET,
                    "email": self.admin_email,
                    "passwordMode": "keepUpdated",
                },
                "config": {
                    "APP_NAME": "Forgejo",
                    "server": server,
                    "service": {"DISABLE_REGISTRATION": true},
                },
            },
            "ingress": {
                "enabled": true,
                "className": "traefik",
                "annotations": {
                    "cert-manager.io/cluster-issuer": ISSUER,
                    "traefik.ingress.kubernetes.io/router.middlewares":
                        format!("{NAMESPACE}-redirect-https@kubernetescrd"),
                },
                "hosts": [{"host": host, "paths": [
                    {"path": "/", "pathType": "Prefix", "port": "http"}
                ]}],
                "tls": [{"secretName": TLS_SECRET, "hosts": [host]}],
            },
            "extraDeploy": [{
                "apiVersion": "traefik.io/v1alpha1",
                "kind": "Middleware",
                "metadata": {"name": "redirect-https", "namespace": NAMESPACE},
                "spec": {"redirectScheme": {"scheme": "https", "permanent": true}},
            }],
            "resources": {
                "requests": {"cpu": "100m", "memory": "256Mi"},
                "limits": {"memory": "1Gi"},
            },
        });
        if let Some(port) = self.ssh_port {
            values["service"] = json!({"ssh": {"type": "LoadBalancer", "port": port}});
        }
        values
    }
}

pub fn tasks(plan: &mut Plan, cluster: &Arc<Cluster>, vars: &Vars) -> Result<()> {
    let settings = Arc::new(Settings::load(vars)?);
    let start = plan.tasks().len();
    let values_file = files_dir(vars, cluster).join("forgejo-values.yaml");
    let values_yaml = format!(
        "# Written by `uc kube configure -t forgejo` for context {}.\n\
         # It is rewritten on every run: change the variables, not this file.\n{}",
        cluster.context,
        serde_yaml::to_string(&settings.values())?
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new("forgejo/report", "Plan Forgejo: its name, volume and login")
            .tags(&["git"])
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
        Task::new("forgejo/namespace", "Create the forgejo namespace")
            .tags(&["git"])
            .after(["forgejo/report"])
            .run(move |ctx| async move { apply(&ctx, &c, &namespace).await }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "forgejo/admin",
            "Keep the admin login in forgejo/forgejo-admin",
        )
        .tags(&["git"])
        .after(["forgejo/namespace"])
        .run(move |ctx| async move { admin(&ctx, &c, &s).await }),
    );

    let s = settings.clone();
    plan.add(
        Task::new("forgejo/dns", "Point the name at the workers on Cloudflare")
            .tags(&["git", "dns"])
            .after(["forgejo/report"])
            .when(
                settings.dns_target.is_some(),
                "forgejo_dns_target is not set; the name is yours to point",
            )
            .run(move |ctx| async move {
                let host = s.host.as_deref().unwrap_or_default();
                let target = s.dns_target.as_deref().unwrap_or_default();
                dns(&ctx, host, target, &s.dns_zone).await
            }),
    );

    let vf = values_file.clone();
    plan.add(
        Task::new("forgejo/files", "Write the chart values")
            .tags(&["git"])
            .after(["forgejo/report"])
            .run(move |ctx| async move {
                let outcome = copy::content(&ctx, &values_yaml, &vf, Some(0o644), false).await?;
                ctx.log(&vf.display().to_string());
                Ok(outcome)
            }),
    );

    let (c, s, vf) = (cluster.clone(), settings.clone(), values_file);
    plan.add(
        Task::new("forgejo/install", "Install Forgejo with helm")
            .tags(&["git"])
            .after([
                "forgejo/admin",
                "forgejo/files",
                // cert-manager asks Let's Encrypt as soon as the Ingress is
                // there, so the name has to resolve by then.
                "forgejo/dns",
                // Its volume needs the driver and the storage class.
                "hcloud_csi/storage-classes",
            ])
            .run(move |ctx| async move { install(&ctx, &c, &s, &vf).await }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "forgejo/verify",
            "Check Forgejo runs on its Hetzner Volume and has its certificate",
        )
        .tags(&["git"])
        .after(["forgejo/install"])
        .run(move |ctx| async move { verify(&ctx, &c, &s).await }),
    );

    plan.gate(
        start,
        settings.host.is_some(),
        "forgejo_host is not set (e.g. -e forgejo_host=git.example.com)",
    );
    Ok(())
}

async fn report(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    let host = s.host.clone().unwrap_or_default();
    ctx.note(&format!(
        "https://{host}, {} on {} (claim {NAMESPACE}/{CLAIM}), admin {} <{}>",
        s.size, s.class, s.admin, s.admin_email
    ));
    ctx.note(&match s.ssh_port {
        Some(port) => format!("git over SSH: ssh://git@{host}:{port}/<owner>/<repo>.git"),
        None => "git over SSH is off; set forgejo_ssh_port to turn it on".into(),
    });

    let mut problems = Vec::new();
    let class_known = cluster.storage_classes.iter().any(|c| c.name == s.class);
    // hcloud_csi makes `hcloud-volumes` in the same run.
    if !class_known && s.class != GENERIC_CLASS {
        problems.push(format!("the storage class {} does not exist", s.class));
    }
    if let Some(class) = cluster.storage_classes.iter().find(|c| c.name == s.class)
        && class.provisioner != DRIVER
    {
        ctx.note(&format!(
            "{} is provisioned by {}, not the Hetzner CSI driver",
            s.class, class.provisioner
        ));
    }

    let (usable, place) = volume_workers(ctx, cluster, &s.class, class_known).await?;
    match usable.len() {
        0 => problems.push(format!(
            "Forgejo runs on Hetzner Cloud workers only, and there is none{place}: \
             add a worker there, or pick a class in a location that has one \
             (forgejo_storage_class)"
        )),
        1 => ctx.note(&format!(
            "warning: {} is the only worker{place}: if it goes down, Forgejo waits for it, \
             since its volume never leaves that location",
            usable[0]
        )),
        _ => ctx.note(&format!(
            "runs on the workers{place}: {}",
            usable.join(", ")
        )),
    }

    let issuer = kubectl(ctx, cluster)
        .args(["get", "clusterissuer", ISSUER, "-o", "name"])
        .any_code()
        .read_only()
        .output()
        .await?;
    if !issuer.success() {
        ctx.note(&format!(
            "there is no ClusterIssuer {ISSUER} yet, so no certificate until there is \
             (lib/infra/bootstrap.sh adds it)"
        ));
    }

    // forgejo/dns sees to the name when it manages the record.
    if s.dns_target.is_none() {
        check_resolves(ctx, cluster, &host).await?;
    }

    if problems.is_empty() {
        return Ok(Outcome::Ok);
    }
    bail!("fix these first:\n- {}", problems.join("\n- "))
}

/// The Hetzner Cloud workers a pod with a volume in `class` can run on, and
/// " in <location>" when the class keeps its volumes in one.
pub(super) async fn volume_workers(
    ctx: &Ctx,
    cluster: &Cluster,
    class: &str,
    class_known: bool,
) -> Result<(Vec<String>, String)> {
    // The location the class keeps its volumes in; on the first run
    // hcloud-volumes doesn't exist yet, and the scheduler picks a worker in
    // the primary location.
    let class_location = if class_known {
        let class = get_json(ctx, cluster, &["get", "storageclass", class]).await?;
        class["allowedTopologies"][0]["matchLabelExpressions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["key"] == TOPOLOGY_KEY)
            .and_then(|e| e["values"][0].as_str())
            .map(str::to_string)
    } else {
        None
    };
    let usable = cluster
        .nodes
        .iter()
        .filter(|n| !n.control_plane && n.label(PROVIDER_KEY) == Some(PROVIDER))
        .filter(|n| match &class_location {
            Some(loc) => worker_location(n) == Some(loc.as_str()),
            None => true,
        })
        .map(|n| n.name.clone())
        .collect();
    let place = class_location
        .as_deref()
        .map(|l| format!(" in {l}"))
        .unwrap_or_default();
    Ok((usable, place))
}

/// Notes when `host` does not resolve to a node.
pub(super) async fn check_resolves(ctx: &Ctx, cluster: &Cluster, host: &str) -> Result<()> {
    let lookup = host.to_string();
    let resolved: Vec<IpAddr> = tokio::task::spawn_blocking(move || {
        (lookup.as_str(), 443)
            .to_socket_addrs()
            .map(|addrs| addrs.map(|a| a.ip()).collect())
            .unwrap_or_default()
    })
    .await?;
    let node_ips: Vec<&str> = cluster
        .nodes
        .iter()
        .flat_map(|n| n.addresses.iter().map(String::as_str))
        .collect();
    if resolved.is_empty() {
        ctx.note(&format!(
            "{host} does not resolve; point it at the workers: lib/infra/dns.sh {host}"
        ));
    } else if !resolved
        .iter()
        .any(|ip| node_ips.contains(&ip.to_string().as_str()))
    {
        ctx.note(&format!(
            "{host} resolves to {}, none of them a node; lib/infra/dns.sh {host} fixes it \
             (unless a proxy is in front)",
            resolved
                .iter()
                .map(IpAddr::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(())
}

/// Keeps `host` a CNAME to `target` on Cloudflare, the only record for
/// that name in `zone`, then waits until public DNS follows it.
pub(super) async fn dns(ctx: &Ctx, host: &str, target: &str, zone: &str) -> Result<Outcome> {
    let cf = |args: &[&str]| {
        ctx.cmd("cf")
            .args(["dns", "records"])
            .args(args.iter().copied())
    };
    let hint = "is cf installed and CLOUDFLARE_API_TOKEN set? (source ~/.bashrc.d/cloudflare.sh)";

    let listed = cf(&["list", "-z", zone, "--name", host])
        .read_only()
        .output()
        .await
        .map_err(|e| anyhow!("{e:#}\n{hint}"))?;
    let records: Vec<Value> = serde_json::from_str(&listed.stdout)
        .map_err(|e| anyhow!("reading `cf dns records list`: {e}"))?;
    let wanted = |r: &Value| {
        r["type"] == "CNAME"
            && r["proxied"] == false
            && r["content"]
                .as_str()
                .is_some_and(|c| c.trim_end_matches('.').eq_ignore_ascii_case(target))
    };
    let keep = records.iter().find(|r| wanted(r)).cloned();
    // A CNAME can't share its name, and the old ones point elsewhere.
    let stale: Vec<&Value> = records
        .iter()
        .filter(|r| matches!(r["type"].as_str(), Some("A" | "AAAA" | "CNAME")))
        .filter(|r| keep.as_ref().is_none_or(|k| k["id"] != r["id"]))
        .collect();

    let mut outcome = Outcome::Ok;
    for r in &stale {
        let id = r["id"].as_str().unwrap_or_default();
        let what = format!(
            "{} {host} -> {}",
            r["type"].as_str().unwrap_or("?"),
            r["content"].as_str().unwrap_or("?")
        );
        if !ctx.check() {
            cf(&["delete", id, "-z", zone, "-q", "--force"])
                .output()
                .await?;
        }
        ctx.log(&format!("removed {what}"));
        outcome = Outcome::Changed;
    }
    if keep.is_none() {
        // Not proxied: Traefik answers Let's Encrypt itself, as for the
        // other names.
        let body = json!({
            "type": "CNAME",
            "name": host,
            "content": target,
            "proxied": false,
            "ttl": 60,
        })
        .to_string();
        if !ctx.check() {
            cf(&["create", "-z", zone, "-q", "--body", &body])
                .output()
                .await?;
        }
        ctx.log(&format!("CNAME {host} -> {target} (new)"));
        outcome = Outcome::Changed;
    }
    if ctx.check() {
        return Ok(outcome);
    }

    let resolve = |name: &str| {
        ctx.cmd("dig")
            .args(["+short", name, "A", "@1.1.1.1"])
            .any_code()
            .read_only()
            .output()
    };
    let ips = |out: &str| -> BTreeSet<IpAddr> {
        out.lines().filter_map(|l| l.trim().parse().ok()).collect()
    };
    let want = ips(&resolve(target).await?.stdout);
    if want.is_empty() {
        bail!("{target} does not resolve, so neither will {host}; lib/infra/dns.sh {target}");
    }
    for _ in 0..60 {
        if ips(&resolve(host).await?.stdout) == want {
            ctx.log(&format!("{host} resolves to the workers"));
            return Ok(outcome);
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    bail!("public DNS still does not resolve {host} like {target} after 5 minutes")
}

/// A node's Hetzner location, as hcloud_csi labels it.
fn worker_location(node: &Node) -> Option<&str> {
    node.label(LOCATION_LABEL).or(node.label(TOPOLOGY_KEY))
}

async fn admin(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    let found = kubectl(ctx, cluster)
        .args(["-n", NAMESPACE, "get", "secret", ADMIN_SECRET, "-o", "name"])
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
    let password = password::generate(
        32,
        Charset {
            letters: true,
            digits: true,
            symbols: false,
        },
    )?;
    // `create`, not `apply`: apply would keep a copy of the password in the
    // last-applied annotation.
    let secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {"name": ADMIN_SECRET, "namespace": NAMESPACE},
        "stringData": {"username": s.admin, "password": password},
    });
    kubectl(ctx, cluster)
        .args(["create", "-f", "-"])
        .stdin(secret.to_string())
        .output()
        .await?;
    Ok(Outcome::Changed)
}

async fn install(
    ctx: &Ctx,
    cluster: &Cluster,
    s: &Settings,
    values_file: &Path,
) -> Result<Outcome> {
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
        r["status"] == "deployed" && r["chart"] == format!("forgejo-{}", s.version)
    });
    // A claim can't shrink: one bigger than forgejo_size keeps its size.
    let claim = kubectl(ctx, cluster)
        .args(["-n", NAMESPACE, "get", "pvc", CLAIM])
        .args(["-o", "jsonpath={.spec.resources.requests.storage}"])
        .any_code()
        .read_only()
        .output()
        .await?;
    let current = claim.stdout.trim();
    let kept = (claim.success() && bytes(current) > bytes(&s.size)).then(|| current.to_string());
    let mut values = s.values();
    if let Some(size) = &kept {
        ctx.note(&format!(
            "{CLAIM} keeps its {size}: a volume grows, never shrinks, so forgejo_size={} \
             only applies to a new one",
            s.size
        ));
        values["persistence"]["size"] = json!(size);
    }
    if deployed {
        let got = helm(ctx, cluster)
            .args(["get", "values", RELEASE, "-n", NAMESPACE, "-o", "json"])
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
        .args([
            "upgrade",
            "--install",
            RELEASE,
            CHART,
            "-n",
            NAMESPACE,
            "--version",
            &s.version,
        ])
        .arg("-f")
        .arg(values_file.display().to_string())
        .args(
            kept.iter()
                .flat_map(|size| ["--set-string".into(), format!("persistence.size={size}")]),
        )
        // The first start attaches a new volume and migrates the database.
        .args(["--wait", "--timeout", "10m"])
        .output()
        .await;
    if let Err(err) = installed {
        bail!("{err:#}\n\n{}", logs(ctx, cluster).await);
    }
    Ok(Outcome::Changed)
}

/// The bytes in a Kubernetes quantity like `10Gi`, `20G` or `512Mi`.
pub(super) fn bytes(quantity: &str) -> Option<u128> {
    let q = quantity.trim();
    let split = q.find(|c: char| !c.is_ascii_digit()).unwrap_or(q.len());
    let (n, unit) = q.split_at(split);
    let mult: u128 = match unit {
        "" => 1,
        "Ki" => 1 << 10,
        "Mi" => 1 << 20,
        "Gi" => 1 << 30,
        "Ti" => 1 << 40,
        "k" => 1_000,
        "M" => 1_000_000,
        "G" => 1_000_000_000,
        "T" => 1_000_000_000_000,
        _ => return None,
    };
    Some(n.parse::<u128>().ok()? * mult)
}

/// The tail of Forgejo's logs and the claim's events, for a failure message.
async fn logs(ctx: &Ctx, cluster: &Cluster) -> String {
    let mut text = String::new();
    for args in [
        vec![
            "logs",
            "deployment/forgejo",
            "--all-containers",
            "--tail=30",
        ],
        vec!["describe", "pvc", CLAIM],
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

async fn verify(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    if ctx.check() && ctx.deps_changed() {
        return Ok(Outcome::Skipped("checked once it is installed".into()));
    }
    let mut problems = Vec::new();
    let rollout = kubectl(ctx, cluster)
        .args([
            "-n",
            NAMESPACE,
            "rollout",
            "status",
            "deployment/forgejo",
            "--timeout=300s",
        ])
        .any_code()
        .read_only()
        .output()
        .await?;
    if !rollout.success() {
        problems.push(format!(
            "deployment/forgejo is not ready: {}",
            rollout.combined().trim()
        ));
    }

    let claim = get_json(ctx, cluster, &["-n", NAMESPACE, "get", "pvc", CLAIM]).await?;
    let class = claim["spec"]["storageClassName"]
        .as_str()
        .unwrap_or_default();
    if class != s.class {
        problems.push(format!(
            "the claim {CLAIM} is in class {class}, not {}; a claim's class can't change, \
             so move the data to a new claim or set forgejo_storage_class={class}",
            s.class
        ));
    }
    if claim["status"]["phase"] != "Bound" {
        problems.push(format!(
            "the claim {CLAIM} is {}, not Bound",
            claim["status"]["phase"].as_str().unwrap_or("unknown")
        ));
    } else if let Some(pv) = claim["spec"]["volumeName"].as_str() {
        let volume = get_json(ctx, cluster, &["get", "pv", pv]).await?;
        let driver = volume["spec"]["csi"]["driver"].as_str().unwrap_or("none");
        if driver != DRIVER && s.class == GENERIC_CLASS {
            problems.push(format!("the volume {pv} comes from {driver}, not {DRIVER}"));
        } else {
            ctx.log(&format!(
                "data on {pv} ({}, {driver})",
                volume["spec"]["capacity"]["storage"]
                    .as_str()
                    .unwrap_or("?")
            ));
        }
    }

    let host = s.host.as_deref().unwrap_or_default();
    let cert = kubectl(ctx, cluster)
        .args([
            "-n",
            NAMESPACE,
            "get",
            "certificate",
            TLS_SECRET,
            "-o",
            "jsonpath={.status.conditions[?(@.type==\"Ready\")].status}",
        ])
        .any_code()
        .read_only()
        .output()
        .await?;
    if cert.stdout.trim() != "True" {
        ctx.note(&format!(
            "the certificate for {host} is not ready yet; once DNS points at the workers \
             and the {ISSUER} ClusterIssuer exists, cert-manager gets it \
             (kubectl -n {NAMESPACE} describe certificate {TLS_SECRET})"
        ));
    }

    if !problems.is_empty() {
        bail!("{}\n\n{}", problems.join("\n"), logs(ctx, cluster).await);
    }
    ctx.log(&format!("https://{host} is up"));
    ctx.note(&format!(
        "log in as {}: kubectl -n {NAMESPACE} get secret {ADMIN_SECRET} \
         -o jsonpath='{{.data.password}}' | base64 -d",
        s.admin
    ));
    Ok(Outcome::Ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(ssh_port: Option<u16>) -> Settings {
        Settings {
            host: Some("git.example.com".into()),
            size: "10Gi".into(),
            class: GENERIC_CLASS.into(),
            admin: "usecode".into(),
            admin_email: "admin@git.example.com".into(),
            ssh_port,
            version: CHART_VERSION.into(),
            dns_target: None,
            dns_zone: "example.com".into(),
        }
    }

    #[test]
    fn keeps_the_data_on_a_hetzner_volume() {
        let v = settings(None).values();
        assert_eq!(v["persistence"]["storageClass"], "hcloud-volumes");
        assert_eq!(v["persistence"]["claimName"], CLAIM);
        assert_eq!(
            v["persistence"]["annotations"]["helm.sh/resource-policy"],
            "keep"
        );
        assert_eq!(v["strategy"]["type"], "Recreate");
    }

    #[test]
    fn runs_on_hetzner_workers_only() {
        let v = settings(None).values();
        let terms = &v["affinity"]["nodeAffinity"]["requiredDuringSchedulingIgnoredDuringExecution"]
            ["nodeSelectorTerms"];
        let exprs = terms[0]["matchExpressions"].as_array().unwrap();
        assert!(exprs.contains(&json!(
            {"key": CONTROL_PLANE, "operator": "DoesNotExist"}
        )));
        assert!(exprs.contains(&json!(
            {"key": PROVIDER_KEY, "operator": "In", "values": [PROVIDER]}
        )));
    }

    #[test]
    fn reads_quantities() {
        assert_eq!(bytes("10Gi"), Some(10 << 30));
        assert_eq!(bytes("20G"), Some(20_000_000_000));
        assert!(bytes("20Gi") > bytes("10Gi"));
        assert_eq!(bytes("lots"), None);
    }

    #[test]
    fn serves_https_on_the_host() {
        let v = settings(None).values();
        assert_eq!(v["ingress"]["hosts"][0]["host"], "git.example.com");
        assert_eq!(v["ingress"]["tls"][0]["hosts"][0], "git.example.com");
        assert_eq!(
            v["gitea"]["config"]["server"]["ROOT_URL"],
            "https://git.example.com/"
        );
    }

    #[test]
    fn ssh_only_when_asked() {
        let off = settings(None).values();
        assert_eq!(off["gitea"]["config"]["server"]["DISABLE_SSH"], true);
        assert!(off.get("service").is_none());
        let on = settings(Some(2222)).values();
        assert_eq!(on["gitea"]["config"]["server"]["SSH_PORT"], 2222);
        assert_eq!(on["service"]["ssh"]["type"], "LoadBalancer");
        assert_eq!(on["service"]["ssh"]["port"], 2222);
    }
}
