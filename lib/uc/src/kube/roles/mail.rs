// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `mail`: your own mail server on the cluster, [Stalwart]
//! (`stalwartlabs/stalwart`): SMTP, submission, IMAP and JMAP in one.
//!
//! Its configuration and mail live on one Hetzner Cloud Volume from
//! [`hcloud_csi`](super::hcloud_csi), in the `hcloud-volumes` class, so
//! Stalwart can move to any Hetzner worker in the primary location and keep
//! them. It runs on worker nodes only, never on a control plane. Removing
//! the deployment leaves the volume; only deleting the claim deletes it.
//!
//! The web admin is on `https://<mail_host>/admin` through k3s's Traefik,
//! with a Let's Encrypt certificate from the `letsencrypt` ClusterIssuer that
//! `lib/infra/bootstrap.sh` sets up. The mail ports (25, 465, 587, 993,
//! 995) are a k3s ServiceLB `LoadBalancer` with `externalTrafficPolicy:
//! Local`, so Stalwart sees the senders' real addresses for SPF, DMARC and
//! its spam checks, and only the node Stalwart runs on answers. The same
//! certificate is mounted at `/tls` for those ports, and a renewed one
//! restarts Stalwart on the next run.
//!
//! Stalwart keeps its settings in its own store, so the role puts the server
//! there, its setup wizard makes the domains and accounts, and
//! [`uc mail configure`](crate::mail::configure) does the rest: DNS, reverse
//! DNS and giving the mail ports the certificate.
//!
//! [Stalwart]: https://stalw.art
//!
//! Variables (`-e` or the user config):
//!
//! * `mail_host`: the name the server answers on, e.g. `mail.example.com`.
//!   Without it the role does nothing.
//! * `mail_size`: the volume size (default: 10Gi, Hetzner's smallest). It
//!   can grow later, never shrink: a bigger volume keeps its size.
//! * `mail_storage_class`: (default: `hcloud-volumes`).
//! * `mail_admin`: the recovery admin's login (default: `admin`).
//! * `mail_recovery_admin`: keep the recovery admin (default: true). Set it
//!   to false once you have an admin of your own.
//! * `mail_version`: the image tag (default: `v0.16`).
//! * `mail_node`: the worker Stalwart always runs on, e.g. `worker-1`. Its
//!   address is the mail name's, its reverse DNS and what SPF names, so
//!   pinning it means a new worker in the volume's location can never take
//!   Stalwart, and with it the mail, somewhere DNS doesn't point. Without
//!   it Stalwart may run on any worker there; `mail/verify` then tells when
//!   DNS no longer follows (`uc mail configure` fixes it).
//!
//! The recovery admin's password is made once, at random, and kept in
//! mail/mail-admin: the secret is the source of truth, so changing it is
//! editing the secret and restarting the deployment.

use crate::configure::ctx::Ctx;
use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::vars::Vars;
use crate::kube::cloudflare;
use crate::kube::cluster::{CONTROL_PLANE, Cluster};
use crate::kube::roles::forgejo::{bytes, volume_workers};
use crate::kube::roles::hcloud_csi::{DRIVER, GENERIC_CLASS, PROVIDER, PROVIDER_KEY};
use crate::kube::{apply, get_json, kubectl, unset, var};
use crate::password::{self, Charset};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::net::IpAddr;
use std::sync::Arc;

pub const NAMESPACE: &str = "mail";
pub const NAME: &str = "stalwart";
const IMAGE: &str = "stalwartlabs/stalwart";
const CLAIM: &str = "mail-data";
pub const ADMIN_SECRET: &str = "mail-admin";
pub const TLS_SECRET: &str = "mail-tls";
const ISSUER: &str = "letsencrypt";
/// Stalwart's plain HTTP listener, which Traefik talks to.
pub const HTTP_PORT: u16 = 8080;
/// The image runs as this user; the volume is made writable for it.
const UID: u32 = 2000;
/// The mail ports the nodes answer on.
const MAIL_PORTS: [(&str, u16); 5] = [
    ("smtp", 25),
    ("submissions", 465),
    ("submission", 587),
    ("imaps", 993),
    ("pop3s", 995),
];
/// The label every node has with its name.
const HOSTNAME: &str = "kubernetes.io/hostname";
/// The pod annotation that restarts Stalwart when its certificate is renewed.
const TLS_ANNOTATION: &str = "usecode.dev/tls-not-after";

/// What the user decided, from the variables.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub host: Option<String>,
    pub size: String,
    pub class: String,
    pub admin: String,
    pub recovery_admin: bool,
    pub version: String,
    /// The worker Stalwart is pinned to.
    pub node: Option<String>,
}

impl Settings {
    pub fn load(vars: &Vars) -> Result<Settings> {
        let name = |v: Option<String>| {
            v.map(|n| n.trim().trim_end_matches('.').to_lowercase())
                .filter(|n| !n.is_empty())
        };
        let host = name(var(vars, "mail_host")?);
        Ok(Settings {
            host,
            size: var(vars, "mail_size")?.unwrap_or_else(|| "10Gi".into()),
            class: var(vars, "mail_storage_class")?.unwrap_or_else(|| GENERIC_CLASS.into()),
            admin: var(vars, "mail_admin")?.unwrap_or_else(|| "admin".into()),
            recovery_admin: var(vars, "mail_recovery_admin")?.unwrap_or(true),
            version: var(vars, "mail_version")?.unwrap_or_else(|| "v0.16".into()),
            node: name(var(vars, "mail_node")?),
        })
    }

    /// Everything Stalwart is, as one kubectl `List`, with the claim at
    /// `size` and `tls` the current certificate's expiry, if there is one.
    pub fn manifest(&self, size: &str, tls: Option<&str>) -> Value {
        let host = self.host.as_deref().unwrap_or_default();
        let meta = |name: &str| json!({"name": name, "namespace": NAMESPACE});
        let secret = |key: &str| json!({"secretKeyRef": {"name": ADMIN_SECRET, "key": key}});
        let mut env =
            vec![json!({"name": "STALWART_PUBLIC_URL", "value": format!("https://{host}")})];
        if self.recovery_admin {
            env.extend([
                json!({"name": "ADMIN_USER", "valueFrom": secret("username")}),
                json!({"name": "ADMIN_PASSWORD", "valueFrom": secret("password")}),
                json!({"name": "STALWART_RECOVERY_ADMIN", "value": "$(ADMIN_USER):$(ADMIN_PASSWORD)"}),
            ]);
        }
        let mut ports = vec![json!({"name": "http", "containerPort": HTTP_PORT})];
        ports.extend(
            MAIL_PORTS
                .iter()
                .map(|(name, port)| json!({"name": name, "containerPort": port})),
        );
        let annotations = tls.map_or_else(|| json!({}), |t| json!({TLS_ANNOTATION: t}));
        let mut placement = vec![
            json!({"key": CONTROL_PLANE, "operator": "DoesNotExist"}),
            json!({"key": PROVIDER_KEY, "operator": "In", "values": [PROVIDER]}),
        ];
        if let Some(node) = &self.node {
            placement.push(json!({"key": HOSTNAME, "operator": "In", "values": [node]}));
        }
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
                    "metadata": {"labels": {"app": NAME}, "annotations": annotations},
                    "spec": {
                        // Workers only, and Hetzner Cloud ones, which volumes
                        // reach; the one in mail_node when it's set.
                        "affinity": {"nodeAffinity": {"requiredDuringSchedulingIgnoredDuringExecution": {
                            "nodeSelectorTerms": [{"matchExpressions": placement}]
                        }}},
                        "securityContext": {"fsGroup": UID},
                        "containers": [{
                            "name": NAME,
                            "image": format!("{IMAGE}:{}", self.version),
                            "ports": ports,
                            "env": env,
                            "volumeMounts": [
                                {"name": "data", "mountPath": "/etc/stalwart", "subPath": "etc"},
                                {"name": "data", "mountPath": "/var/lib/stalwart", "subPath": "data"},
                                {"name": "tls", "mountPath": "/tls", "readOnly": true},
                            ],
                            "readinessProbe": {"tcpSocket": {"port": HTTP_PORT}},
                            "resources": {
                                "requests": {"cpu": "100m", "memory": "256Mi"},
                                "limits": {"memory": "2Gi"},
                            },
                        }],
                        "volumes": [
                            {"name": "data", "persistentVolumeClaim": {"claimName": CLAIM}},
                            // cert-manager makes it once the name resolves.
                            {"name": "tls", "secret": {"secretName": TLS_SECRET, "optional": true}},
                        ],
                    },
                },
            },
        });
        let web = json!({
            "apiVersion": "v1",
            "kind": "Service",
            "metadata": meta(NAME),
            "spec": {
                "selector": {"app": NAME},
                "ports": [{"name": "http", "port": HTTP_PORT, "targetPort": HTTP_PORT}],
            },
        });
        let mail = json!({
            "apiVersion": "v1",
            "kind": "Service",
            "metadata": meta("mail"),
            "spec": {
                "type": "LoadBalancer",
                // Keeps the senders' addresses, which SPF and spam checks need.
                "externalTrafficPolicy": "Local",
                "selector": {"app": NAME},
                "ports": MAIL_PORTS.iter().map(|(name, port)| json!(
                    {"name": name, "port": port, "targetPort": port, "protocol": "TCP"}
                )).collect::<Vec<_>>(),
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
                    "backend": {"service": {"name": NAME, "port": {"number": HTTP_PORT}}},
                }]}}],
            },
        });
        json!({
            "apiVersion": "v1",
            "kind": "List",
            "items": [claim, deployment, web, mail, redirect, ingress],
        })
    }
}

pub fn tasks(plan: &mut Plan, cluster: &Arc<Cluster>, vars: &Vars) -> Result<()> {
    let settings = Arc::new(Settings::load(vars)?);
    let start = plan.tasks().len();

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "mail/report",
            "Plan the mail server: its name, volume and ports",
        )
        .tags(&["mail"])
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
        Task::new("mail/namespace", "Create the mail namespace")
            .tags(&["mail"])
            .after(["mail/report"])
            .run(move |ctx| async move { apply(&ctx, &c, &namespace).await }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new("mail/admin", "Keep the recovery admin in mail/mail-admin")
            .tags(&["mail"])
            .after(["mail/namespace"])
            .run(move |ctx| async move { admin(&ctx, &c, &s).await }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new("mail/install", "Run Stalwart on its Hetzner Volume")
            .tags(&["mail"])
            .after([
                "mail/admin",
                // Its volume needs the driver and the storage class.
                "hcloud_csi/storage-classes",
            ])
            .run(move |ctx| async move { install(&ctx, &c, &s).await }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "mail/verify",
            "Check Stalwart runs on its Hetzner Volume and has its certificate",
        )
        .tags(&["mail"])
        .after(["mail/install"])
        .run(move |ctx| async move { verify(&ctx, &c, &s).await }),
    );

    plan.gate(
        start,
        settings.host.is_some(),
        &unset(cluster, "mail_host", "mail.example.com"),
    );
    Ok(())
}

async fn report(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    let host = s.host.clone().unwrap_or_default();
    ctx.note(&format!(
        "https://{host}/admin, Stalwart {} with {} on {} (claim {NAMESPACE}/{CLAIM})",
        s.version, s.size, s.class
    ));
    ctx.note(&format!(
        "mail ports {} on the node Stalwart runs on",
        MAIL_PORTS.map(|(_, p)| p.to_string()).join(", ")
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
    if let Some(node) = &s.node {
        if usable.contains(node) {
            ctx.note(&format!("pinned to {node} (mail_node)"));
        } else {
            problems.push(format!(
                "mail_node is {node}, which is not a Hetzner Cloud worker{place} that the \
                 volume reaches; pick one of: {}",
                if usable.is_empty() {
                    "none".into()
                } else {
                    usable.join(", ")
                }
            ));
        }
    }
    match usable.len() {
        0 => problems.push(format!(
            "Stalwart runs on Hetzner Cloud workers only, and there is none{place}: \
             add a worker there, or pick a class in a location that has one \
             (mail_storage_class)"
        )),
        1 => ctx.note(&format!(
            "warning: {} is the only worker{place}: if it goes down, mail waits \
             for it, since its volume never leaves that location",
            usable[0]
        )),
        _ if s.node.is_none() => ctx.note(&format!(
            "runs on the workers{place}: {}; it can move between them, and the mail \
             name and reverse DNS only follow with `uc mail configure`, so consider \
             pinning it with mail_node",
            usable.join(", ")
        )),
        _ => {}
    }

    if problems.is_empty() {
        return Ok(Outcome::Ok);
    }
    bail!("fix these first:\n- {}", problems.join("\n- "))
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

/// When the current certificate for the mail ports expires, once
/// cert-manager has one.
async fn certificate_expiry(ctx: &Ctx, cluster: &Cluster) -> Result<Option<String>> {
    let out = kubectl(ctx, cluster)
        .args(["-n", NAMESPACE, "get", "certificate", TLS_SECRET])
        .args(["-o", "jsonpath={.status.notAfter}"])
        .any_code()
        .read_only()
        .output()
        .await?;
    let after = out.stdout.trim();
    Ok((out.success() && !after.is_empty()).then(|| after.to_string()))
}

async fn install(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    // A claim can't shrink: one bigger than mail_size keeps its size.
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
            "{CLAIM} keeps its {current}: a volume grows, never shrinks, so mail_size={} \
             only applies to a new one",
            s.size
        ));
        current
    } else {
        &s.size
    };
    let tls = certificate_expiry(ctx, cluster).await?;
    let manifest = s.manifest(size, tls.as_deref()).to_string();
    let outcome = apply(ctx, cluster, &manifest).await?;
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

/// The tail of Stalwart's logs and the claim's events, for a failure
/// message.
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
             so move the data to a new claim or set mail_storage_class={class}",
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
                "mail on {pv} ({}, {driver})",
                volume["spec"]["capacity"]["storage"]
                    .as_str()
                    .unwrap_or("?")
            ));
        }
    }

    let host = s.host.as_deref().unwrap_or_default();
    match certificate_expiry(ctx, cluster).await? {
        Some(after) => ctx.log(&format!("certificate for {host} until {after}")),
        None => ctx.note(&format!(
            "the certificate for {host} is not ready yet; once `uc mail configure` \
             points the name at the mail node and the {ISSUER} ClusterIssuer exists, \
             cert-manager gets it (kubectl -n {NAMESPACE} describe certificate \
             {TLS_SECRET}); run `uc mail configure` again then, so the mail ports get it too"
        )),
    }

    // With externalTrafficPolicy Local only Stalwart's node answers.
    let pods = get_json(
        ctx,
        cluster,
        &["-n", NAMESPACE, "get", "pods", "-l", &format!("app={NAME}")],
    )
    .await?;
    if let Some(node) = pods["items"][0]["spec"]["nodeName"].as_str() {
        ctx.log(&format!("the mail ports answer on {node}"));
    }
    if let Some(problem) = dns_follows(ctx, cluster, host).await? {
        problems.push(problem);
    }

    if !problems.is_empty() {
        bail!("{}\n\n{}", problems.join("\n"), logs(ctx, cluster).await);
    }
    ctx.log(&format!("https://{host}/admin is up"));
    if s.recovery_admin {
        ctx.note(&format!(
            "admin password: kubectl -n {NAMESPACE} get secret {ADMIN_SECRET} \
             -o jsonpath='{{.data.password}}' | base64 -d  (user {})",
            s.admin
        ));
    }
    ctx.note(&format!(
        "first time? In the wizard at https://{host}/admin: server hostname {host}, keep \
         the HTTP listener on {HTTP_PORT} (Traefik talks to it). Stalwart reads that setup \
         only when it starts, so restart it once the wizard is done (kubectl -n \
         {NAMESPACE} rollout restart deployment/{NAME}); before that it shows the wizard \
         again. Then add your domain, and `uc mail configure` does its DNS, the workers' \
         reverse DNS and the certificate on the mail ports"
    ));
    ctx.note(
        "Hetzner blocks outgoing mail on ports 25 and 465 for new projects: ask them to \
         lift it",
    );
    Ok(Outcome::Ok)
}

/// Whether the mail name points where the mail ports answer, which a move
/// to another worker changes: other servers would deliver to a node that
/// drops the connection.
async fn dns_follows(ctx: &Ctx, cluster: &Cluster, host: &str) -> Result<Option<String>> {
    let service = get_json(ctx, cluster, &["-n", NAMESPACE, "get", "service", "mail"]).await?;
    let answers: BTreeSet<IpAddr> = service["status"]["loadBalancer"]["ingress"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| i["ip"].as_str()?.parse().ok())
        .collect();
    if answers.is_empty() {
        ctx.note("the mail ports have no address yet (kubectl -n mail describe service mail)");
        return Ok(None);
    }
    let resolved: BTreeSet<IpAddr> = cloudflare::resolve(host)
        .await?
        .into_iter()
        .filter(IpAddr::is_ipv4)
        .collect();
    let list = |ips: &BTreeSet<IpAddr>| {
        ips.iter()
            .map(IpAddr::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    if resolved.is_empty() {
        ctx.note(&format!(
            "{host} does not resolve yet; `uc mail configure` points it at {}",
            list(&answers)
        ));
        return Ok(None);
    }
    if resolved != answers {
        return Ok(Some(format!(
            "{host} points at {}, but the mail ports answer on {}, so mail to it doesn't \
             arrive; run `uc mail configure` (and pin Stalwart with mail_node)",
            list(&resolved),
            list(&answers)
        )));
    }
    ctx.log(&format!("{host} points where the mail ports answer"));
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            host: Some("mail.example.com".into()),
            size: "10Gi".into(),
            class: GENERIC_CLASS.into(),
            admin: "admin".into(),
            recovery_admin: true,
            version: "v0.16".into(),
            node: None,
        }
    }

    fn items<'a>(list: &'a Value, kind: &str) -> Vec<&'a Value> {
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["kind"] == kind)
            .collect()
    }

    fn item<'a>(list: &'a Value, kind: &str) -> &'a Value {
        items(list, kind)[0]
    }

    fn container(m: &Value) -> &Value {
        &item(m, "Deployment")["spec"]["template"]["spec"]["containers"][0]
    }

    #[test]
    fn keeps_the_mail_on_a_hetzner_volume() {
        let s = settings();
        let m = s.manifest(&s.size, None);
        let claim = item(&m, "PersistentVolumeClaim");
        assert_eq!(claim["metadata"]["name"], CLAIM);
        assert_eq!(claim["spec"]["storageClassName"], "hcloud-volumes");
        assert_eq!(claim["spec"]["resources"]["requests"]["storage"], "10Gi");
        let deployment = item(&m, "Deployment");
        assert_eq!(deployment["spec"]["strategy"]["type"], "Recreate");
        let mounts = container(&m)["volumeMounts"].as_array().unwrap();
        for path in ["/etc/stalwart", "/var/lib/stalwart"] {
            assert!(
                mounts
                    .iter()
                    .any(|v| v["name"] == "data" && v["mountPath"] == path)
            );
        }
    }

    #[test]
    fn runs_on_hetzner_workers_only() {
        let m = settings().manifest("10Gi", None);
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
    fn mail_node_pins_stalwart_to_one_worker() {
        let pinned = |s: &Settings| {
            let m = s.manifest("10Gi", None);
            item(&m, "Deployment")["spec"]["template"]["spec"]["affinity"]["nodeAffinity"]
                ["requiredDuringSchedulingIgnoredDuringExecution"]["nodeSelectorTerms"][0]
                ["matchExpressions"]
                .as_array()
                .unwrap()
                .contains(&json!({"key": HOSTNAME, "operator": "In", "values": ["worker-1"]}))
        };
        let mut s = settings();
        assert!(!pinned(&s));
        s.node = Some("worker-1".into());
        assert!(pinned(&s));
    }

    #[test]
    fn serves_the_mail_ports_keeping_client_addresses() {
        let m = settings().manifest("10Gi", None);
        let mail = items(&m, "Service")
            .into_iter()
            .find(|s| s["spec"]["type"] == "LoadBalancer")
            .unwrap();
        assert_eq!(mail["spec"]["externalTrafficPolicy"], "Local");
        let ports: Vec<_> = mail["spec"]["ports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["port"].as_u64().unwrap())
            .collect();
        assert_eq!(ports, [25, 465, 587, 993, 995]);
    }

    #[test]
    fn serves_the_admin_on_the_host() {
        let m = settings().manifest("10Gi", None);
        let ingress = item(&m, "Ingress");
        assert_eq!(ingress["spec"]["rules"][0]["host"], "mail.example.com");
        assert_eq!(ingress["spec"]["tls"][0]["secretName"], TLS_SECRET);
        let env = container(&m)["env"].as_array().unwrap();
        assert!(env.contains(&json!(
            {"name": "STALWART_PUBLIC_URL", "value": "https://mail.example.com"}
        )));
    }

    #[test]
    fn the_recovery_admin_can_go() {
        let has = |s: &Settings| {
            container(&s.manifest("10Gi", None))["env"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["name"] == "STALWART_RECOVERY_ADMIN")
        };
        let mut s = settings();
        assert!(has(&s));
        s.recovery_admin = false;
        assert!(!has(&s));
    }

    #[test]
    fn a_renewed_certificate_restarts_stalwart() {
        let s = settings();
        let annotations = |tls| {
            item(&s.manifest("10Gi", tls), "Deployment")["spec"]["template"]["metadata"]
                ["annotations"]
                .clone()
        };
        assert_eq!(annotations(None), json!({}));
        assert_eq!(
            annotations(Some("2027-01-01T00:00:00Z"))[TLS_ANNOTATION],
            "2027-01-01T00:00:00Z"
        );
    }
}
