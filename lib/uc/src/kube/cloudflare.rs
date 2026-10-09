// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Cloudflare's DNS API and public DNS-over-HTTPS, called directly, so the
//! roles need no `cf`, `curl` or `dig` on the machine.

use anyhow::{Result, anyhow, bail};
use reqwest::Method;
use serde_json::Value;
use std::collections::BTreeSet;
use std::net::IpAddr;
use std::sync::OnceLock;
use std::time::Duration;

const API: &str = "https://api.cloudflare.com/client/v4";
const DOH: &str = "https://cloudflare-dns.com/dns-query";
pub const HINT: &str = "source ~/.bashrc.d/cloudflare.sh; the token needs Zone:Read and DNS:Edit";
/// Tries per request; transport errors, 429 and 5xx are tried again.
const TRIES: u32 = 3;

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        // graviola is pure Rust, so the static musl build needs no C
        // toolchain, as aws-lc or ring would. Err only means a provider is
        // already installed.
        let _ = rustls_graviola::default_provider().install_default();
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("HTTP client")
    })
}

/// Sends a request built by `build`, trying again on errors that may pass.
async fn send(
    what: &str,
    build: impl Fn() -> reqwest::RequestBuilder,
) -> Result<reqwest::Response> {
    let mut last = String::new();
    for attempt in 1..=TRIES {
        if attempt > 1 {
            tokio::time::sleep(Duration::from_secs(2 * u64::from(attempt))).await;
        }
        match build().send().await {
            Ok(r) if r.status().is_server_error() || r.status().as_u16() == 429 => {
                last = format!("HTTP {}", r.status());
            }
            Ok(r) => return Ok(r),
            Err(e) => last = format!("{e:#}"),
        }
    }
    bail!("{what}: {last} after {TRIES} tries")
}

pub struct Cloudflare {
    token: String,
}

impl Cloudflare {
    /// Takes the token from `CLOUDFLARE_API_TOKEN`.
    pub fn from_env() -> Result<Cloudflare> {
        let token = std::env::var("CLOUDFLARE_API_TOKEN")
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .ok_or_else(|| anyhow!("CLOUDFLARE_API_TOKEN is not set\n{HINT}"))?;
        Ok(Cloudflare { token })
    }

    /// Calls the API and returns its `result`.
    async fn call(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<Value> {
        let what = format!("Cloudflare API {method} {path}");
        let reply = send(&what, || {
            let req = client()
                .request(method.clone(), format!("{API}{path}"))
                .bearer_auth(&self.token)
                .query(query);
            match body {
                Some(b) => req.json(b),
                None => req,
            }
        })
        .await?;
        let status = reply.status();
        let text = reply.text().await?;
        let reply: Value = serde_json::from_str(&text)
            .map_err(|e| anyhow!("{what}: HTTP {status}, not JSON ({e}): {}", text.trim()))?;
        if reply["success"] != true {
            let errors: Vec<String> = reply["errors"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| format!("{} {}", e["code"], e["message"].as_str().unwrap_or("?")))
                .collect();
            let errors = if errors.is_empty() {
                format!("HTTP {status}")
            } else {
                errors.join("; ")
            };
            bail!("{what} failed: {errors}\n{HINT}");
        }
        Ok(reply["result"].clone())
    }

    /// The id of `zone`, which must be on the token's account.
    pub async fn zone_id(&self, zone: &str) -> Result<String> {
        let zones = self
            .call(Method::GET, "/zones", &[("name", zone)], None)
            .await?;
        zones[0]["id"].as_str().map(str::to_string).ok_or_else(|| {
            anyhow!(
                "zone {zone} is not on this Cloudflare account, or the token can't read it\n{HINT}"
            )
        })
    }

    /// Every record named `name` in the zone.
    pub async fn records(&self, zone_id: &str, name: &str) -> Result<Vec<Value>> {
        let records = self
            .call(
                Method::GET,
                &format!("/zones/{zone_id}/dns_records"),
                &[("name", name), ("per_page", "100")],
                None,
            )
            .await?;
        Ok(records.as_array().cloned().unwrap_or_default())
    }

    pub async fn delete(&self, zone_id: &str, id: &str) -> Result<()> {
        self.call(
            Method::DELETE,
            &format!("/zones/{zone_id}/dns_records/{id}"),
            &[],
            None,
        )
        .await?;
        Ok(())
    }

    pub async fn create(&self, zone_id: &str, record: &Value) -> Result<()> {
        self.call(
            Method::POST,
            &format!("/zones/{zone_id}/dns_records"),
            &[],
            Some(record),
        )
        .await?;
        Ok(())
    }
}

/// The A addresses public DNS gives for `name`, asked over HTTPS at
/// 1.1.1.1, not the local resolver, which may cache an old answer.
pub async fn resolve(name: &str) -> Result<BTreeSet<IpAddr>> {
    let what = format!("resolving {name} at {DOH}");
    let reply = send(&what, || {
        client()
            .get(DOH)
            .header("accept", "application/dns-json")
            .query(&[("name", name), ("type", "A")])
    })
    .await?
    .error_for_status()
    .map_err(|e| anyhow!("{what}: {e}"))?;
    let reply: Value = reply.json().await.map_err(|e| anyhow!("{what}: {e}"))?;
    Ok(addresses(&reply))
}

/// The A records of a DNS JSON answer; a CNAME chain's names are left out.
fn addresses(reply: &Value) -> BTreeSet<IpAddr> {
    reply["Answer"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a["type"] == 1)
        .filter_map(|a| a["data"].as_str()?.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_only_the_a_records_of_an_answer() {
        let reply = json!({"Status": 0, "Answer": [
            {"name": "registry.example.com", "type": 5, "data": "example.com."},
            {"name": "example.com", "type": 1, "data": "192.0.2.1"},
            {"name": "example.com", "type": 1, "data": "192.0.2.2"},
        ]});
        let want: BTreeSet<IpAddr> = ["192.0.2.1", "192.0.2.2"]
            .iter()
            .map(|a| a.parse().unwrap())
            .collect();
        assert_eq!(addresses(&reply), want);
    }

    #[test]
    fn no_answer_is_no_addresses() {
        assert!(addresses(&json!({"Status": 3})).is_empty());
    }
}
