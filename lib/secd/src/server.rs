// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! HTTP API: POST /signing-key, /upsert and /read with SSH pubkey auth
//! (per tenant / resource group `authorized_keys`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Duration, Utc};
use serde_json::{Map, Value, json};

use crate::auth;
use crate::keys::{self, RawPub};
use crate::store;
use crate::vault;

const MAX_SKEW_SECS: i64 = 5 * 60;
const SIGNING_KEY_TTL_SECS: i64 = 10 * 60;

/// Outcome of a request: status plus JSON body, or a plain-text 500.
#[derive(Debug, PartialEq)]
pub enum Reply {
    Json(u16, Value),
    Internal,
}

fn detail(status: u16, msg: impl Into<String>) -> Reply {
    Reply::Json(status, json!({ "detail": msg.into() }))
}

struct SigningKey {
    key: String,
    created_at: DateTime<Utc>,
}

/// Server state: per (tenant, resource group) short-lived signing keys.
#[derive(Default)]
pub struct State {
    signing_keys: Mutex<HashMap<(String, String), SigningKey>>,
}

impl State {
    fn ttl() -> Duration {
        Duration::seconds(SIGNING_KEY_TTL_SECS)
    }

    /// Return (key, expires_at), creating a new key if expired or missing.
    fn get_or_create(&self, tenant: &str, rg: &str, now: DateTime<Utc>) -> (String, DateTime<Utc>) {
        let mut map = self.signing_keys.lock().unwrap_or_else(|e| e.into_inner());
        let k = (tenant.to_string(), rg.to_string());
        let fresh = map
            .get(&k)
            .is_some_and(|e| now - e.created_at < Self::ttl());
        if !fresh {
            let mut bytes = [0u8; 32];
            getrandom::fill(&mut bytes).expect("OS random number generator");
            map.insert(
                k.clone(),
                SigningKey {
                    key: B64.encode(bytes),
                    created_at: now,
                },
            );
        }
        let e = &map[&k];
        (e.key.clone(), e.created_at + Self::ttl())
    }

    /// Current signing key if present and not expired.
    fn current(&self, tenant: &str, rg: &str, now: DateTime<Utc>) -> Option<String> {
        let map = self.signing_keys.lock().unwrap_or_else(|e| e.into_inner());
        map.get(&(tenant.to_string(), rg.to_string()))
            .filter(|e| now - e.created_at < Self::ttl())
            .map(|e| e.key.clone())
    }
}

fn load_authorized_keys(tenant: &str, rg: &str) -> Result<Vec<RawPub>, Reply> {
    let path = store::authorized_keys_path(tenant, rg).map_err(|e| detail(400, e))?;
    if !store::exists(&path) {
        return Err(detail(
            403,
            format!("No authorized_keys file found for tenant '{tenant}' / resource-group '{rg}'"),
        ));
    }
    let text = std::fs::read_to_string(&path).map_err(|_| Reply::Internal)?;
    let keys: Vec<RawPub> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| keys::parse_openssh_pub(l).ok().flatten())
        .collect();
    if keys.is_empty() {
        return Err(detail(
            403,
            format!("No valid ed25519 public keys found in {path}"),
        ));
    }
    Ok(keys)
}

/// Parse the public key carried in the token: OpenSSH text or raw base64.
fn parse_client_pubkey(s: &str) -> Result<RawPub, String> {
    let s = s.trim();
    if s.starts_with("ssh-ed25519 ") {
        return keys::parse_openssh_pub(s)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not an ed25519 key".to_string());
    }
    let raw = B64.decode(s).map_err(|e| e.to_string())?;
    raw.try_into()
        .map_err(|_| "Ed25519 raw public key must be 32 bytes".to_string())
}

/// Request data needed for authentication.
pub struct Req<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub tenant: Option<&'a str>,
    pub authorization: Option<&'a str>,
    pub body: &'a Value,
}

/// Authenticate a request; returns (tenant, resource_group).
fn verify_ssh(
    state: &State,
    req: &Req,
    require_signing_key: bool,
    now: DateTime<Utc>,
) -> Result<(String, String), Reply> {
    let tenant = req
        .tenant
        .filter(|t| !t.is_empty())
        .ok_or_else(|| detail(401, "Missing header: X-Tenant"))?;

    let rg = req
        .body
        .get("resource_group")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| detail(400, "Invalid JSON or missing resource_group"))?;

    let header = req
        .authorization
        .filter(|h| !h.is_empty())
        .ok_or_else(|| detail(401, "Missing Authorization header"))?;
    let token = auth::parse_header(header)
        .map_err(|e| detail(401, format!("Invalid Authorization token: {e}")))?;

    let ts = auth::parse_timestamp(&token.timestamp)
        .ok_or_else(|| detail(401, "Invalid timestamp (ISO UTC expected)"))?;
    if (now - ts).abs() > Duration::seconds(MAX_SKEW_SECS) {
        return Err(detail(401, "Timestamp outside allowed window (±5 min)"));
    }

    let signing_key = if require_signing_key {
        Some(state.current(tenant, rg, now).ok_or_else(|| {
            detail(
                401,
                "No active signing key for this tenant/resource-group; fetch one via POST /signing-key first",
            )
        })?)
    } else {
        None
    };
    let message = auth::message(
        tenant,
        &token.nonce,
        &token.timestamp,
        req.method,
        req.path,
        signing_key.as_deref(),
    );

    let client = parse_client_pubkey(&token.pubkey)
        .map_err(|e| detail(400, format!("Invalid public key format: {e}")))?;

    let allowed = load_authorized_keys(tenant, rg)?;
    if !allowed.contains(&client) {
        return Err(detail(
            403,
            "Public key not authorized for this tenant/resource-group",
        ));
    }

    if !keys::verify(&client, &token.signature, message.as_bytes()) {
        return Err(detail(401, "Invalid signature"));
    }

    Ok((tenant.to_string(), rg.to_string()))
}

/// Validate that `body` is an object with the given string fields.
fn validate<'a>(body: &'a Value, fields: &[&str]) -> Result<Vec<&'a str>, Reply> {
    let Some(obj) = body.as_object() else {
        return Err(Reply::Json(
            422,
            json!({"detail": [{
                "type": "model_attributes_type",
                "loc": ["body"],
                "msg": "Input should be a valid dictionary or object to extract fields from",
                "input": body,
            }]}),
        ));
    };
    let mut errors = Vec::new();
    let mut out = Vec::new();
    for f in fields {
        match obj.get(*f) {
            None => errors.push(json!({
                "type": "missing", "loc": ["body", f], "msg": "Field required", "input": body,
            })),
            Some(Value::String(s)) => out.push(s.as_str()),
            Some(v) => errors.push(json!({
                "type": "string_type", "loc": ["body", f],
                "msg": "Input should be a valid string", "input": v,
            })),
        }
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(Reply::Json(422, json!({ "detail": errors })))
    }
}

fn mismatch() -> Reply {
    detail(
        400,
        "resource_group in body must match authenticated resource group",
    )
}

/// Route and handle one request.
pub fn handle(
    state: &State,
    method: &str,
    path: &str,
    tenant: Option<&str>,
    authorization: Option<&str>,
    raw_body: &[u8],
    now: DateTime<Utc>,
) -> Reply {
    if !matches!(path, "/signing-key" | "/upsert" | "/read") {
        return detail(404, "Not Found");
    }
    if method != "POST" {
        return detail(405, "Method Not Allowed");
    }

    let body = if raw_body.iter().all(u8::is_ascii_whitespace) {
        Value::Null
    } else {
        match serde_json::from_slice::<Value>(raw_body) {
            Ok(v) => v,
            Err(e) => {
                return Reply::Json(
                    422,
                    json!({"detail": [{
                        "type": "json_invalid",
                        "loc": ["body", e.column().saturating_sub(1)],
                        "msg": "JSON decode error",
                        "input": Map::new(),
                        "ctx": {"error": e.to_string()},
                    }]}),
                );
            }
        }
    };

    let req = Req {
        method,
        path,
        tenant,
        authorization,
        body: &body,
    };

    let result = match path {
        "/signing-key" => signing_key(state, &req, now),
        "/upsert" => upsert(state, &req, now),
        _ => read(state, &req, now),
    };
    result.unwrap_or_else(|r| r)
}

fn signing_key(state: &State, req: &Req, now: DateTime<Utc>) -> Result<Reply, Reply> {
    let (tenant, rg) = verify_ssh(state, req, false, now)?;
    let f = validate(req.body, &["resource_group"])?;
    if f[0] != rg {
        return Err(mismatch());
    }
    let (key, expires_at) = state.get_or_create(&tenant, &rg, now);
    Ok(Reply::Json(
        200,
        json!({"signing_key": key, "expires_at": auth::py_isoformat(expires_at)}),
    ))
}

fn upsert(state: &State, req: &Req, now: DateTime<Utc>) -> Result<Reply, Reply> {
    let (tenant, rg) = verify_ssh(state, req, true, now)?;
    let f = validate(
        req.body,
        &["resource_group", "name", "value", "vault_password"],
    )?;
    if f[0] != rg {
        return Err(mismatch());
    }
    let path = store::secret_path(&tenant, &rg, f[1]).map_err(|e| detail(400, e))?;
    store::write_secret(&path, f[2], f[3]).map_err(|e| {
        eprintln!("ERROR:    {e:#}");
        Reply::Internal
    })?;
    Ok(Reply::Json(
        200,
        json!({"status": "upserted", "path": path}),
    ))
}

fn read(state: &State, req: &Req, now: DateTime<Utc>) -> Result<Reply, Reply> {
    let (tenant, rg) = verify_ssh(state, req, true, now)?;
    let f = validate(req.body, &["resource_group", "name", "vault_password"])?;
    if f[0] != rg {
        return Err(mismatch());
    }
    let path = store::secret_path(&tenant, &rg, f[1]).map_err(|e| detail(400, e))?;
    if !store::exists(&path) {
        return Err(detail(404, "Secret not found"));
    }
    let text = std::fs::read_to_string(&path).map_err(|_| Reply::Internal)?;
    match vault::decrypt(&text, f[2]) {
        Ok(value) => Ok(Reply::Json(200, json!({ "value": value }))),
        Err(e) => Err(detail(400, e)),
    }
}

fn header<'a>(req: &'a tiny_http::Request, name: &'static str) -> Option<&'a str> {
    req.headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str())
}

fn respond(state: &State, mut req: tiny_http::Request) {
    let mut body = Vec::new();
    let reply = match req.as_reader().read_to_end(&mut body) {
        Ok(_) => {
            let method = req.method().as_str().to_string();
            let url = req.url().to_string();
            let path = url.split(['?', '#']).next().unwrap_or("");
            handle(
                state,
                &method,
                path,
                header(&req, "X-Tenant"),
                header(&req, "Authorization"),
                &body,
                Utc::now(),
            )
        }
        Err(_) => Reply::Internal,
    };

    let (status, content_type, payload) = match reply {
        Reply::Json(s, v) => (s, "application/json", v.to_string()),
        Reply::Internal => (
            500,
            "text/plain; charset=utf-8",
            "Internal Server Error".into(),
        ),
    };
    let code = tiny_http::StatusCode(status);
    let peer = req
        .remote_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|| "-".into());
    eprintln!(
        "INFO:     {peer} - \"{} {} HTTP/{}\" {status} {}",
        req.method(),
        req.url(),
        req.http_version(),
        code.default_reason_phrase()
    );
    let ct = tiny_http::Header::from_bytes("Content-Type", content_type).expect("valid header");
    let resp = tiny_http::Response::from_string(payload)
        .with_status_code(code)
        .with_header(ct);
    if let Err(e) = req.respond(resp) {
        eprintln!("ERROR:    failed to send response: {e}");
    }
}

/// Serve forever on `addr`.
pub fn serve(addr: &str) -> Result<()> {
    let server = tiny_http::Server::http(addr).map_err(|e| anyhow!("binding {addr}: {e}"))?;
    let state = Arc::new(State::default());
    eprintln!("INFO:     Listening on http://{addr}");
    for req in server.incoming_requests() {
        let state = Arc::clone(&state);
        std::thread::spawn(move || respond(&state, req));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(
        method: &str,
        path: &str,
        tenant: Option<&str>,
        auth: Option<&str>,
        body: &str,
    ) -> Reply {
        handle(
            &State::default(),
            method,
            path,
            tenant,
            auth,
            body.as_bytes(),
            Utc::now(),
        )
    }

    fn msg(r: Reply) -> (u16, String) {
        match r {
            Reply::Json(s, v) => (s, v["detail"].as_str().unwrap_or("").to_string()),
            Reply::Internal => (500, String::new()),
        }
    }

    #[test]
    fn routing() {
        assert_eq!(
            msg(call("POST", "/nope", None, None, "")),
            (404, "Not Found".into())
        );
        assert_eq!(
            msg(call("GET", "/read", None, None, "")),
            (405, "Method Not Allowed".into())
        );
        let Reply::Json(s, _) = call("POST", "/read", None, None, "{bad") else {
            panic!()
        };
        assert_eq!(s, 422);
    }

    #[test]
    fn auth_order() {
        assert_eq!(
            msg(call("POST", "/read", None, None, "{}")),
            (401, "Missing header: X-Tenant".into())
        );
        assert_eq!(
            msg(call("POST", "/read", Some("t"), None, "{}")),
            (400, "Invalid JSON or missing resource_group".into())
        );
        assert_eq!(
            msg(call(
                "POST",
                "/read",
                Some("t"),
                None,
                r#"{"resource_group":"rg"}"#
            )),
            (401, "Missing Authorization header".into())
        );
        assert_eq!(
            msg(call(
                "POST",
                "/read",
                Some("t"),
                Some("Bearer x"),
                r#"{"resource_group":"rg"}"#
            )),
            (
                401,
                "Invalid Authorization token: Expected 'Signature <token>' format".into()
            )
        );
        let tok = |ts: &str| {
            format!(
                "Signature {}",
                auth::build_token(&auth::Token {
                    pubkey: "x".into(),
                    timestamp: ts.into(),
                    nonce: "n".into(),
                    signature: "s".into(),
                })
            )
        };
        let body = r#"{"resource_group":"rg"}"#;
        assert_eq!(
            msg(call("POST", "/read", Some("t"), Some(&tok("nope")), body)),
            (401, "Invalid timestamp (ISO UTC expected)".into())
        );
        assert_eq!(
            msg(call(
                "POST",
                "/read",
                Some("t"),
                Some(&tok("2000-01-01T00:00:00Z")),
                body
            )),
            (401, "Timestamp outside allowed window (±5 min)".into())
        );
        let now = auth::format_timestamp(Utc::now());
        assert_eq!(
            msg(call("POST", "/read", Some("t"), Some(&tok(&now)), body)).1,
            "No active signing key for this tenant/resource-group; fetch one via POST /signing-key first"
        );
        let (s, m) = msg(call(
            "POST",
            "/signing-key",
            Some("t"),
            Some(&tok(&now)),
            body,
        ));
        assert_eq!(s, 400);
        assert!(m.starts_with("Invalid public key format: "), "{m}");
    }

    #[test]
    fn signing_key_ttl() {
        let st = State::default();
        let t0 = Utc::now();
        assert_eq!(st.current("t", "rg", t0), None);
        let (k1, exp) = st.get_or_create("t", "rg", t0);
        assert_eq!(exp, t0 + Duration::minutes(10));
        assert_eq!(st.current("t", "rg", t0).as_deref(), Some(k1.as_str()));
        let (k2, _) = st.get_or_create("t", "rg", t0 + Duration::minutes(5));
        assert_eq!(k1, k2);
        let later = t0 + Duration::minutes(10);
        assert_eq!(st.current("t", "rg", later), None);
        let (k3, _) = st.get_or_create("t", "rg", later);
        assert_ne!(k1, k3);
        assert_eq!(B64.decode(k3).unwrap().len(), 32);
    }

    #[test]
    fn body_validation() {
        let body = json!({"resource_group": "rg", "name": 5});
        let Err(Reply::Json(422, v)) = validate(&body, &["resource_group", "name", "value"]) else {
            panic!()
        };
        let d = v["detail"].as_array().unwrap();
        assert_eq!(d.len(), 2);
        assert_eq!(d[0]["type"], "string_type");
        assert_eq!(d[1]["type"], "missing");
    }

    #[test]
    fn client_pubkey_formats() {
        let raw = keys::public_of(&[1u8; 32]);
        assert_eq!(
            parse_client_pubkey(&keys::openssh_string(&raw)).unwrap(),
            raw
        );
        assert_eq!(parse_client_pubkey(&B64.encode(raw)).unwrap(), raw);
        assert_eq!(
            parse_client_pubkey(&B64.encode([0u8; 31])).unwrap_err(),
            "Ed25519 raw public key must be 32 bytes"
        );
    }
}
