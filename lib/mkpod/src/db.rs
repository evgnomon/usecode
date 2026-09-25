// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Access to the `public.resources` table.

use std::collections::HashSet;

use anyhow::{Result, anyhow};
use postgres::{Client, NoTls};
use serde_json::{Value, json};

use crate::spec::{self, Pod, PodRow};

/// Human-readable error text, preferring the server's message.
pub fn err_text(e: &postgres::Error) -> String {
    match e.as_db_error() {
        Some(db) => format!("{}: {}", db.severity(), db.message()),
        None => match std::error::Error::source(e) {
            Some(src) => format!("{e}: {src}"),
            None => e.to_string(),
        },
    }
}

fn wrap(e: postgres::Error) -> anyhow::Error {
    anyhow!(err_text(&e))
}

pub struct Conn {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

pub fn connect(c: &Conn) -> Result<Client> {
    postgres::Config::new()
        .host(&c.host)
        .port(c.port)
        .user(&c.user)
        .password(&c.password)
        .dbname(&c.database)
        .connect(NoTls)
        .map_err(|e| anyhow!("Error connecting to database: {}", err_text(&e)))
}

/// All `Pod` resources in namespace/tenant, ordered by name.
pub fn get_pods(client: &mut Client, namespace: &str, tenant: &str) -> Result<Vec<Pod>> {
    let rows = client
        .query(
            "SELECT id::text, api_version, kind, name, namespace, tenant,
                    labels, annotations, spec
             FROM public.resources
             WHERE kind = 'Pod'
                 AND namespace = $1
                 AND tenant = $2
             ORDER BY name",
            &[&namespace, &tenant],
        )
        .map_err(|e| anyhow!("Error querying database: {}", err_text(&e)))?;
    rows.iter()
        .map(|r| {
            Ok(spec::pod_from_row(PodRow {
                id: r.try_get::<_, Option<String>>(0)?.unwrap_or_default(),
                api_version: r.try_get(1)?,
                kind: r.try_get(2)?,
                name: r.try_get(3)?,
                namespace: r.try_get(4)?,
                labels: r.try_get(6)?,
                annotations: r.try_get(7)?,
                spec: r.try_get(8)?,
            }))
        })
        .collect::<Result<_, postgres::Error>>()
        .map_err(|e| anyhow!("Error querying database: {}", err_text(&e)))
}

/// `spec` of the named resource, `None` if no such row exists.
fn find_spec(
    client: &mut Client,
    kind: &str,
    name: &str,
    namespace: &str,
    tenant: &str,
) -> Result<Option<Value>> {
    let row = client
        .query_opt(
            "SELECT spec FROM public.resources
             WHERE kind = $1 AND name = $2 AND namespace = $3 AND tenant = $4",
            &[&kind, &name, &namespace, &tenant],
        )
        .map_err(wrap)?;
    row.map(|r| r.try_get::<_, Option<Value>>(0))
        .transpose()
        .map_err(wrap)
        .map(|s| s.map(|s| s.filter(spec::truthy).unwrap_or_else(|| json!({}))))
}

fn insert(
    client: &mut Client,
    kind: &str,
    name: &str,
    namespace: &str,
    tenant: &str,
    spec: &Value,
) -> Result<()> {
    client
        .execute(
            "INSERT INTO public.resources
                 (api_version, kind, name, namespace, tenant, spec)
             VALUES ($1, $2, $3, $4, $5, $6)",
            &[&"v1", &kind, &name, &namespace, &tenant, spec],
        )
        .map_err(wrap)?;
    Ok(())
}

/// Ensure all referenced PVCs exist, auto-creating missing ones.
pub fn ensure_pvcs(
    client: &mut Client,
    pod: &Value,
    namespace: &str,
    tenant: &str,
) -> Result<Vec<Value>> {
    let kind = "PersistentVolumeClaim";
    let mut pvcs = Vec::new();
    for name in spec::referenced_pvcs(pod) {
        let spec = match find_spec(client, kind, &name, namespace, tenant)? {
            Some(s) => s,
            None => {
                let s = spec::default_pvc_spec();
                insert(client, kind, &name, namespace, tenant, &s)?;
                println!("Auto-created PersistentVolumeClaim '{name}' with default 1Gi storage");
                s
            }
        };
        pvcs.push(spec::pvc_doc(&name, spec));
    }
    Ok(pvcs)
}

/// Ensure all referenced ConfigMaps exist, auto-creating missing ones.
pub fn ensure_configmaps(
    client: &mut Client,
    pod: &Value,
    namespace: &str,
    tenant: &str,
) -> Result<Vec<Value>> {
    let kind = "ConfigMap";
    let mut cms = Vec::new();
    for name in spec::referenced_configmaps(pod) {
        let data = match find_spec(client, kind, &name, namespace, tenant)? {
            Some(d) => d,
            None => {
                insert(client, kind, &name, namespace, tenant, &json!({}))?;
                println!("Auto-created empty ConfigMap '{name}'");
                println!("  Populate it with: echo 'value' | confmap {name} key");
                json!({})
            }
        };
        cms.push(spec::configmap_doc(&name, data));
    }
    Ok(cms)
}

/// Ids of all current `Pod` resources in namespace/tenant.
pub fn active_pod_ids(
    client: &mut Client,
    namespace: &str,
    tenant: &str,
) -> Result<HashSet<String>> {
    let rows = client
        .query(
            "SELECT id::text FROM public.resources WHERE kind = 'Pod' AND namespace = $1 AND tenant = $2",
            &[&namespace, &tenant],
        )
        .map_err(wrap)?;
    Ok(rows
        .iter()
        .filter_map(|r| r.get::<_, Option<String>>(0))
        .collect())
}

/// (name, spec) of all `Pod` resources, ordered by name.
pub fn list_pods(
    client: &mut Client,
    namespace: &str,
    tenant: &str,
) -> Result<Vec<(Option<String>, Option<Value>)>> {
    let rows = client
        .query(
            "SELECT name, spec FROM public.resources
             WHERE kind = 'Pod' AND namespace = $1 AND tenant = $2
             ORDER BY name",
            &[&namespace, &tenant],
        )
        .map_err(|e| anyhow!("Error querying database: {}", err_text(&e)))?;
    rows.iter()
        .map(|r| Ok((r.try_get(0)?, r.try_get(1)?)))
        .collect::<Result<_, postgres::Error>>()
        .map_err(|e| anyhow!("Error querying database: {}", err_text(&e)))
}
