// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `console`: usecode.dev itself, the landing page, pricing, sign up and log
//! in (`lib/console`), with its own Postgres.
//!
//! Postgres keeps its data on one Hetzner Cloud Volume from
//! [`hcloud_csi`](super::hcloud_csi), in the `hcloud-volumes` class, so it
//! can move to any Hetzner worker in the primary location and keep it.
//! Removing the deployment leaves the volume; only deleting the claim
//! deletes it. The console itself holds nothing and runs as several
//! replicas on the workers.
//!
//! It answers on `https://<console_host>` through k3s's Traefik, with a
//! Let's Encrypt certificate from the `letsencrypt` ClusterIssuer that
//! `lib/infra/bootstrap.sh` sets up. With `console_dns_target` the role
//! keeps the name a CNAME to it on Cloudflare's API; otherwise point it at
//! the workers yourself (`lib/infra/dns.sh <console_host>`).
//!
//! Variables (`-e` or the user config):
//!
//! * `console_host`: the name the console answers on, e.g. `usecode.dev`.
//!   Without it the role does nothing.
//! * `console_image`: the image `make -C lib/console push` pushed (default:
//!   `<registry_host>/usecode-console:latest` when the cluster has
//!   [`registry`](super::registry), else
//!   `ghcr.io/evgnomon/usecode-console:latest`). Pin a tag to roll out a
//!   given build; `:latest` is pulled again on every restart. An image on
//!   `registry_host` is pulled with the registry's login, which the role
//!   copies into console/registry.
//! * `console_replicas`: (default: 2).
//! * `console_db_size`: the database volume size (default: 10Gi, Hetzner's
//!   smallest). It can grow later, never shrink.
//! * `console_storage_class`: (default: `hcloud-volumes`).
//! * `console_dns_target`, `console_dns_zone`: as `forgejo_dns_target` and
//!   `forgejo_dns_zone` in [`forgejo`](super::forgejo).
//!
//! Billing is on once `STRIPE_SECRET_KEY`, `STRIPE_PRICE_ID` and
//! `STRIPE_WEBHOOK_SECRET` are in the environment of a run: the role keeps
//! them in console/console-stripe, and restarts the console when they change.
//! Runs without them leave the secret as it is. Stripe's webhook endpoint is
//! `https://<console_host>/stripe/webhook`.
//!
//! The database password is made once, at random, and kept in
//! console/console-db: the secret is the source of truth.

use crate::configure::ctx::Ctx;
use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::vars::Vars;
use crate::kube::cluster::{CONTROL_PLANE, Cluster};
use crate::kube::roles::forgejo::{bytes, check_resolves, dns, volume_workers};
use crate::kube::roles::hcloud_csi::{DRIVER, GENERIC_CLASS, PROVIDER, PROVIDER_KEY};
use crate::kube::roles::registry::{AUTH_SECRET, NAMESPACE as REGISTRY_NAMESPACE};
use crate::kube::{apply, get_json, kubectl, unset, var};
use crate::password::{self, Charset};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{Duration, Instant};

const NAMESPACE: &str = "console";
const NAME: &str = "console";
const DB: &str = "console-db";
const DB_IMAGE: &str = "postgres:17-alpine";
const DB_USER: &str = "console";
const CLAIM: &str = "console-db-data";
const DB_SECRET: &str = "console-db";
const STRIPE_SECRET: &str = "console-stripe";
/// The login to `registry_host` the pods pull the console's image with.
const PULL_SECRET: &str = "registry";
/// The annotation on console-stripe and the pods that says which keys they have.
const FINGERPRINT: &str = "usecode.dev/stripe-fingerprint";
const TLS_SECRET: &str = "console-tls";
const ISSUER: &str = "letsencrypt";
const PORT: u16 = 8080;
const DB_PORT: u16 = 5432;
/// The environment variables that switch billing on, as the console reads them.
const STRIPE_ENV: [&str; 3] = [
    "STRIPE_SECRET_KEY",
    "STRIPE_PRICE_ID",
    "STRIPE_WEBHOOK_SECRET",
];

/// What the user decided, from the variables.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub host: Option<String>,
    pub image: String,
    /// `registry_host`, the cluster's own registry, when there is one.
    pub registry: Option<String>,
    pub replicas: u32,
    pub size: String,
    pub class: String,
    /// The CNAME target for `host`, when the role manages its record.
    pub dns_target: Option<String>,
    pub dns_zone: String,
    /// The Stripe settings from this run's environment, when all are there.
    pub stripe: Option<Vec<(String, String)>>,
}

impl Settings {
    pub fn load(vars: &Vars) -> Result<Settings> {
        let name = |v: Option<String>| {
            v.map(|n| n.trim().trim_end_matches('.').to_lowercase())
                .filter(|n| !n.is_empty())
        };
        let host = name(var(vars, "console_host")?);
        let dns_target = name(var(vars, "console_dns_target")?);
        if dns_target.is_some() && dns_target == host {
            bail!("console_dns_target can't be console_host itself");
        }
        let dns_zone = name(var(vars, "console_dns_zone")?).unwrap_or_else(|| {
            let host = host.as_deref().unwrap_or_default();
            // usecode.dev is a zone of its own; www.usecode.dev is in it.
            match host.matches('.').count() {
                0 | 1 => host.to_string(),
                _ => host
                    .split_once('.')
                    .map_or(host, |(_, zone)| zone)
                    .to_string(),
            }
        });
        let replicas = var(vars, "console_replicas")?.unwrap_or(2);
        if replicas == 0 {
            bail!("console_replicas has to be at least 1");
        }
        let registry = name(var(vars, "registry_host")?);
        let image = var(vars, "console_image")?.unwrap_or_else(|| match &registry {
            Some(registry) => format!("{registry}/usecode-console:latest"),
            None => "ghcr.io/evgnomon/usecode-console:latest".into(),
        });
        Ok(Settings {
            host,
            image,
            registry,
            replicas,
            size: var(vars, "console_db_size")?.unwrap_or_else(|| "10Gi".into()),
            class: var(vars, "console_storage_class")?.unwrap_or_else(|| GENERIC_CLASS.into()),
            dns_target,
            dns_zone,
            stripe: stripe_env(|k| std::env::var(k).ok()),
        })
    }

    /// The cluster's registry, when the image comes from it.
    pub fn pull_registry(&self) -> Option<&str> {
        self.registry
            .as_deref()
            .filter(|r| self.image.starts_with(&format!("{r}/")))
    }

    /// The fingerprint of the Stripe settings, when this run has them.
    pub fn fingerprint(&self) -> Option<String> {
        let stripe = self.stripe.as_ref()?;
        let mut hash = Sha256::new();
        for (k, v) in stripe {
            hash.update(k.as_bytes());
            hash.update([0]);
            hash.update(v.as_bytes());
            hash.update([0]);
        }
        Some(
            hash.finalize()
                .iter()
                .take(16)
                .map(|b| format!("{b:02x}"))
                .collect(),
        )
    }

    /// Everything the console is, as one kubectl `List`, with the database
    /// claim at `size`. `fingerprint` is that of the Stripe keys in
    /// console/console-stripe, so a change to them restarts the console.
    pub fn manifest(&self, size: &str, fingerprint: Option<&str>) -> Value {
        let host = self.host.as_deref().unwrap_or_default();
        let meta = |name: &str| json!({"name": name, "namespace": NAMESPACE});
        let db_password = json!({"secretKeyRef": {"name": DB_SECRET, "key": "password"}});
        let workers = |hetzner: bool| {
            let mut exprs = vec![json!({"key": CONTROL_PLANE, "operator": "DoesNotExist"})];
            if hetzner {
                // Hetzner Cloud ones, which volumes reach.
                exprs.push(json!({"key": PROVIDER_KEY, "operator": "In", "values": [PROVIDER]}));
            }
            json!({"nodeAffinity": {"requiredDuringSchedulingIgnoredDuringExecution": {
                "nodeSelectorTerms": [{"matchExpressions": exprs}]
            }}})
        };

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
        let db = json!({
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "metadata": {"name": DB, "namespace": NAMESPACE, "labels": {"app": DB}},
            "spec": {
                "replicas": 1,
                // A Hetzner Volume is attached to one node at a time.
                "strategy": {"type": "Recreate"},
                "selector": {"matchLabels": {"app": DB}},
                "template": {
                    "metadata": {"labels": {"app": DB}},
                    "spec": {
                        "affinity": workers(true),
                        "containers": [{
                            "name": "postgres",
                            "image": DB_IMAGE,
                            "ports": [{"containerPort": DB_PORT, "name": "postgres"}],
                            "env": [
                                {"name": "POSTGRES_USER", "value": DB_USER},
                                {"name": "POSTGRES_DB", "value": DB_USER},
                                {"name": "POSTGRES_PASSWORD", "valueFrom": db_password},
                                // The volume's root has lost+found in it.
                                {"name": "PGDATA", "value": "/var/lib/postgresql/data/pgdata"},
                            ],
                            "volumeMounts": [{"name": "data", "mountPath": "/var/lib/postgresql/data"}],
                            "readinessProbe": {"exec": {"command": ["pg_isready", "-U", DB_USER, "-d", DB_USER]}},
                            "resources": {
                                "requests": {"cpu": "100m", "memory": "256Mi"},
                                "limits": {"memory": "1Gi"},
                            },
                        }],
                        "volumes": [{"name": "data", "persistentVolumeClaim": {"claimName": CLAIM}}],
                    },
                },
            },
        });
        let db_service = json!({
            "apiVersion": "v1",
            "kind": "Service",
            "metadata": meta(DB),
            "spec": {
                "selector": {"app": DB},
                "ports": [{"name": "postgres", "port": DB_PORT, "targetPort": DB_PORT}],
            },
        });

        let stripe_env = STRIPE_ENV.map(|key| {
            json!({"name": key, "valueFrom": {"secretKeyRef": {
                "name": STRIPE_SECRET, "key": key, "optional": true,
            }}})
        });
        let mut env = vec![
            json!({"name": "PUBLIC_URL", "value": format!("https://{host}")}),
            json!({"name": "PGHOST", "value": DB}),
            json!({"name": "PGPORT", "value": DB_PORT.to_string()}),
            json!({"name": "PGUSER", "value": DB_USER}),
            json!({"name": "PGDATABASE", "value": DB_USER}),
            json!({"name": "PGPASSWORD", "valueFrom": db_password}),
        ];
        env.extend(stripe_env);
        let mut console = json!({
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "metadata": {"name": NAME, "namespace": NAMESPACE, "labels": {"app": NAME}},
            "spec": {
                "replicas": self.replicas,
                "selector": {"matchLabels": {"app": NAME}},
                "template": {
                    "metadata": {
                        "labels": {"app": NAME},
                        "annotations": {FINGERPRINT: fingerprint.unwrap_or("none")},
                    },
                    "spec": {
                        "affinity": workers(false),
                        // One per node where there are enough of them.
                        "topologySpreadConstraints": [{
                            "maxSkew": 1,
                            "topologyKey": "kubernetes.io/hostname",
                            "whenUnsatisfiable": "ScheduleAnyway",
                            "labelSelector": {"matchLabels": {"app": NAME}},
                        }],
                        "containers": [{
                            "name": NAME,
                            "image": self.image,
                            "imagePullPolicy": if self.image.ends_with(":latest") { "Always" } else { "IfNotPresent" },
                            "ports": [{"containerPort": PORT, "name": "http"}],
                            "env": env,
                            "readinessProbe": {"httpGet": {"path": "/healthz", "port": PORT}},
                            "livenessProbe": {
                                "httpGet": {"path": "/healthz", "port": PORT},
                                "initialDelaySeconds": 30,
                                "periodSeconds": 20,
                                "failureThreshold": 6,
                            },
                            // The image's `node` user, by number: runAsNonRoot can't
                            // check a user name.
                            "securityContext": {
                                "runAsNonRoot": true,
                                "runAsUser": 1000,
                                "runAsGroup": 1000,
                                "allowPrivilegeEscalation": false,
                                "readOnlyRootFilesystem": true,
                            },
                            "resources": {
                                "requests": {"cpu": "50m", "memory": "96Mi"},
                                "limits": {"memory": "384Mi"},
                            },
                        }],
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
                "ports": [{"name": "http", "port": PORT, "targetPort": PORT}],
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
        if self.pull_registry().is_some() {
            console["spec"]["template"]["spec"]["imagePullSecrets"] =
                json!([{"name": PULL_SECRET}]);
        }
        json!({
            "apiVersion": "v1",
            "kind": "List",
            "items": [claim, db, db_service, console, service, redirect, ingress],
        })
    }
}

/// The Stripe settings, when `get` has all of them.
fn stripe_env(get: impl Fn(&str) -> Option<String>) -> Option<Vec<(String, String)>> {
    STRIPE_ENV
        .iter()
        .map(|k| {
            get(k)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .map(|v| (k.to_string(), v))
        })
        .collect()
}

pub fn tasks(plan: &mut Plan, cluster: &Arc<Cluster>, vars: &Vars) -> Result<()> {
    let settings = Arc::new(Settings::load(vars)?);
    let start = plan.tasks().len();

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "console/report",
            "Plan the console: its name, image, database volume and billing",
        )
        .tags(&["console"])
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
        Task::new("console/namespace", "Create the console namespace")
            .tags(&["console"])
            .after(["console/report"])
            .run(move |ctx| async move { apply(&ctx, &c, &namespace).await }),
    );

    let c = cluster.clone();
    plan.add(
        Task::new(
            "console/db-secret",
            "Keep the database password in console/console-db",
        )
        .tags(&["console"])
        .after(["console/namespace"])
        .run(move |ctx| async move { db_secret(&ctx, &c).await }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "console/stripe",
            "Keep the Stripe keys in console/console-stripe",
        )
        .tags(&["console"])
        .after(["console/namespace"])
        .when(
            settings.stripe.is_some(),
            "STRIPE_SECRET_KEY, STRIPE_PRICE_ID and STRIPE_WEBHOOK_SECRET are not all set; \
             billing stays as it is",
        )
        .run(move |ctx| async move { stripe(&ctx, &c, &s).await }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "console/pull-secret",
            "Keep the login to the cluster's registry in console/registry",
        )
        .tags(&["console"])
        .after(["console/namespace"])
        .when(
            settings.pull_registry().is_some(),
            "the image is not on registry_host; it is pulled without a login",
        )
        .run(move |ctx| async move { pull_secret(&ctx, &c, &s).await }),
    );

    let s = settings.clone();
    plan.add(
        Task::new("console/dns", "Point the name at the workers on Cloudflare")
            .tags(&["console", "dns"])
            .after(["console/report"])
            .when(
                settings.dns_target.is_some(),
                "console_dns_target is not set; the name is yours to point at the workers",
            )
            .run(move |ctx| async move {
                let host = s.host.as_deref().unwrap_or_default();
                let target = s.dns_target.as_deref().unwrap_or_default();
                dns(&ctx, host, target, &s.dns_zone).await
            }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "console/install",
            "Run the console and its Postgres on a Hetzner Volume",
        )
        .tags(&["console"])
        .after([
            "console/db-secret",
            "console/stripe",
            "console/pull-secret",
            // cert-manager asks Let's Encrypt as soon as the Ingress is
            // there, so the name has to resolve by then.
            "console/dns",
            // Its volume needs the driver and the storage class.
            "hcloud_csi/storage-classes",
        ])
        .run(move |ctx| async move { install(&ctx, &c, &s).await }),
    );

    let (c, s) = (cluster.clone(), settings.clone());
    plan.add(
        Task::new(
            "console/verify",
            "Check the console and its database run and it has its certificate",
        )
        .tags(&["console"])
        .after(["console/install"])
        .run(move |ctx| async move { verify(&ctx, &c, &s).await }),
    );

    plan.gate(
        start,
        settings.host.is_some(),
        &unset(cluster, "console_host", "usecode.dev"),
    );
    Ok(())
}

async fn report(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    let host = s.host.clone().unwrap_or_default();
    ctx.note(&format!(
        "https://{host}: {} x{}, Postgres on {} of {} (claim {NAMESPACE}/{CLAIM})",
        s.image, s.replicas, s.size, s.class
    ));
    ctx.note(if s.stripe.is_some() {
        "billing: Stripe keys from this run's environment"
    } else {
        "billing: no Stripe keys in this run's environment; console-stripe stays as it is"
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
            "the console's database runs on Hetzner Cloud workers only, and there is none{place}: \
             add a worker there, or pick a class in a location that has one \
             (console_storage_class)"
        )),
        1 => ctx.note(&format!(
            "warning: {} is the only worker{place}: if it goes down, sign ups and logins wait \
             for it, since the database's volume never leaves that location",
            usable[0]
        )),
        _ => ctx.note(&format!(
            "the database runs on the workers{place}: {}",
            usable.join(", ")
        )),
    }

    // console/dns sees to the name when it manages the record.
    if s.dns_target.is_none() {
        check_resolves(ctx, cluster, &host).await?;
    }

    if problems.is_empty() {
        return Ok(Outcome::Ok);
    }
    bail!("fix these first:\n- {}", problems.join("\n- "))
}

async fn db_secret(ctx: &Ctx, cluster: &Cluster) -> Result<Outcome> {
    let found = kubectl(ctx, cluster)
        .args(["-n", NAMESPACE, "get", "secret", DB_SECRET, "-o", "name"])
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
        "metadata": {"name": DB_SECRET, "namespace": NAMESPACE},
        "stringData": {"password": password},
    });
    kubectl(ctx, cluster)
        .args(["create", "-f", "-"])
        .stdin(secret.to_string())
        .output()
        .await?;
    Ok(Outcome::Changed)
}

/// Copies the registry's login into console/registry, as the
/// `.dockerconfigjson` the pods pull with. The secrets are read straight
/// from kubectl so that what they hold is never logged.
async fn pull_secret(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    let Some(registry) = s.pull_registry() else {
        return Ok(Outcome::Ok);
    };
    let field = |key: &'static str| secret_field(cluster, REGISTRY_NAMESPACE, AUTH_SECRET, key);
    let (Some(user), Some(password)) = (field("username").await, field("password").await) else {
        bail!(
            "{REGISTRY_NAMESPACE}/{AUTH_SECRET} has no login yet: run `uc kube sync -t registry` \
             first, or set console_image to an image the cluster can pull without one"
        );
    };
    use base64::Engine;
    let auth = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"));
    let config =
        json!({"auths": {registry: {"username": user, "password": password, "auth": auth}}})
            .to_string();
    let have = secret_field(cluster, NAMESPACE, PULL_SECRET, ".dockerconfigjson").await;
    if have.as_deref() == Some(config.as_str()) {
        return Ok(Outcome::Ok);
    }
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    let secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "type": "kubernetes.io/dockerconfigjson",
        "metadata": {"name": PULL_SECRET, "namespace": NAMESPACE},
        "stringData": {".dockerconfigjson": config},
    });
    // `create` or `replace`, never `apply`, which would keep a copy of the
    // password in the last-applied annotation.
    let verb = if have.is_some() { "replace" } else { "create" };
    kubectl(ctx, cluster)
        .args([verb, "-f", "-"])
        .stdin(secret.to_string())
        .output()
        .await?;
    Ok(Outcome::Changed)
}

/// One field of a secret, decoded, if the secret has it. kubectl runs
/// directly, not through the task's logged runner.
async fn secret_field(cluster: &Cluster, namespace: &str, name: &str, key: &str) -> Option<String> {
    use base64::Engine;
    let key = key.replace('.', "\\.");
    let out = tokio::process::Command::new("kubectl")
        .args([
            "--context",
            &cluster.context,
            "-n",
            namespace,
            "get",
            "secret",
            name,
        ])
        .arg(format!("-o=jsonpath={{.data.{key}}}"))
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .ok()
        .filter(|o| o.status.success())?;
    let value = base64::engine::general_purpose::STANDARD
        .decode(out.stdout.trim_ascii())
        .ok()?;
    String::from_utf8(value).ok().filter(|v| !v.is_empty())
}

/// The fingerprint console/console-stripe was written with, if it exists.
/// Only the annotation is read: what kubectl prints is logged.
async fn stripe_fingerprint(ctx: &Ctx, cluster: &Cluster) -> Result<Option<String>> {
    let out = kubectl(ctx, cluster)
        .args([
            "-n",
            NAMESPACE,
            "get",
            "secret",
            STRIPE_SECRET,
            "--ignore-not-found",
        ])
        .arg(format!(
            "-o=jsonpath={{.metadata.annotations.{}}}",
            FINGERPRINT.replace('.', "\\.")
        ))
        .read_only()
        .output()
        .await?;
    let fp = out.stdout.trim();
    Ok((!fp.is_empty()).then(|| fp.to_string()))
}

async fn stripe(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    let (Some(keys), Some(want)) = (&s.stripe, s.fingerprint()) else {
        return Ok(Outcome::Ok);
    };
    let have = stripe_fingerprint(ctx, cluster).await?;
    if have.as_deref() == Some(want.as_str()) {
        return Ok(Outcome::Ok);
    }
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    let data: serde_json::Map<String, Value> = keys
        .iter()
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .collect();
    let secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": STRIPE_SECRET,
            "namespace": NAMESPACE,
            "annotations": {FINGERPRINT: want},
        },
        "stringData": data,
    });
    // `create` or `replace`, never `apply`, which would keep a copy of the
    // keys in the last-applied annotation.
    let verb = if have.is_some() { "replace" } else { "create" };
    kubectl(ctx, cluster)
        .args([verb, "-f", "-"])
        .stdin(secret.to_string())
        .output()
        .await?;
    Ok(Outcome::Changed)
}

async fn install(ctx: &Ctx, cluster: &Cluster, s: &Settings) -> Result<Outcome> {
    // A claim can't shrink: one bigger than console_db_size keeps its size.
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
            "{CLAIM} keeps its {current}: a volume grows, never shrinks, so console_db_size={} \
             only applies to a new one",
            s.size
        ));
        current
    } else {
        &s.size
    };
    // This run's keys, else the ones the cluster already has.
    let fingerprint = match s.fingerprint() {
        Some(fp) => Some(fp),
        None => stripe_fingerprint(ctx, cluster).await?,
    };
    let outcome = apply(
        ctx,
        cluster,
        &s.manifest(size, fingerprint.as_deref()).to_string(),
    )
    .await?;
    if ctx.check() || outcome == Outcome::Ok {
        return Ok(outcome);
    }
    // The first start attaches a new volume, and the console waits for the
    // database before it answers.
    for deployment in [DB, NAME] {
        if let Err(err) = wait_ready(ctx, cluster, deployment).await {
            bail!("{err:#}\n\n{}", logs(ctx, cluster).await);
        }
    }
    Ok(outcome)
}

/// Waits up to ten minutes for a deployment to roll out, and gives up at
/// once when its image can't be pulled, which waiting doesn't fix.
async fn wait_ready(ctx: &Ctx, cluster: &Cluster, deployment: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        let rollout = kubectl(ctx, cluster)
            .args(["-n", NAMESPACE, "rollout", "status"])
            .arg(format!("deployment/{deployment}"))
            .arg("--timeout=10s")
            .any_code()
            .read_only()
            .output()
            .await?;
        if rollout.success() {
            return Ok(());
        }
        let selector = format!("app={deployment}");
        let pods = get_json(
            ctx,
            cluster,
            &["-n", NAMESPACE, "get", "pods", "-l", &selector],
        )
        .await?;
        let deploy = get_json(
            ctx,
            cluster,
            &["-n", NAMESPACE, "get", "deployment", deployment],
        )
        .await?;
        if let Some(why) = pull_failure(&pods, &images(&deploy)) {
            bail!("deployment/{deployment} can't pull its image: {why}");
        }
        if Instant::now() > deadline {
            bail!(
                "deployment/{deployment} is not ready after 10 minutes: {}",
                rollout.combined().trim()
            );
        }
    }
}

/// The images a deployment runs now.
fn images(deploy: &Value) -> Vec<&str> {
    deploy["spec"]["template"]["spec"]["containers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["image"].as_str())
        .collect()
}

/// The message of the first pod container stuck pulling one of `images`.
/// Pods of an older rollout that can't pull an image no longer used don't
/// count: they go once the new ones are ready.
fn pull_failure(pods: &Value, images: &[&str]) -> Option<String> {
    pods["items"]
        .as_array()?
        .iter()
        .flat_map(|pod| {
            pod["status"]["containerStatuses"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter(|status| {
            status["image"]
                .as_str()
                .is_some_and(|image| images.contains(&image))
        })
        .map(|status| &status["state"]["waiting"])
        .find(|waiting| {
            matches!(
                waiting["reason"].as_str(),
                Some("ImagePullBackOff" | "ErrImagePull" | "InvalidImageName")
            )
        })
        .map(|waiting| {
            format!(
                "{}: {}",
                waiting["reason"].as_str().unwrap_or_default(),
                waiting["message"].as_str().unwrap_or_default()
            )
        })
}

/// The tail of the console's and the database's logs and the claim's
/// events, for a failure message.
async fn logs(ctx: &Ctx, cluster: &Cluster) -> String {
    let (console, db) = (format!("deployment/{NAME}"), format!("deployment/{DB}"));
    let mut text = String::new();
    for args in [
        vec!["logs", console.as_str(), "--all-containers", "--tail=30"],
        vec!["logs", db.as_str(), "--tail=30"],
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
    for deployment in [DB, NAME] {
        let rollout = kubectl(ctx, cluster)
            .args(["-n", NAMESPACE, "rollout", "status"])
            .arg(format!("deployment/{deployment}"))
            .arg("--timeout=300s")
            .any_code()
            .read_only()
            .output()
            .await?;
        if !rollout.success() {
            problems.push(format!(
                "deployment/{deployment} is not ready: {}",
                rollout.combined().trim()
            ));
        }
    }

    let claim = get_json(ctx, cluster, &["-n", NAMESPACE, "get", "pvc", CLAIM]).await?;
    let class = claim["spec"]["storageClassName"]
        .as_str()
        .unwrap_or_default();
    if class != s.class {
        problems.push(format!(
            "the claim {CLAIM} is in class {class}, not {}; a claim's class can't change, \
             so move the data to a new claim or set console_storage_class={class}",
            s.class
        ));
    }
    if claim["status"]["phase"] != "Bound" {
        problems.push(format!(
            "the claim {CLAIM} is {}, not Bound",
            claim["status"]["phase"].as_str().unwrap_or("unknown")
        ));
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
    if stripe_fingerprint(ctx, cluster).await?.is_none() {
        ctx.note(&format!(
            "billing is off: run again with STRIPE_SECRET_KEY, STRIPE_PRICE_ID and \
             STRIPE_WEBHOOK_SECRET set; Stripe's webhook goes to https://{host}/stripe/webhook"
        ));
    }
    Ok(Outcome::Ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            host: Some("usecode.dev".into()),
            image: "ghcr.io/evgnomon/usecode-console:abc123".into(),
            registry: None,
            replicas: 2,
            size: "10Gi".into(),
            class: GENERIC_CLASS.into(),
            dns_target: None,
            dns_zone: "usecode.dev".into(),
            stripe: None,
        }
    }

    fn item<'a>(list: &'a Value, kind: &str, name: &str) -> &'a Value {
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["kind"] == kind && i["metadata"]["name"] == name)
            .unwrap()
    }

    #[test]
    fn keeps_the_database_on_a_hetzner_volume() {
        let s = settings();
        let m = s.manifest(&s.size, None);
        let claim = item(&m, "PersistentVolumeClaim", CLAIM);
        assert_eq!(claim["spec"]["storageClassName"], "hcloud-volumes");
        assert_eq!(claim["spec"]["resources"]["requests"]["storage"], "10Gi");
        let db = item(&m, "Deployment", DB);
        assert_eq!(db["spec"]["strategy"]["type"], "Recreate");
        let exprs = &db["spec"]["template"]["spec"]["affinity"]["nodeAffinity"]["requiredDuringSchedulingIgnoredDuringExecution"]
            ["nodeSelectorTerms"][0]["matchExpressions"];
        assert!(exprs.as_array().unwrap().contains(&json!(
            {"key": PROVIDER_KEY, "operator": "In", "values": [PROVIDER]}
        )));
    }

    #[test]
    fn the_console_finds_its_database_and_its_keys() {
        let m = settings().manifest("10Gi", Some("abc"));
        let console = item(&m, "Deployment", NAME);
        assert_eq!(console["spec"]["replicas"], 2);
        let pod = &console["spec"]["template"];
        assert_eq!(pod["metadata"]["annotations"][FINGERPRINT], "abc");
        let container = &pod["spec"]["containers"][0];
        assert_eq!(container["imagePullPolicy"], "IfNotPresent");
        let env = container["env"].as_array().unwrap();
        let get = |name: &str| env.iter().find(|e| e["name"] == name).unwrap();
        assert_eq!(get("PUBLIC_URL")["value"], "https://usecode.dev");
        assert_eq!(get("PGHOST")["value"], DB);
        assert_eq!(
            get("PGPASSWORD")["valueFrom"]["secretKeyRef"]["name"],
            DB_SECRET
        );
        assert_eq!(
            get("STRIPE_SECRET_KEY")["valueFrom"]["secretKeyRef"]["optional"],
            true
        );
    }

    #[test]
    fn latest_is_pulled_again() {
        let s = Settings {
            image: "ghcr.io/evgnomon/usecode-console:latest".into(),
            ..settings()
        };
        let m = s.manifest("10Gi", None);
        let c = &item(&m, "Deployment", NAME)["spec"]["template"]["spec"]["containers"][0];
        assert_eq!(c["imagePullPolicy"], "Always");
    }

    #[test]
    fn pulls_from_the_cluster_registry_with_its_login() {
        let pull_secrets = |s: &Settings| {
            let m = s.manifest("10Gi", None);
            item(&m, "Deployment", NAME)["spec"]["template"]["spec"]["imagePullSecrets"].clone()
        };
        let ours = Settings {
            image: "registry.example.com/usecode-console:latest".into(),
            registry: Some("registry.example.com".into()),
            ..settings()
        };
        assert_eq!(ours.pull_registry(), Some("registry.example.com"));
        assert_eq!(pull_secrets(&ours), json!([{"name": PULL_SECRET}]));
        let ghcr = Settings {
            registry: Some("registry.example.com".into()),
            ..settings()
        };
        assert_eq!(ghcr.pull_registry(), None);
        assert!(pull_secrets(&ghcr).is_null());
    }

    #[test]
    fn serves_https_on_the_host() {
        let m = settings().manifest("10Gi", None);
        let ingress = item(&m, "Ingress", NAME);
        assert_eq!(ingress["spec"]["rules"][0]["host"], "usecode.dev");
        assert_eq!(ingress["spec"]["tls"][0]["secretName"], TLS_SECRET);
    }

    #[test]
    fn billing_needs_all_three_keys() {
        let all = |k: &str| Some(format!("{k}-value"));
        assert_eq!(stripe_env(all).unwrap().len(), 3);
        let some = |k: &str| (k != "STRIPE_PRICE_ID").then(|| "x".to_string());
        assert_eq!(stripe_env(some), None);
        let blank = |_: &str| Some("  ".to_string());
        assert_eq!(stripe_env(blank), None);
    }

    #[test]
    fn the_fingerprint_follows_the_keys() {
        let with = |key: &str| Settings {
            stripe: stripe_env(|k| Some(format!("{k}={key}"))),
            ..settings()
        };
        assert_eq!(settings().fingerprint(), None);
        assert_eq!(with("a").fingerprint(), with("a").fingerprint());
        assert_ne!(with("a").fingerprint(), with("b").fingerprint());
        assert_eq!(with("a").fingerprint().unwrap().len(), 32);
    }
}
