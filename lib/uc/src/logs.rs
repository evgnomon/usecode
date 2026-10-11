// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc kube logs`: the container logs of the whole cluster, from the
//! VictoriaLogs the `obs` role of `uc kube configure` runs.
//!
//! What you ask for becomes one LogsQL query, see [`Filter`]. VictoriaLogs
//! is reached through a `kubectl port-forward` to its Service, so all it
//! needs is kubectl, or at `UC_LOGS_URL` when that is set.

use crate::kube::remote::{Forward, port_forward};
use crate::kube::roles::obs::{LOGS, LOGS_PORT, NAMESPACE};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;

/// The fields the log collector gives each line.
pub const NAMESPACE_FIELD: &str = "kubernetes.pod_namespace";
pub const POD_FIELD: &str = "kubernetes.pod_name";
pub const CONTAINER_FIELD: &str = "kubernetes.container_name";
pub const NODE_FIELD: &str = "kubernetes.pod_node_name";
const LABEL_PREFIX: &str = "kubernetes.pod_labels.";

/// The HTTP client: no read timeout, since following the logs can be
/// quiet for long; each query sets its own.
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        // graviola is pure Rust, see `kube::cloudflare::client`.
        let _ = rustls_graviola::default_provider().install_default();
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .build()
            .expect("HTTP client")
    })
}

/// Which logs: every condition given has to hold, and several values of
/// one condition are alternatives.
#[derive(Debug, Default, Clone)]
pub struct Filter {
    pub namespaces: Vec<String>,
    /// Pods, or the start of their names: `backend` is every pod of the
    /// backend Deployment.
    pub pods: Vec<String>,
    pub containers: Vec<String>,
    pub nodes: Vec<String>,
    /// `key=value` pod labels.
    pub labels: Vec<String>,
    /// Only lines that look like errors.
    pub errors: bool,
    /// Words to look for, or any LogsQL filter, as typed.
    pub words: Vec<String>,
}

impl Filter {
    /// The LogsQL filter, with no time range (as tailing wants it).
    pub fn query(&self) -> Result<String> {
        let mut parts = Vec::new();
        if let Some(p) = any(NAMESPACE_FIELD, &self.namespaces, false) {
            parts.push(p);
        }
        if let Some(p) = any(POD_FIELD, &self.pods, true) {
            parts.push(p);
        }
        if let Some(p) = any(CONTAINER_FIELD, &self.containers, false) {
            parts.push(p);
        }
        if let Some(p) = any(NODE_FIELD, &self.nodes, false) {
            parts.push(p);
        }
        for label in &self.labels {
            let Some((key, value)) = label.split_once('=') else {
                bail!("a label is key=value, not {label:?}");
            };
            parts.push(format!(
                "{}:={}",
                quote(&format!("{LABEL_PREFIX}{}", key.trim())),
                quote(value.trim())
            ));
        }
        if self.errors {
            // The words most logs use, and the levels of those that log
            // JSON, pino's numbers too.
            parts.push(
                r#"(i(error) or i(fatal) or i(panic) or level:in("error","fatal","50","60"))"#
                    .to_string(),
            );
        }
        if !self.words.is_empty() {
            parts.push(format!("({})", self.words.join(" ")));
        }
        Ok(if parts.is_empty() {
            "*".to_string()
        } else {
            parts.join(" ")
        })
    }

    /// The filter over the time `since` covers.
    pub fn query_since(&self, since: &str) -> Result<String> {
        Ok(format!("{} {}", time(since)?, self.query()?))
    }
}

/// `field` is one of `values` (or starts with one, for `prefix`).
fn any(field: &str, values: &[String], prefix: bool) -> Option<String> {
    let field = quote(field);
    let one = |v: &String| {
        if prefix {
            format!("{field}:{}*", quote(v))
        } else {
            format!("{field}:={}", quote(v))
        }
    };
    match values {
        [] => None,
        [v] => Some(one(v)),
        _ => Some(format!(
            "({})",
            values.iter().map(one).collect::<Vec<_>>().join(" or ")
        )),
    }
}

/// A LogsQL string: JSON's quoting is the same.
fn quote(s: &str) -> String {
    serde_json::to_string(s).expect("a string is JSON")
}

/// The time filter for `since`: a duration back from now (`5m`, `2h`,
/// `7d`) or a moment (`2026-10-11`, `2026-10-11T15:00:00Z`).
pub fn time(since: &str) -> Result<String> {
    let since = since.trim();
    let duration = since.starts_with(|c: char| c.is_ascii_digit())
        && since.ends_with(|c: char| c.is_ascii_alphabetic())
        && since.chars().all(|c| c.is_ascii_alphanumeric() || c == '.');
    if duration {
        Ok(format!("_time:{since}"))
    } else if since.starts_with(|c: char| c.is_ascii_digit()) {
        Ok(format!("_time:>={since}"))
    } else {
        bail!("--since is a duration like 15m or 2d, or a time like 2026-10-11T15:00:00Z")
    }
}

/// VictoriaLogs, over a port-forward or at a URL.
pub struct Logs {
    url: String,
    _forward: Option<Forward>,
}

impl Logs {
    /// At `UC_LOGS_URL`, else through the cluster behind `context`.
    pub async fn connect(context: impl FnOnce() -> Result<String>) -> Result<Logs> {
        if let Some(url) = std::env::var("UC_LOGS_URL")
            .ok()
            .filter(|u| !u.trim().is_empty())
        {
            return Ok(Logs {
                url: url.trim().trim_end_matches('/').to_string(),
                _forward: None,
            });
        }
        let forward = port_forward(&context()?, NAMESPACE, &format!("svc/{LOGS}"), LOGS_PORT)
            .await
            .context("reaching VictoriaLogs; is `uc kube configure -t obs` done?")?;
        Ok(Logs {
            url: forward.url.clone(),
            _forward: Some(forward),
        })
    }

    async fn get(&self, endpoint: &str, query: &str) -> Result<reqwest::Response> {
        let res = client()
            .get(format!("{}/select/logsql/{endpoint}", self.url))
            .query(&[("query", query)])
            .send()
            .await
            .with_context(|| format!("asking VictoriaLogs at {}", self.url))?;
        if !res.status().is_success() {
            let status = res.status();
            bail!(
                "VictoriaLogs answered {status} to {query:?}: {}",
                res.text().await.unwrap_or_default().trim()
            );
        }
        Ok(res)
    }

    /// Every entry `query` finds, one JSON object per line.
    pub async fn query(&self, query: &str) -> Result<Vec<Value>> {
        let body = tokio::time::timeout(Duration::from_secs(120), async {
            anyhow::Ok(self.get("query", query).await?.text().await?)
        })
        .await
        .context("VictoriaLogs did not answer in 120s")??;
        body.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).with_context(|| format!("reading {l:?}")))
            .collect()
    }

    /// The last `limit` entries that `filter` finds, oldest first.
    pub async fn last(&self, filter: &str, limit: usize) -> Result<Vec<Value>> {
        let mut entries = self
            .query(&format!("{filter} | sort by (_time desc) limit {limit}"))
            .await?;
        entries.reverse();
        Ok(entries)
    }

    /// How many lines each value of `fields` has, and its last one, most
    /// lines first.
    pub async fn count_by(&self, filter: &str, fields: &[&str]) -> Result<Vec<Value>> {
        let by = fields
            .iter()
            .map(|f| quote(f))
            .collect::<Vec<_>>()
            .join(", ");
        self.query(&format!(
            "{filter} | stats by ({by}) count() logs, max(_time) last | sort by (logs desc)"
        ))
        .await
    }

    /// The fields the entries `filter` finds have, with how many have each.
    pub async fn fields(&self, filter: &str) -> Result<Vec<(String, u64)>> {
        let body: Value = self.get("field_names", filter).await?.json().await?;
        Ok(body["values"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| {
                (
                    v["value"].as_str().unwrap_or_default().to_string(),
                    v["hits"].as_u64().unwrap_or_default(),
                )
            })
            .collect())
    }

    /// Calls `each` with every new entry `filter` finds, as they come,
    /// until it returns false.
    pub async fn tail(&self, filter: &str, mut each: impl FnMut(Value) -> bool) -> Result<()> {
        let mut res = self.get("tail", filter).await?;
        let mut buf = Vec::new();
        while let Some(chunk) = res.chunk().await? {
            buf.extend_from_slice(&chunk);
            while let Some(end) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=end).collect();
                if let Ok(entry) = serde_json::from_slice(&line)
                    && !each(entry)
                {
                    return Ok(());
                }
            }
        }
        bail!("VictoriaLogs ended the tail")
    }
}

/// `_time` in a form that sorts: VictoriaLogs leaves out the trailing
/// zeros of the nanoseconds.
pub fn sort_key(entry: &Value) -> String {
    let t = entry["_time"].as_str().unwrap_or_default();
    let t = t.trim_end_matches('Z');
    match t.split_once('.') {
        Some((s, frac)) => format!("{s}.{frac:0<9}"),
        None => format!("{t}.000000000"),
    }
}

/// `2026-10-11T16:21:48.704633709Z` to `2026-10-11 16:21:48.704`.
pub fn short_time(t: &str) -> String {
    let t = t.trim_end_matches('Z').replacen('T', " ", 1);
    match t.split_once('.') {
        Some((s, frac)) => format!("{s}.{:0<3}", &frac[..frac.len().min(3)]),
        None => format!("{t}.000"),
    }
}

/// Where a line came from: `namespace/pod[container]`.
pub fn source(entry: &Value) -> String {
    let field = |f: &str| entry[f].as_str().unwrap_or("?").to_string();
    format!(
        "{}/{}[{}]",
        field(NAMESPACE_FIELD),
        field(POD_FIELD),
        field(CONTAINER_FIELD)
    )
}

/// One entry as a line: when, where from, and the message.
pub fn line(entry: &Value, color: bool) -> String {
    let time = short_time(entry["_time"].as_str().unwrap_or_default());
    let from = source(entry);
    let msg = entry["_msg"].as_str().unwrap_or_default().trim_end();
    if color {
        // The same pod keeps its color.
        let pod = entry[POD_FIELD].as_str().unwrap_or_default();
        let hue = 31 + pod.bytes().fold(0u32, |h, b| h.wrapping_mul(31) + b as u32) % 6;
        format!("\x1b[2m{time}\x1b[0m \x1b[{hue}m{from}\x1b[0m {msg}")
    } else {
        format!("{time} {from} {msg}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn everything_without_conditions() {
        assert_eq!(Filter::default().query().unwrap(), "*");
    }

    #[test]
    fn conditions_all_hold() {
        let f = Filter {
            namespaces: vec!["obs".into()],
            pods: vec!["backend".into(), "frontend".into()],
            labels: vec!["app.kubernetes.io/name=web".into()],
            words: vec!["timeout".into(), "-healthz".into()],
            ..Filter::default()
        };
        assert_eq!(
            f.query_since("15m").unwrap(),
            r#"_time:15m "kubernetes.pod_namespace":="obs" ("kubernetes.pod_name":"backend"* or "kubernetes.pod_name":"frontend"*) "kubernetes.pod_labels.app.kubernetes.io/name":="web" (timeout -healthz)"#
        );
    }

    #[test]
    fn labels_need_a_value() {
        let f = Filter {
            labels: vec!["app".into()],
            ..Filter::default()
        };
        assert!(f.query().is_err());
    }

    #[test]
    fn since_is_a_duration_or_a_moment() {
        assert_eq!(time("2h").unwrap(), "_time:2h");
        assert_eq!(time("1h30m").unwrap(), "_time:1h30m");
        assert_eq!(time("2026-10-11").unwrap(), "_time:>=2026-10-11");
        assert_eq!(
            time("2026-10-11T15:00:00Z").unwrap(),
            "_time:>=2026-10-11T15:00:00Z"
        );
        assert!(time("yesterday").is_err());
    }

    #[test]
    fn times_sort_and_shorten() {
        let a = json!({"_time": "2026-10-11T16:21:48.7Z"});
        let b = json!({"_time": "2026-10-11T16:21:48.698862758Z"});
        assert!(sort_key(&a) > sort_key(&b));
        assert_eq!(
            short_time("2026-10-11T16:21:48.7Z"),
            "2026-10-11 16:21:48.700"
        );
        assert_eq!(
            short_time("2026-10-11T16:21:48Z"),
            "2026-10-11 16:21:48.000"
        );
    }

    #[test]
    fn a_line_says_where_it_came_from() {
        let e = json!({
            "_time": "2026-10-11T16:21:48.704633709Z",
            "_msg": "request completed\n",
            "kubernetes.pod_namespace": "shop",
            "kubernetes.pod_name": "backend-57fc5cf79d-j52rx",
            "kubernetes.container_name": "backend",
        });
        assert_eq!(
            line(&e, false),
            "2026-10-11 16:21:48.704 shop/backend-57fc5cf79d-j52rx[backend] request completed"
        );
    }
}
