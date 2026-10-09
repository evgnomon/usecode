// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `registry`: your own private container registry on the cluster (CNCF
//! Distribution, `registry:3`).
//!
//! The images live on one Hetzner Cloud Volume from
//! [`hcloud_csi`](super::hcloud_csi), in the `hcloud-volumes` class, so the
//! registry can move to any Hetzner worker in the primary location and keep
//! them. It runs on worker nodes only, never on a control plane. Removing
//! the deployment leaves the volume; only deleting the claim deletes it.
//!
//! It answers on `https://<registry_host>` through k3s's Traefik, with a
//! Let's Encrypt certificate from the `letsencrypt` ClusterIssuer that
//! `lib/infra/bootstrap.sh` sets up. With `registry_dns_target` the role
//! keeps the name a CNAME to it on Cloudflare's API; otherwise point it
//! at the workers yourself (`lib/infra/dns.sh <registry_host>`).
//!
//! Variables (`-e` or the user config):
//!
//! * `registry_host`: the name the registry answers on, e.g.
//!   `registry.example.com`. Without it the role does nothing.
//! * `registry_size`: the volume size (default: 10Gi, Hetzner's smallest).
//!   It can grow later, never shrink: a bigger volume keeps its size.
//! * `registry_storage_class`: (default: `hcloud-volumes`).
//! * `registry_user`: the login (default: `usecode`).
//! * `registry_dns_target`, `registry_dns_zone`: as `forgejo_dns_target`
//!   and `forgejo_dns_zone` in [`forgejo`](super::forgejo).
//!
//! The password is made once, at random, and kept in registry/registry-auth:
//! the secret is the source of truth, so changing it is editing the secret
//! and restarting the deployment. The htpasswd file is built from it on
//! every start.

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

const NAMESPACE: &str = "registry";
const NAME: &str = "registry";
const IMAGE: &str = "registry:3";
const CLAIM: &str = "registry-data";
/// The claim the registry `lib/infra/bootstrap.sh` used to install kept its
/// images on, on one node's disk.
const OLD_CLAIM: &str = "registry";
const AUTH_SECRET: &str = "registry-auth";
const TLS_SECRET: &str = "registry-tls";
const ISSUER: &str = "letsencrypt";
const PORT: u16 = 5000;

/// What the user decided, from the variables.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub host: Option<String>,
    pub size: String,
    pub class: String,
    pub user: String,
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
        let host = name(var(vars, "registry_host")?);
        let dns_target = name(var(vars, "registry_dns_target")?);
        if dns_target.is_some() && dns_target == host {
            bail!("registry_dns_target can't be registry_host itself");
        }
        let dns_zone = name(var(vars, "registry_dns_zone")?).unwrap_or_else(|| {
            let host = host.as_deref().unwrap_or_default();
            host.split_once('.')
                .map_or(host, |(_, zone)| zone)
                .to_string()
        });
        Ok(Settings {
            host,
            size: var(vars, "registry_size")?.unwrap_or_else(|| "10Gi".into()),
            class: var(vars, "registry_storage_class")?.unwrap_or_else(|| GENERIC_CLASS.into()),
            user: var(vars, "registry_user")?.unwrap_or_else(|| "usecode".into()),
            dns_target,
            dns_zone,
        })
    }

    /// Everything the registry is, as one kubectl `List`, with the claim at
    /// `size`.
    pub fn manifest(&self, size: &str) -> Value {
        let host = self.host.as_deref().unwrap_or_default();
        let meta = |name: &str| json!({"name": name, "namespace": NAMESPACE});
        let secret = |key: &str| json!({"secretKeyRef": {"name": AUTH_SECRET, "key": key}});
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
                        "initContainers": [{
                            "name": "htpasswd",
                            "image": "httpd:2.4-alpine",
                            "command": ["sh", "-c",
                                "printf '%s' \"$PASSWORD\" | htpasswd -iBc /auth/htpasswd \"$USERNAME\""],
                            "env": [
                                {"name": "USERNAME", "valueFrom": secret("username")},
                                {"name": "PASSWORD", "valueFrom": secret("password")},
                            ],
                            "volumeMounts": [{"name": "auth", "mountPath": "/auth"}],
                        }],
                        "containers": [{
                            "name": NAME,
                            "image": IMAGE,
                            "ports": [{"containerPort": PORT}],
                            "env": [
                                {"name": "REGISTRY_AUTH", "value": "htpasswd"},
                                {"name": "REGISTRY_AUTH_HTPASSWD_REALM", "value": NAME},
                                {"name": "REGISTRY_AUTH_HTPASSWD_PATH", "value": "/auth/htpasswd"},
                                {"name": "REGISTRY_STORAGE_DELETE_ENABLED", "value": "true"},
                                {"name": "REGISTRY_HTTP_RELATIVEURLS", "value": "true"},
                                {"name": "REGISTRY_HTTP_SECRET", "valueFrom": secret("http-secret")},
                            ],
                            "volumeMounts": [
                                {"name": "auth", "mountPath": "/auth", "readOnly": true},
                                {"name": "data", "mountPath": "/var/lib/registry"},
                            ],
                            "readinessProbe": {"httpGet": {"path": "/", "port": PORT}},
                            "resources": {
                                "requests": {"cpu": "50m", "memory": "64Mi"},
                                "limits": {"memory": "512Mi"},
                            },
                        }],
                        "volumes": [
                            {"name": "auth", "emptyDir": {"medium": "Memory"}},
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
                "ports": [{"port": PORT, "targetPort": PORT}],
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
        Task::new(
            "registry/report",
            "Plan the registry: its name, volume and login",
        )
        .tags(&["registry"])
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
        Task::new("registry/namespace", "Create the registry namespace")
            .tags(&["registry"])
            .after(["registry/report"])
            .run(move |ctx| async move { apply(&ctx, &c, &namespace).await }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new("registry/auth", "Keep the login in registry/registry-auth")
            .tags(&["registry"])
            .after(["registry/namespace"])
            .run(move |ctx| async move { auth(&ctx, &c, &s).await }),
    );

    let s = settings.clone();
    plan.add(
        Task::new(
            "registry/dns",
            "Point the name at the workers on Cloudflare",
        )
        .tags(&["registry", "dns"])
        .after(["registry/report"])
        .when(
            settings.dns_target.is_some(),
            "registry_dns_target is not set; the name is yours to point at the workers",
        )
        .run(move |ctx| async move {
            let host = s.host.as_deref().unwrap_or_default();
            let target = s.dns_target.as_deref().unwrap_or_default();
            dns(&ctx, host, target, &s.dns_zone).await
        }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new("registry/install", "Run the registry on its Hetzner Volume")
            .tags(&["registry"])
            .after([
                "registry/auth",
                // cert-manager asks Let's Encrypt as soon as the Ingress is
                // there, so the name has to resolve by then.
                "registry/dns",
                // Its volume needs the driver and the storage class.
                "hcloud_csi/storage-classes",
            ])
            .run(move |ctx| async move { install(&ctx, &c, &s).await }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "registry/verify",
            "Check the registry runs on its Hetzner Volume and has its certificate",
        )
        .tags(&["registry"])
        .after(["registry/install"])
        .run(move |ctx| async move { verify(&ctx, &c, &s).await }),
    );

    plan.gate(
        start,
        settings.host.is_some(),
        &unset(cluster, "registry_host", "registry.example.com"),
    );
    Ok(())
}

async fn report(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    let host = s.host.clone().unwrap_or_default();
    ctx.note(&format!(
        "https://{host}, {} on {} (claim {NAMESPACE}/{CLAIM}), login {}",
        s.size, s.class, s.user
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
            "the registry runs on Hetzner Cloud workers only, and there is none{place}: \
             add a worker there, or pick a class in a location that has one \
             (registry_storage_class)"
        )),
        1 => ctx.note(&format!(
            "warning: {} is the only worker{place}: if it goes down, the registry waits \
             for it, since its volume never leaves that location",
            usable[0]
        )),
        _ => ctx.note(&format!(
            "runs on the workers{place}: {}",
            usable.join(", ")
        )),
    }

    if let Some(old) = old_claim(cluster) {
        ctx.note(&format!(
            "the registry lib/infra/bootstrap.sh used to install kept its images on {NAMESPACE}/{OLD_CLAIM} \
             ({}); this one starts empty on {CLAIM}, so push your images again",
            old.class
        ));
    }

    // registry/dns sees to the name when it manages the record.
    if s.dns_target.is_none() {
        check_resolves(ctx, cluster, &host).await?;
    }

    if problems.is_empty() {
        return Ok(Outcome::Ok);
    }
    bail!("fix these first:\n- {}", problems.join("\n- "))
}

/// The claim of the registry this role replaces, while it is still there.
fn old_claim(cluster: &Cluster) -> Option<&crate::kube::cluster::Claim> {
    cluster
        .claims
        .iter()
        .find(|c| c.namespace == NAMESPACE && c.name == OLD_CLAIM)
}

async fn auth(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    let found = kubectl(ctx, cluster)
        .args(["-n", NAMESPACE, "get", "secret", AUTH_SECRET, "-o", "name"])
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
    // `create`, not `apply`: apply would keep a copy of the password in the
    // last-applied annotation.
    let secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {"name": AUTH_SECRET, "namespace": NAMESPACE},
        "stringData": {
            "username": s.user,
            "password": random(32)?,
            "http-secret": random(64)?,
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
    // A claim can't shrink: one bigger than registry_size keeps its size.
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
            "{CLAIM} keeps its {current}: a volume grows, never shrinks, so registry_size={} \
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

/// The tail of the registry's logs and the claim's events, for a failure
/// message.
async fn logs(ctx: &Ctx, cluster: &Cluster) -> String {
    let deployment = format!("deployment/{NAME}");
    let mut text = String::new();
    for args in [
        vec!["logs", deployment.as_str(), "--all-containers", "--tail=30"],
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
             so move the data to a new claim or set registry_storage_class={class}",
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
                "images on {pv} ({}, {driver})",
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
        "log in: kubectl -n {NAMESPACE} get secret {AUTH_SECRET} -o jsonpath='{{.data.password}}' \
         | base64 -d | docker login {host} -u {} --password-stdin",
        s.user
    ));
    if old_claim(cluster).is_some() {
        ctx.note(&format!(
            "once your images are pushed again, free the old one's disk: \
             kubectl -n {NAMESPACE} delete pvc {OLD_CLAIM}"
        ));
    }
    Ok(Outcome::Ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            host: Some("registry.example.com".into()),
            size: "10Gi".into(),
            class: GENERIC_CLASS.into(),
            user: "usecode".into(),
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
    fn keeps_the_images_on_a_hetzner_volume() {
        let s = settings();
        let m = s.manifest(&s.size);
        let claim = item(&m, "PersistentVolumeClaim");
        assert_eq!(claim["metadata"]["name"], CLAIM);
        assert_eq!(claim["spec"]["storageClassName"], "hcloud-volumes");
        assert_eq!(claim["spec"]["resources"]["requests"]["storage"], "10Gi");
        let deployment = item(&m, "Deployment");
        assert_eq!(deployment["spec"]["strategy"]["type"], "Recreate");
        let volumes = &deployment["spec"]["template"]["spec"]["volumes"];
        assert!(volumes.as_array().unwrap().contains(&json!(
            {"name": "data", "persistentVolumeClaim": {"claimName": CLAIM}}
        )));
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
    fn serves_https_on_the_host() {
        let m = settings().manifest("10Gi");
        let ingress = item(&m, "Ingress");
        assert_eq!(ingress["spec"]["rules"][0]["host"], "registry.example.com");
        assert_eq!(
            ingress["spec"]["tls"][0]["hosts"][0],
            "registry.example.com"
        );
        assert_eq!(ingress["spec"]["tls"][0]["secretName"], TLS_SECRET);
    }
}
