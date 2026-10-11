// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `rustfs`: your own S3-compatible object storage on the cluster, with
//! [RustFS](https://rustfs.com) (`rustfs/rustfs`).
//!
//! The objects live on one Hetzner Cloud Volume from
//! [`hcloud_csi`](super::hcloud_csi), in the `hcloud-volumes` class, so
//! RustFS can move to any Hetzner worker in the primary location and keep
//! them. It runs on worker nodes only, never on a control plane. Removing
//! the deployment leaves the volume; only deleting the claim deletes it.
//!
//! Inside the cluster the S3 API is `http://rustfs.rustfs.svc:9000`. Outside
//! it answers on `https://<rustfs_host>` through k3s's Traefik, with a
//! Let's Encrypt certificate from the `letsencrypt` ClusterIssuer that
//! `lib/infra/bootstrap.sh` sets up. With `rustfs_dns_target` the role keeps
//! the name a CNAME to it on Cloudflare's API; otherwise point it at the
//! workers yourself (`lib/infra/dns.sh <rustfs_host>`). The web console is
//! not exposed: `kubectl -n rustfs port-forward svc/rustfs 9001`.
//!
//! Variables (`-e` or the user config):
//!
//! * `rustfs_host`: the name the S3 API answers on, e.g. `s3.example.com`.
//!   Without it the role does nothing.
//! * `rustfs_size`: the volume size (default: 10Gi, Hetzner's smallest).
//!   It can grow later, never shrink: a bigger volume keeps its size.
//! * `rustfs_storage_class`: (default: `hcloud-volumes`).
//! * `rustfs_dns_target`, `rustfs_dns_zone`: as `forgejo_dns_target` and
//!   `forgejo_dns_zone` in [`forgejo`](super::forgejo).
//!
//! The root access and secret keys are made once, at random, and kept in
//! rustfs/rustfs-root: the secret is the source of truth, so changing them
//! is editing the secret and restarting the deployment.

use crate::configure::ctx::Ctx;
use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::vars::Vars;
use crate::kube::cluster::{CONTROL_PLANE, Cluster};
use crate::kube::roles::forgejo::{bytes, check_resolves, dns, volume_workers};
use crate::kube::roles::hcloud_csi::{DRIVER, GENERIC_CLASS, PROVIDER, PROVIDER_KEY};
use crate::kube::{apply, get_json, kubectl, unset, var};
use crate::password::{self, Charset};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::sync::Arc;

const NAMESPACE: &str = "rustfs";
const NAME: &str = "rustfs";
const IMAGE: &str = "rustfs/rustfs:1.0.1";
const CLAIM: &str = "rustfs-data";
const ROOT_SECRET: &str = "rustfs-root";
const TLS_SECRET: &str = "rustfs-tls";
const ISSUER: &str = "letsencrypt";
const PORT: u16 = 9000;
const CONSOLE_PORT: u16 = 9001;
/// The image's user and group.
const UID: u32 = 10001;

/// What the user decided, from the variables.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub host: Option<String>,
    pub size: String,
    pub class: String,
    /// The CNAME target for `host`, when the role manages its record.
    pub dns_target: Option<String>,
    pub dns_zone: String,
}

impl Settings {
    pub fn load(vars: &Vars) -> Result<Settings> {
        let name = |v: Option<String>| {
            v.map(|n| n.trim().trim_end_matches('.').to_lowercase())
                .filter(|n| !n.is_empty())
        };
        let host = name(var(vars, "rustfs_host")?);
        let dns_target = name(var(vars, "rustfs_dns_target")?);
        if dns_target.is_some() && dns_target == host {
            bail!("rustfs_dns_target can't be rustfs_host itself");
        }
        let dns_zone = name(var(vars, "rustfs_dns_zone")?).unwrap_or_else(|| {
            let host = host.as_deref().unwrap_or_default();
            host.split_once('.')
                .map_or(host, |(_, zone)| zone)
                .to_string()
        });
        Ok(Settings {
            host,
            size: var(vars, "rustfs_size")?.unwrap_or_else(|| "10Gi".into()),
            class: var(vars, "rustfs_storage_class")?.unwrap_or_else(|| GENERIC_CLASS.into()),
            dns_target,
            dns_zone,
        })
    }

    /// Everything RustFS is, as one kubectl `List`, with the claim at
    /// `size`.
    pub fn manifest(&self, size: &str) -> Value {
        let host = self.host.as_deref().unwrap_or_default();
        let meta = |name: &str| json!({"name": name, "namespace": NAMESPACE});
        let secret = |key: &str| json!({"secretKeyRef": {"name": ROOT_SECRET, "key": key}});
        let claim = json!({
            "apiVersion": "v1",
            "kind": "PersistentVolumeClaim",
            "metadata": meta(CLAIM),
            "spec": {
                "accessModes": ["ReadWriteOnce"],
                "storageClassName": self.class,
                "resources": {"requests": {"storage": size}},
            },
        });
        let health = |path: &str| json!({"httpGet": {"path": path, "port": PORT}});
        let deployment = json!({
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "metadata": {"name": NAME, "namespace": NAMESPACE, "labels": {"app": NAME}},
            "spec": {
                "replicas": 1,
                // A Hetzner Volume is attached to one node at a time.
                "strategy": {"type": "Recreate"},
                "selector": {"matchLabels": {"app": NAME}},
                "template": {
                    "metadata": {"labels": {"app": NAME}},
                    "spec": {
                        // Workers only, and Hetzner Cloud ones, which volumes reach.
                        "affinity": {"nodeAffinity": {"requiredDuringSchedulingIgnoredDuringExecution": {
                            "nodeSelectorTerms": [{"matchExpressions": [
                                {"key": CONTROL_PLANE, "operator": "DoesNotExist"},
                                {"key": PROVIDER_KEY, "operator": "In", "values": [PROVIDER]},
                            ]}]
                        }}},
                        // A fresh volume belongs to root; the group lets RustFS write it.
                        "securityContext": {
                            "runAsUser": UID,
                            "runAsGroup": UID,
                            "fsGroup": UID,
                            "fsGroupChangePolicy": "OnRootMismatch",
                        },
                        "containers": [{
                            "name": NAME,
                            "image": IMAGE,
                            "ports": [
                                {"name": "s3", "containerPort": PORT},
                                {"name": "console", "containerPort": CONSOLE_PORT},
                            ],
                            "env": [
                                {"name": "RUSTFS_ACCESS_KEY", "valueFrom": secret("access-key")},
                                {"name": "RUSTFS_SECRET_KEY", "valueFrom": secret("secret-key")},
                                // A directory under the mount, away from lost+found.
                                {"name": "RUSTFS_VOLUMES", "value": "/data/rustfs"},
                                {"name": "RUSTFS_ADDRESS", "value": format!(":{PORT}")},
                                {"name": "RUSTFS_CONSOLE_ENABLE", "value": "true"},
                                {"name": "RUSTFS_CONSOLE_ADDRESS", "value": format!(":{CONSOLE_PORT}")},
                                // Empty, so logs go to stdout, where kubectl logs and
                                // obs read them. No `value`: the API server drops "".
                                {"name": "RUSTFS_OBS_LOG_DIRECTORY"},
                            ],
                            "volumeMounts": [{"name": "data", "mountPath": "/data"}],
                            "readinessProbe": health("/health/ready"),
                            "livenessProbe": health("/health"),
                            "resources": {
                                "requests": {"cpu": "100m", "memory": "256Mi"},
                                "limits": {"memory": "2Gi"},
                            },
                        }],
                        "volumes": [
                            {"name": "data", "persistentVolumeClaim": {"claimName": CLAIM}},
                        ],
                    },
                },
            },
        });
        let service = json!({
            "apiVersion": "v1",
            "kind": "Service",
            "metadata": meta(NAME),
            "spec": {
                "selector": {"app": NAME},
                "ports": [
                    {"name": "s3", "port": PORT, "targetPort": PORT},
                    {"name": "console", "port": CONSOLE_PORT, "targetPort": CONSOLE_PORT},
                ],
            },
        });
        let redirect = json!({
            "apiVersion": "traefik.io/v1alpha1",
            "kind": "Middleware",
            "metadata": meta("redirect-https"),
            "spec": {"redirectScheme": {"scheme": "https", "permanent": true}},
        });
        let ingress = json!({
            "apiVersion": "networking.k8s.io/v1",
            "kind": "Ingress",
            "metadata": {
                "name": NAME,
                "namespace": NAMESPACE,
                "annotations": {
                    "cert-manager.io/cluster-issuer": ISSUER,
                    "traefik.ingress.kubernetes.io/router.middlewares":
                        format!("{NAMESPACE}-redirect-https@kubernetescrd"),
                },
            },
            "spec": {
                "ingressClassName": "traefik",
                "tls": [{"hosts": [host], "secretName": TLS_SECRET}],
                "rules": [{"host": host, "http": {"paths": [{
                    "path": "/",
                    "pathType": "Prefix",
                    "backend": {"service": {"name": NAME, "port": {"number": PORT}}},
                }]}}],
            },
        });
        json!({
            "apiVersion": "v1",
            "kind": "List",
            "items": [claim, deployment, service, redirect, ingress],
        })
    }
}

pub fn tasks(plan: &mut Plan, cluster: &Arc<Cluster>, vars: &Vars) -> Result<()> {
    let settings = Arc::new(Settings::load(vars)?);
    let start = plan.tasks().len();

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new("rustfs/report", "Plan RustFS: its name and volume")
            .tags(&["rustfs"])
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
        Task::new("rustfs/namespace", "Create the rustfs namespace")
            .tags(&["rustfs"])
            .after(["rustfs/report"])
            .run(move |ctx| async move { apply(&ctx, &c, &namespace).await }),
    );

    let c = cluster.clone();
    plan.add(
        Task::new("rustfs/auth", "Keep the root keys in rustfs/rustfs-root")
            .tags(&["rustfs"])
            .after(["rustfs/namespace"])
            .run(move |ctx| async move { auth(&ctx, &c).await }),
    );

    let s = settings.clone();
    plan.add(
        Task::new("rustfs/dns", "Point the name at the workers on Cloudflare")
            .tags(&["rustfs", "dns"])
            .after(["rustfs/report"])
            .when(
                settings.dns_target.is_some(),
                "rustfs_dns_target is not set; the name is yours to point at the workers",
            )
            .run(move |ctx| async move {
                let host = s.host.as_deref().unwrap_or_default();
                let target = s.dns_target.as_deref().unwrap_or_default();
                dns(&ctx, host, target, &s.dns_zone).await
            }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new("rustfs/install", "Run RustFS on its Hetzner Volume")
            .tags(&["rustfs"])
            .after([
                "rustfs/auth",
                // cert-manager asks Let's Encrypt as soon as the Ingress is
                // there, so the name has to resolve by then.
                "rustfs/dns",
                // Its volume needs the driver and the storage class.
                "hcloud_csi/storage-classes",
            ])
            .run(move |ctx| async move { install(&ctx, &c, &s).await }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "rustfs/verify",
            "Check RustFS runs on its Hetzner Volume and has its certificate",
        )
        .tags(&["rustfs"])
        .after(["rustfs/install"])
        .run(move |ctx| async move { verify(&ctx, &c, &s).await }),
    );

    plan.gate(
        start,
        settings.host.is_some(),
        &unset(cluster, "rustfs_host", "s3.example.com"),
    );
    Ok(())
}

async fn report(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    let host = s.host.clone().unwrap_or_default();
    ctx.note(&format!(
        "https://{host}, {} on {} (claim {NAMESPACE}/{CLAIM})",
        s.size, s.class
    ));

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
            "RustFS runs on Hetzner Cloud workers only, and there is none{place}: \
             add a worker there, or pick a class in a location that has one \
             (rustfs_storage_class)"
        )),
        1 => ctx.note(&format!(
            "warning: {} is the only worker{place}: if it goes down, RustFS waits \
             for it, since its volume never leaves that location",
            usable[0]
        )),
        _ => ctx.note(&format!(
            "runs on the workers{place}: {}",
            usable.join(", ")
        )),
    }

    // rustfs/dns sees to the name when it manages the record.
    if s.dns_target.is_none() {
        check_resolves(ctx, cluster, &host).await?;
    }

    if problems.is_empty() {
        return Ok(Outcome::Ok);
    }
    bail!("fix these first:\n- {}", problems.join("\n- "))
}

async fn auth(ctx: &Ctx, cluster: &Cluster) -> Result<Outcome> {
    let found = kubectl(ctx, cluster)
        .args(["-n", NAMESPACE, "get", "secret", ROOT_SECRET, "-o", "name"])
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
    let random = |len| {
        password::generate(
            len,
            Charset {
                letters: true,
                digits: true,
                symbols: false,
            },
        )
    };
    // `create`, not `apply`: apply would keep a copy of the keys in the
    // last-applied annotation.
    let secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {"name": ROOT_SECRET, "namespace": NAMESPACE},
        "stringData": {
            "access-key": random(20)?,
            "secret-key": random(40)?,
        },
    });
    kubectl(ctx, cluster)
        .args(["create", "-f", "-"])
        .stdin(secret.to_string())
        .output()
        .await?;
    Ok(Outcome::Changed)
}

async fn install(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    // A claim can't shrink: one bigger than rustfs_size keeps its size.
    let claim = kubectl(ctx, cluster)
        .args(["-n", NAMESPACE, "get", "pvc", CLAIM])
        .args(["-o", "jsonpath={.spec.resources.requests.storage}"])
        .any_code()
        .read_only()
        .output()
        .await?;
    let current = claim.stdout.trim();
    let size = if claim.success() && bytes(current) > bytes(&s.size) {
        ctx.note(&format!(
            "{CLAIM} keeps its {current}: a volume grows, never shrinks, so rustfs_size={} \
             only applies to a new one",
            s.size
        ));
        current
    } else {
        &s.size
    };
    let outcome = apply(ctx, cluster, &s.manifest(size).to_string()).await?;
    if ctx.check() || outcome == Outcome::Ok {
        return Ok(outcome);
    }
    // The first start attaches a new volume.
    let rollout = kubectl(ctx, cluster)
        .args(["-n", NAMESPACE, "rollout", "status"])
        .arg(format!("deployment/{NAME}"))
        .arg("--timeout=600s")
        .output()
        .await;
    if let Err(err) = rollout {
        bail!("{err:#}\n\n{}", logs(ctx, cluster).await);
    }
    Ok(outcome)
}

/// The tail of RustFS's logs and the claim's events, for a failure message.
async fn logs(ctx: &Ctx, cluster: &Cluster) -> String {
    let deployment = format!("deployment/{NAME}");
    let mut text = String::new();
    for args in [
        vec!["logs", deployment.as_str(), "--tail=30"],
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
        .args(["-n", NAMESPACE, "rollout", "status"])
        .arg(format!("deployment/{NAME}"))
        .arg("--timeout=300s")
        .any_code()
        .read_only()
        .output()
        .await?;
    if !rollout.success() {
        problems.push(format!(
            "deployment/{NAME} is not ready: {}",
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
             so move the data to a new claim or set rustfs_storage_class={class}",
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
                "objects on {pv} ({}, {driver})",
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
        "keys: kubectl -n {NAMESPACE} get secret {ROOT_SECRET} -o jsonpath='{{.data.access-key}}' \
         | base64 -d (and secret-key); console: kubectl -n {NAMESPACE} port-forward svc/{NAME} \
         {CONSOLE_PORT}, then http://localhost:{CONSOLE_PORT}"
    ));
    Ok(Outcome::Ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            host: Some("s3.example.com".into()),
            size: "10Gi".into(),
            class: GENERIC_CLASS.into(),
            dns_target: None,
            dns_zone: "example.com".into(),
        }
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
    fn keeps_the_objects_on_a_hetzner_volume() {
        let s = settings();
        let m = s.manifest(&s.size);
        let claim = item(&m, "PersistentVolumeClaim");
        assert_eq!(claim["metadata"]["name"], CLAIM);
        assert_eq!(claim["spec"]["storageClassName"], "hcloud-volumes");
        assert_eq!(claim["spec"]["resources"]["requests"]["storage"], "10Gi");
        let deployment = item(&m, "Deployment");
        assert_eq!(deployment["spec"]["strategy"]["type"], "Recreate");
        let pod = &deployment["spec"]["template"]["spec"];
        assert!(pod["volumes"].as_array().unwrap().contains(&json!(
            {"name": "data", "persistentVolumeClaim": {"claimName": CLAIM}}
        )));
        assert_eq!(pod["securityContext"]["fsGroup"], UID);
    }

    #[test]
    fn a_claim_keeps_a_bigger_size() {
        let m = settings().manifest("20Gi");
        let claim = item(&m, "PersistentVolumeClaim");
        assert_eq!(claim["spec"]["resources"]["requests"]["storage"], "20Gi");
    }

    #[test]
    fn runs_on_hetzner_workers_only() {
        let m = settings().manifest("10Gi");
        let spec = &item(&m, "Deployment")["spec"]["template"]["spec"];
        let exprs = spec["affinity"]["nodeAffinity"]["requiredDuringSchedulingIgnoredDuringExecution"]
            ["nodeSelectorTerms"][0]["matchExpressions"]
            .as_array()
            .unwrap();
        assert!(exprs.contains(&json!(
            {"key": CONTROL_PLANE, "operator": "DoesNotExist"}
        )));
        assert!(exprs.contains(&json!(
            {"key": PROVIDER_KEY, "operator": "In", "values": [PROVIDER]}
        )));
    }

    #[test]
    fn serves_s3_over_https_on_the_host() {
        let m = settings().manifest("10Gi");
        let ingress = item(&m, "Ingress");
        assert_eq!(ingress["spec"]["rules"][0]["host"], "s3.example.com");
        assert_eq!(
            ingress["spec"]["rules"][0]["http"]["paths"][0]["backend"]["service"]["port"]["number"],
            PORT
        );
        assert_eq!(ingress["spec"]["tls"][0]["secretName"], TLS_SECRET);
    }
}
