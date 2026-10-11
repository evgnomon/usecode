// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! PowerDNS's HTTP API, for one instance, through a `kubectl
//! port-forward` to its pod: the API only answers on the pod's loopback.

use crate::kube::cloudflare::{client, send};
use crate::kube::remote::{Forward, port_forward};
use crate::kube::roles::dns::{API_PORT, NAMESPACE};
use anyhow::{Result, anyhow, bail};
use reqwest::Method;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const BASE: &str = "/api/v1/servers/localhost";

/// An RRset: every record of one name and type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RRset {
    pub ttl: u32,
    /// The contents, sorted.
    pub records: Vec<String>,
}

/// A zone's RRsets by (name, type), without the SOA, which each instance
/// keeps itself.
pub type Records = BTreeMap<(String, String), RRset>;

pub struct Pdns {
    /// `pdns-<n>`.
    pub name: String,
    url: String,
    key: String,
    _forward: Forward,
}

impl Pdns {
    pub async fn connect(context: &str, name: &str, key: &str) -> Result<Pdns> {
        let forward =
            port_forward(context, NAMESPACE, &format!("deployment/{name}"), API_PORT).await?;
        Ok(Pdns {
            name: name.to_string(),
            url: format!("{}{BASE}", forward.url),
            key: key.to_string(),
            _forward: forward,
        })
    }

    async fn call(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        let what = format!("{} {method} {path}", self.name);
        let url = format!("{}{path}", self.url);
        let reply = send(&what, || {
            let req = client()
                .request(method.clone(), &url)
                .header("X-API-Key", &self.key);
            match body {
                Some(b) => req.json(b),
                None => req,
            }
        })
        .await?;
        let status = reply.status();
        let text = reply.text().await?;
        let json: Value = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        };
        if !status.is_success() {
            let why = json["error"]
                .as_str()
                .map_or_else(|| json.to_string(), str::to_string);
            bail!("{what}: HTTP {status}: {why}");
        }
        Ok(json)
    }

    /// The zones it has, as `example.com.`.
    pub async fn zones(&self) -> Result<Vec<String>> {
        let list = self.call(Method::GET, "/zones", None).await?;
        let mut zones: Vec<String> = list
            .as_array()
            .ok_or_else(|| anyhow!("{}: no zone list in {list}", self.name))?
            .iter()
            .filter_map(|z| z["name"].as_str().map(str::to_string))
            .collect();
        zones.sort();
        Ok(zones)
    }

    /// The zone's records, or None when it doesn't have the zone.
    pub async fn records(&self, zone: &str) -> Result<Option<Records>> {
        if !self.zones().await?.iter().any(|z| z == zone) {
            return Ok(None);
        }
        let z = self
            .call(Method::GET, &format!("/zones/{zone}"), None)
            .await?;
        Ok(Some(parse_records(&z)))
    }

    /// Makes `zone` with `records` (which hold its NS records).
    pub async fn create_zone(&self, zone: &str, records: &Records) -> Result<()> {
        let rrsets: Vec<Value> = records
            .iter()
            .map(|((name, kind), set)| {
                json!({
                    "name": name,
                    "type": kind,
                    "ttl": set.ttl,
                    "records": set.records.iter().map(|c| json!({"content": c, "disabled": false})).collect::<Vec<_>>(),
                })
            })
            .collect();
        let body = json!({
            "name": zone,
            "kind": "Native",
            "nameservers": [],
            "soa_edit_api": "DEFAULT",
            "rrsets": rrsets,
        });
        self.call(Method::POST, "/zones", Some(&body)).await?;
        Ok(())
    }

    pub async fn delete_zone(&self, zone: &str) -> Result<()> {
        self.call(Method::DELETE, &format!("/zones/{zone}"), None)
            .await?;
        Ok(())
    }

    /// Sets each (name, type) to its RRset, or deletes it for None.
    pub async fn patch(
        &self,
        zone: &str,
        changes: &BTreeMap<(String, String), Option<RRset>>,
    ) -> Result<()> {
        if changes.is_empty() {
            return Ok(());
        }
        let rrsets: Vec<Value> = changes
            .iter()
            .map(|((name, kind), set)| match set {
                Some(set) => json!({
                    "name": name,
                    "type": kind,
                    "ttl": set.ttl,
                    "changetype": "REPLACE",
                    "records": set.records.iter().map(|c| json!({"content": c, "disabled": false})).collect::<Vec<_>>(),
                }),
                None => json!({"name": name, "type": kind, "changetype": "DELETE"}),
            })
            .collect();
        self.call(
            Method::PATCH,
            &format!("/zones/{zone}"),
            Some(&json!({"rrsets": rrsets})),
        )
        .await?;
        Ok(())
    }
}

/// The RRsets of a zone from the API, without the SOA.
pub fn parse_records(zone: &Value) -> Records {
    zone["rrsets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["type"] != "SOA")
        .filter_map(|r| {
            let mut records: Vec<String> = r["records"]
                .as_array()?
                .iter()
                .filter(|c| c["disabled"] != true)
                .filter_map(|c| c["content"].as_str().map(str::to_string))
                .collect();
            records.sort();
            Some((
                (
                    r["name"].as_str()?.to_string(),
                    r["type"].as_str()?.to_string(),
                ),
                RRset {
                    ttl: r["ttl"].as_u64().unwrap_or(3600) as u32,
                    records,
                },
            ))
        })
        .filter(|(_, set)| !set.records.is_empty())
        .collect()
}

/// What makes `to` the same as `from`.
pub fn diff(from: &Records, to: &Records) -> BTreeMap<(String, String), Option<RRset>> {
    let mut changes = BTreeMap::new();
    for (key, set) in from {
        if to.get(key) != Some(set) {
            changes.insert(key.clone(), Some(set.clone()));
        }
    }
    for key in to.keys() {
        if !from.contains_key(key) {
            changes.insert(key.clone(), None);
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(ttl: u32, records: &[&str]) -> RRset {
        RRset {
            ttl,
            records: records.iter().map(|r| r.to_string()).collect(),
        }
    }

    fn key(name: &str, kind: &str) -> (String, String) {
        (name.into(), kind.into())
    }

    #[test]
    fn reads_rrsets_without_the_soa() {
        let z = json!({"rrsets": [
            {"name": "example.com.", "type": "SOA", "ttl": 3600, "records": [{"content": "x", "disabled": false}]},
            {"name": "example.com.", "type": "A", "ttl": 300, "records": [
                {"content": "192.0.2.2", "disabled": false},
                {"content": "192.0.2.1", "disabled": false},
                {"content": "192.0.2.9", "disabled": true},
            ]},
        ]});
        let r = parse_records(&z);
        assert_eq!(r.len(), 1);
        assert_eq!(
            r[&key("example.com.", "A")],
            set(300, &["192.0.2.1", "192.0.2.2"])
        );
    }

    #[test]
    fn diff_replaces_and_deletes() {
        let from = Records::from([
            (key("a.", "A"), set(300, &["192.0.2.1"])),
            (key("b.", "A"), set(300, &["192.0.2.2"])),
        ]);
        let to = Records::from([
            (key("a.", "A"), set(300, &["192.0.2.1"])),
            (key("b.", "A"), set(60, &["192.0.2.2"])),
            (key("c.", "TXT"), set(300, &["\"x\""])),
        ]);
        let d = diff(&from, &to);
        assert_eq!(d.len(), 2);
        assert_eq!(d[&key("b.", "A")], Some(set(300, &["192.0.2.2"])));
        assert_eq!(d[&key("c.", "TXT")], None);
        assert!(diff(&from, &from).is_empty());
    }
}
