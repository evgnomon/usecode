// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! /hosts: usecoded calling home.
//!
//! `uc daemon install` asks for a one-time enrollment token with the
//! user's API key and hands it to the host. The daemon there registers
//! with it (`POST /hosts/register`) and gets a secret of its own, which it
//! sends with a heartbeat every few minutes (`POST /hosts/heartbeat`). The
//! user's key never goes on the host. Deleting a host here makes its secret
//! stop working, and the daemon then stops calling.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::App;
use crate::error::{AppError, AppResult};
use crate::extract::{Client, JsonBody};
use crate::models::{
    HeartbeatIn, HeartbeatOut, HostCountsOut, HostEnrollmentOut, HostListOut, HostOut,
    HostRegisterIn, HostRegisterOut,
};
use crate::store::{Heartbeat, HostQuery, HostRow, HostSort, HostState, NewHost, ts};

/// How often a host is asked to call.
pub const HEARTBEAT_SECONDS: u64 = 300;

/// A host is online while it last called within this many heartbeats.
const MISSED_BEATS: u64 = 3;

/// How long an enrollment token is good for.
const ENROLLMENT_TTL: Duration = Duration::hours(1);

/// The most module statuses one heartbeat may carry.
const MAX_MODULES: usize = 64;

/// Hosts per page unless asked otherwise, and the most one page holds.
const PAGE: i64 = 50;
const MAX_PAGE: i64 = 200;

/// How long a host may be silent and still be online.
pub(crate) fn online_window() -> Duration {
    Duration::seconds((HEARTBEAT_SECONDS * MISSED_BEATS) as i64)
}

pub fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/hosts", get(list_hosts))
        .route("/hosts/enrollments", post(create_enrollment))
        .route("/hosts/register", post(register))
        .route("/hosts/heartbeat", post(heartbeat))
        .route("/hosts/{host_id}", delete(delete_host))
}

async fn create_enrollment(
    State(app): State<Arc<App>>,
    Client(client): Client,
) -> AppResult<Json<HostEnrollmentOut>> {
    let expires_at = Utc::now() + ENROLLMENT_TTL;
    let token = app
        .store
        .issue_host_enrollment(&client.user_id, expires_at)
        .await?;
    Ok(Json(HostEnrollmentOut {
        token,
        expires_at: ts(expires_at),
    }))
}

async fn register(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    JsonBody(payload): JsonBody<HostRegisterIn>,
) -> AppResult<Json<HostRegisterOut>> {
    let name = clip(&payload.name, 253);
    let machine_id = clip(&payload.machine_id, 128);
    if name.is_empty() {
        return Err(AppError::invalid(&["body", "name"], "A host needs a name."));
    }
    if machine_id.is_empty() {
        return Err(AppError::invalid(
            &["body", "machine_id"],
            "A host needs a machine id.",
        ));
    }
    let ip = client_ip(&headers);
    let host = NewHost {
        name: &name,
        machine_id: &machine_id,
        version: &clip(&payload.version, 64),
        arch: &clip(&payload.arch, 32),
        os: &clip(&payload.os, 128),
        ip: ip.as_deref(),
    };
    let (row, secret) = app
        .store
        .register_host(&payload.token, host)
        .await?
        .ok_or_else(|| {
            AppError::unauthorized(
                "That enrollment token is unknown, used or ran out; \
                 `uc daemon install` hands the host a new one.",
            )
        })?;
    tracing::info!(host = %row.id, name = %row.name, "host registered");
    Ok(Json(HostRegisterOut {
        id: row.id.to_string(),
        secret,
        heartbeat_seconds: HEARTBEAT_SECONDS,
    }))
}

async fn heartbeat(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    JsonBody(payload): JsonBody<HeartbeatIn>,
) -> AppResult<Json<HeartbeatOut>> {
    let secret = bearer(&headers).ok_or_else(|| AppError::unauthorized("Missing host secret"))?;
    let mut modules = payload.modules;
    modules.truncate(MAX_MODULES);
    for m in &mut modules {
        m.name = clip(&m.name, 64);
        m.status = clip(&m.status, 32);
        m.detail = m.detail.as_deref().map(|d| clip(d, 512));
    }
    let modules = serde_json::to_value(modules).map_err(anyhow::Error::from)?;
    let ip = client_ip(&headers);
    let workspaces = payload
        .workspaces
        .map(super::workspaces::reported)
        .transpose()?;
    let cluster_id = clip(payload.cluster_id.as_deref().unwrap_or_default(), 64);
    let beat = Heartbeat {
        version: &clip(&payload.version, 64),
        modules: &modules,
        uptime_seconds: payload.uptime_seconds,
        ip: ip.as_deref(),
        workspaces: workspaces.as_ref().map(|w| (w, cluster_id.as_str())),
    };
    let row = app
        .store
        .host_heartbeat(&secret, beat)
        .await?
        .ok_or_else(|| AppError::unauthorized("This host is not registered"))?;
    Ok(Json(HeartbeatOut {
        id: row.id.to_string(),
        heartbeat_seconds: HEARTBEAT_SECONDS,
    }))
}

/// `GET /hosts?q=web&state=offline&sort=quiet&limit=50&offset=0`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ListQuery {
    q: String,
    state: Option<String>,
    sort: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list_hosts(
    State(app): State<Arc<App>>,
    Client(client): Client,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<HostListOut>> {
    let state = match query.state.as_deref().unwrap_or("all") {
        "all" => HostState::All,
        "online" => HostState::Online,
        "offline" => HostState::Offline,
        "failing" => HostState::Failing,
        _ => {
            return Err(AppError::invalid(
                &["query", "state"],
                "state is one of all, online, offline and failing",
            ));
        }
    };
    let sort = match query.sort.as_deref().unwrap_or("name") {
        "name" => HostSort::Name,
        "quiet" => HostSort::Quiet,
        _ => {
            return Err(AppError::invalid(
                &["query", "sort"],
                "sort is name or quiet",
            ));
        }
    };
    let limit = query.limit.unwrap_or(PAGE).clamp(1, MAX_PAGE);
    let offset = query.offset.unwrap_or(0).max(0);
    let search = clip(&query.q, 100);
    let (rows, counts) = app
        .store
        .list_hosts(
            &client.partition,
            &client.user_id,
            &HostQuery {
                search: &search,
                state,
                sort,
                online_since: Utc::now() - online_window(),
                limit,
                offset,
            },
        )
        .await?;
    Ok(Json(HostListOut {
        hosts: rows.into_iter().map(host_out).collect(),
        total: counts.of(state),
        counts: HostCountsOut {
            all: counts.all,
            online: counts.online,
            offline: counts.offline,
            failing: counts.failing,
        },
        limit,
        offset,
    }))
}

async fn delete_host(
    State(app): State<Arc<App>>,
    Client(client): Client,
    Path(host_id): Path<String>,
) -> AppResult<StatusCode> {
    if !app
        .store
        .delete_host(&client.partition, &client.user_id, &host_id)
        .await?
    {
        return Err(AppError::not_found("Host not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

fn host_out(row: HostRow) -> HostOut {
    let online = Utc::now() - row.last_seen_at < online_window();
    let modules = match row.modules.0 {
        Value::Array(items) => items,
        _ => Vec::new(),
    };
    let failing = online && modules.iter().any(|m| m["status"] == "error");
    HostOut {
        id: row.id.to_string(),
        name: row.name,
        version: row.version,
        arch: row.arch,
        os: row.os,
        modules: Value::Array(modules),
        uptime_seconds: row.uptime_seconds,
        last_ip: row.last_ip,
        online,
        failing,
        last_seen_at: ts(row.last_seen_at),
        created_at: ts(row.created_at),
    }
}

/// The token in `Authorization: Bearer ...`.
pub(crate) fn bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// Where the request came from, as the ingress in front of the API says:
/// the first address in `X-Forwarded-For`, else `X-Real-IP`.
fn client_ip(headers: &HeaderMap) -> Option<String> {
    let forwarded = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next());
    let real = || headers.get("x-real-ip").and_then(|v| v.to_str().ok());
    forwarded
        .or_else(real)
        .map(str::trim)
        .filter(|ip| ip.parse::<std::net::IpAddr>().is_ok())
        .map(str::to_string)
}

/// `value` trimmed and cut to at most `max` characters.
pub(crate) fn clip(value: &str, max: usize) -> String {
    value.trim().chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use serde_json::json;

    use super::*;
    use crate::testing::{self, Answer};

    async fn call(
        app: &Arc<App>,
        method: &str,
        path: &str,
        auth: (&str, &str),
        body: Option<Value>,
    ) -> Answer {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(auth.0, auth.1);
        let body = match body {
            Some(json) => {
                request = request.header(header::CONTENT_TYPE, "application/json");
                Body::from(json.to_string())
            }
            None => Body::empty(),
        };
        testing::request_app(app, request.body(body).unwrap()).await
    }

    fn register_body(token: &str, machine: &str) -> Value {
        json!({
            "token": token, "name": "edge", "machine_id": machine,
            "version": "0.1.0", "arch": "x86_64", "os": "Debian GNU/Linux 13",
        })
    }

    #[tokio::test]
    async fn a_host_registers_calls_home_and_is_forgotten() {
        let Some(t) = testing::app(|_| {}).await else {
            return;
        };
        let app = &t.app;
        let (user, _) = app.store.get_or_create_user("+14155552671").await.unwrap();
        let key = app.store.issue_api_key(&user, "test").await.unwrap();
        let as_user = ("x-api-key", key.as_str());

        let enrolled = call(app, "POST", "/hosts/enrollments", as_user, None).await;
        assert_eq!(enrolled.status, StatusCode::OK, "{}", enrolled.body);
        let token = enrolled.body["token"].as_str().unwrap().to_string();

        let registered = call(
            app,
            "POST",
            "/hosts/register",
            ("x-forwarded-for", "203.0.113.7, 10.0.0.1"),
            Some(register_body(&token, "m1")),
        )
        .await;
        assert_eq!(registered.status, StatusCode::OK, "{}", registered.body);
        assert_eq!(registered.body["heartbeat_seconds"], HEARTBEAT_SECONDS);
        let id = registered.body["id"].as_str().unwrap().to_string();
        let secret = registered.body["secret"].as_str().unwrap().to_string();

        // A token is good once.
        let again = call(
            app,
            "POST",
            "/hosts/register",
            ("x-real-ip", "203.0.113.7"),
            Some(register_body(&token, "m1")),
        )
        .await;
        assert_eq!(again.status, StatusCode::UNAUTHORIZED);

        let bearer = format!("Bearer {secret}");
        let beat = call(
            app,
            "POST",
            "/hosts/heartbeat",
            ("authorization", &bearer),
            Some(json!({
                "version": "0.1.1", "uptime_seconds": 42,
                "modules": [{"name": "mesh", "status": "ok"},
                            {"name": "kube", "status": "error", "detail": "no k3s"}],
            })),
        )
        .await;
        assert_eq!(beat.status, StatusCode::OK, "{}", beat.body);
        assert_eq!(beat.body["id"], id.as_str());

        let listed = call(app, "GET", "/hosts", as_user, None).await;
        let hosts = listed.body["hosts"].as_array().unwrap();
        assert_eq!(hosts.len(), 1);
        let host = &hosts[0];
        assert_eq!(host["name"], "edge");
        assert_eq!(host["version"], "0.1.1");
        assert_eq!(host["uptime_seconds"], 42);
        assert_eq!(host["last_ip"], "203.0.113.7");
        assert_eq!(host["online"], true);
        assert_eq!(host["failing"], true);
        assert_eq!(host["modules"][1]["detail"], "no k3s");
        assert_eq!(listed.body["total"], 1);
        assert_eq!(
            listed.body["counts"],
            json!({"all": 1, "online": 1, "offline": 0, "failing": 1})
        );

        // The same machine registering again keeps its row and gets a new
        // secret; the old one stops working.
        let token = call(app, "POST", "/hosts/enrollments", as_user, None)
            .await
            .body["token"]
            .as_str()
            .unwrap()
            .to_string();
        let back = call(
            app,
            "POST",
            "/hosts/register",
            ("x-real-ip", "203.0.113.7"),
            Some(register_body(&token, "m1")),
        )
        .await;
        assert_eq!(back.body["id"], id.as_str());
        let old = call(
            app,
            "POST",
            "/hosts/heartbeat",
            ("authorization", &bearer),
            None,
        )
        .await;
        assert_eq!(old.status, StatusCode::UNAUTHORIZED);
        let bearer = format!("Bearer {}", back.body["secret"].as_str().unwrap());

        let deleted = call(app, "DELETE", &format!("/hosts/{id}"), as_user, None).await;
        assert_eq!(deleted.status, StatusCode::NO_CONTENT);
        let gone = call(
            app,
            "POST",
            "/hosts/heartbeat",
            ("authorization", &bearer),
            None,
        )
        .await;
        assert_eq!(gone.status, StatusCode::UNAUTHORIZED);
        let listed = call(app, "GET", "/hosts", as_user, None).await;
        assert_eq!(listed.body["hosts"], json!([]));
    }

    #[tokio::test]
    async fn hundreds_of_hosts_come_a_page_at_a_time() {
        let Some(t) = testing::app(|_| {}).await else {
            return;
        };
        let app = &t.app;
        let (user, _) = app.store.get_or_create_user("+14155552672").await.unwrap();
        let key = app.store.issue_api_key(&user, "test").await.unwrap();
        let as_user = ("x-api-key", key.as_str());

        for i in 0..120 {
            let token = app
                .store
                .issue_host_enrollment(&user, Utc::now() + ENROLLMENT_TTL)
                .await
                .unwrap();
            let name = format!("web-{i:03}");
            let host = NewHost {
                name: &name,
                machine_id: &name,
                version: "0.1.0",
                arch: "x86_64",
                os: if i % 2 == 0 { "Debian" } else { "Ubuntu" },
                ip: None,
            };
            let (_, secret) = app
                .store
                .register_host(&token, host)
                .await
                .unwrap()
                .unwrap();
            let modules = if i < 5 {
                json!([{"name": "mesh", "status": "error", "detail": "no key"}])
            } else {
                json!([{"name": "mesh", "status": "ok"}])
            };
            let beat = Heartbeat {
                version: "0.1.0",
                modules: &modules,
                uptime_seconds: Some(1),
                ip: None,
                workspaces: None,
            };
            app.store.host_heartbeat(&secret, beat).await.unwrap();
        }
        // Ten went quiet an hour ago.
        let pool = app.store.db().pool(&None).await.unwrap();
        sqlx::query(
            "UPDATE hosts SET last_seen_at = now() - interval '1 hour'
             WHERE name >= 'web-110'",
        )
        .execute(&pool)
        .await
        .unwrap();

        let page = call(app, "GET", "/hosts", as_user, None).await.body;
        assert_eq!(page["hosts"].as_array().unwrap().len(), PAGE as usize);
        assert_eq!(page["total"], 120);
        assert_eq!(page["hosts"][0]["name"], "web-000");
        assert_eq!(
            page["counts"],
            json!({"all": 120, "online": 110, "offline": 10, "failing": 5})
        );

        let last = call(app, "GET", "/hosts?offset=100&limit=50", as_user, None)
            .await
            .body;
        assert_eq!(last["hosts"].as_array().unwrap().len(), 20);
        assert_eq!(last["hosts"][0]["name"], "web-100");

        let quiet = call(app, "GET", "/hosts?state=offline", as_user, None)
            .await
            .body;
        assert_eq!(quiet["total"], 10);
        assert_eq!(quiet["hosts"][0]["online"], false);

        let failing = call(app, "GET", "/hosts?state=failing", as_user, None)
            .await
            .body;
        assert_eq!(failing["total"], 5);

        let found = call(app, "GET", "/hosts?q=WEB-01", as_user, None)
            .await
            .body;
        assert_eq!(found["total"], 10);
        assert_eq!(found["counts"]["all"], 10);
        let debian = call(app, "GET", "/hosts?q=debian&limit=1", as_user, None)
            .await
            .body;
        assert_eq!(debian["total"], 60);
        assert_eq!(debian["hosts"].as_array().unwrap().len(), 1);
        // LIKE's wildcards are only themselves.
        let none = call(app, "GET", "/hosts?q=%25", as_user, None).await.body;
        assert_eq!(none["total"], 0);

        let bad = call(app, "GET", "/hosts?state=sleepy", as_user, None).await;
        assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn the_client_ip_comes_from_the_ingress() {
        let mut headers = HeaderMap::new();
        assert_eq!(client_ip(&headers), None);
        headers.insert("x-real-ip", "198.51.100.2".parse().unwrap());
        assert_eq!(client_ip(&headers).as_deref(), Some("198.51.100.2"));
        headers.insert("x-forwarded-for", "2001:db8::1, 10.0.0.1".parse().unwrap());
        assert_eq!(client_ip(&headers).as_deref(), Some("2001:db8::1"));
        headers.insert("x-forwarded-for", "not an ip".parse().unwrap());
        assert_eq!(client_ip(&headers), None);
    }
}
