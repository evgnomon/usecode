// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! HTTP client for a remote secd server.

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::Value;

use crate::auth;
use crate::keys;

/// Key material and identity for signing requests.
pub struct Signer<'a> {
    pub tenant: &'a str,
    pub pubkey: &'a str,
    pub privkey: &'a str,
}

impl Signer<'_> {
    /// Authorization header value for `POST path`.
    fn authorization(&self, method: &str, path: &str, signing_key: Option<&str>) -> Result<String> {
        let public = keys::load_pubkey(self.pubkey)?;
        let seed = keys::load_priv_seed(self.privkey)?;

        let mut rnd = [0u8; 16];
        getrandom::fill(&mut rnd).map_err(|e| anyhow!("random: {e}"))?;
        let nonce: String = B64.encode(rnd).chars().take(22).collect();
        let ts = auth::format_timestamp(chrono::Utc::now());
        let msg = auth::message(self.tenant, &nonce, &ts, method, path, signing_key);

        let token = auth::build_token(&auth::Token {
            pubkey: keys::openssh_string(&public),
            timestamp: ts,
            nonce,
            signature: keys::sign(&seed, msg.as_bytes()),
        });
        Ok(format!("Signature {token}"))
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build()
}

/// POST `payload` as JSON to `remote + path` and return the decoded JSON.
fn post(
    remote: &str,
    path: &str,
    signer: &Signer,
    signing_key: Option<&str>,
    payload: &Value,
) -> Result<Value> {
    let url = format!("{}{path}", remote.trim_end_matches('/'));
    let authz = signer.authorization("POST", path, signing_key)?;
    let resp = agent()
        .post(&url)
        .set("X-Tenant", signer.tenant)
        .set("Authorization", &authz)
        .set("Content-Type", "application/json")
        .send_string(&payload.to_string());
    match resp {
        Ok(r) => r.into_json().context("decoding JSON response"),
        Err(ureq::Error::Status(code, r)) => {
            let kind = if code < 500 { "Client" } else { "Server" };
            let reason = r.status_text().to_string();
            let body = r.into_string().unwrap_or_default();
            bail!("{code} {kind} Error: {reason} for url: {url}\n{body}")
        }
        Err(e) => Err(anyhow!("{e}")),
    }
}

/// Fetch a signing key (SSH-only auth) for `resource_group`.
fn fetch_signing_key(remote: &str, signer: &Signer, resource_group: &str) -> Result<String> {
    let v = post(
        remote,
        "/signing-key",
        signer,
        None,
        &serde_json::json!({ "resource_group": resource_group }),
    )?;
    v.get("signing_key")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("KeyError: 'signing_key'"))
}

/// Fetch a signing key, then POST `payload` to `path` signed with it.
pub fn call(
    remote: &str,
    signer: &Signer,
    resource_group: &str,
    path: &str,
    payload: &Value,
) -> Result<Value> {
    let sk = fetch_signing_key(remote, signer, resource_group)?;
    post(remote, path, signer, Some(&sk), payload)
}

/// Python `repr()` of a JSON value (as `click.echo(dict)` prints it).
pub fn py_repr(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => py_str_repr(s),
        Value::Array(a) => {
            let items: Vec<String> = a.iter().map(py_repr).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(o) => {
            let items: Vec<String> = o
                .iter()
                .map(|(k, v)| format!("{}: {}", py_str_repr(k), py_repr(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
    }
}

fn py_str_repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn repr_like_python() {
        let v = json!({"status": "upserted", "path": "/var/secrets/a/b/secrets/c"});
        assert_eq!(
            py_repr(&v),
            "{'status': 'upserted', 'path': '/var/secrets/a/b/secrets/c'}"
        );
        assert_eq!(py_str_repr("it's"), "\"it's\"");
        assert_eq!(py_str_repr("a'b\"c"), "'a\\'b\"c'");
        assert_eq!(py_repr(&json!([1, true, null])), "[1, True, None]");
    }
}
