// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! /workspaces: the workspaces (`uc ws`) of your clusters, and a shell in
//! one from the browser.
//!
//! The usecode daemon on a Kubernetes controller reports its cluster's
//! workspaces, and keeps a WebSocket open here (`GET /hosts/connect`) that
//! the shells ride; see [`crate::relay`] for what goes over it.
//!
//! - `GET /workspaces`: every workspace, once per cluster, with the host
//!   that carries its shells.
//! - `POST /workspaces/{host}/{name}/shell`: a ticket for one shell.
//! - `GET /workspaces/shell?ticket=...`: the shell, a WebSocket speaking
//!   restty's PTY protocol.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite;
use uuid::Uuid;

use crate::App;
use crate::error::{AppError, AppResult};
use crate::extract::Client;
use crate::models::{ShellTicketOut, WorkspaceIn, WorkspaceListOut, WorkspaceOut};
use crate::relay::{Down, HostLink, Ticket};
use crate::routes::hosts::{bearer, clip, online_window};
use crate::store::{HostRow, ts};

/// The most workspaces one host may report.
const MAX_WORKSPACES: usize = 256;

/// How often each side of a relay socket is pinged, so proxies keep it
/// open and a dead one is noticed.
const PING: Duration = Duration::from_secs(30);

/// Output frames a shell may have waiting for a slow browser before it is
/// cut off, so one stuck page can't hold up the others on its host.
const BACKLOG: usize = 1024;

/// How long a page has to say its size before the shell opens at 80x24.
const FIRST_SIZE: Duration = Duration::from_secs(3);

/// Set on a shell one instance hands to another, so it is handed on once.
const RELAYED: &str = "x-uc-relayed";

pub fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/workspaces", get(list))
        .route("/workspaces/shell", get(shell))
        .route("/workspaces/{host_id}/{name}/shell", post(ticket))
        .route("/hosts/connect", get(connect))
}

/// A workspace name `uc ws` takes: a DNS label once `ws-` is in front.
fn valid_name(name: &str) -> bool {
    (1..=60).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

/// What a host reported, cleaned up as it is kept: sane fields, sorted by
/// name, the ones with a name `uc ws` couldn't have made left out.
pub fn reported(list: Vec<WorkspaceIn>) -> AppResult<Value> {
    let mut kept: Vec<WorkspaceIn> = list
        .into_iter()
        .take(MAX_WORKSPACES)
        .filter_map(|w| {
            let name = clip(&w.name, 60);
            valid_name(&name).then(|| WorkspaceIn {
                name,
                state: match w.state.as_str() {
                    s @ ("running" | "starting" | "stopped") => s.to_string(),
                    _ => "unknown".to_string(),
                },
                user: clip(&w.user, 32),
                image: clip(&w.image, 256),
            })
        })
        .collect();
    kept.sort_by(|a, b| a.name.cmp(&b.name));
    kept.dedup_by(|a, b| a.name == b.name);
    Ok(serde_json::to_value(kept).map_err(anyhow::Error::from)?)
}

fn workspaces_of(row: &HostRow) -> Vec<WorkspaceIn> {
    serde_json::from_value(row.workspaces.0.clone()).unwrap_or_default()
}

fn online(row: &HostRow, now: DateTime<Utc>) -> bool {
    now - row.last_seen_at < online_window()
}

async fn list(
    State(app): State<Arc<App>>,
    Client(client): Client,
) -> AppResult<Json<WorkspaceListOut>> {
    let rows = app
        .store
        .hosts_with_workspaces(&client.partition, &client.user_id)
        .await?;
    Ok(Json(WorkspaceListOut {
        workspaces: listed(rows, Utc::now()),
    }))
}

/// Each workspace once per cluster, every controller of which reports it:
/// from the host best placed to carry a shell (connected, then online,
/// then the one heard from last).
fn listed(rows: Vec<HostRow>, now: DateTime<Utc>) -> Vec<WorkspaceOut> {
    let mut best: BTreeMap<(String, String), WorkspaceOut> = BTreeMap::new();
    for row in rows {
        let online = online(&row, now);
        let connected = online && row.relay_url.is_some();
        let cluster = match row.cluster_id.as_str() {
            "" => row.id.to_string(),
            id => id.to_string(),
        };
        for w in workspaces_of(&row) {
            let out = WorkspaceOut {
                name: w.name,
                state: w.state,
                user: w.user,
                image: w.image,
                host_id: row.id.to_string(),
                host_name: row.name.clone(),
                cluster_id: row.cluster_id.clone(),
                online,
                connected,
                last_seen_at: ts(row.last_seen_at),
            };
            let rank = |w: &WorkspaceOut| (w.connected, w.online);
            let key = (cluster.clone(), out.name.clone());
            let better = best.get(&key).is_none_or(|have| {
                rank(&out) > rank(have)
                    || (rank(&out) == rank(have) && out.last_seen_at > have.last_seen_at)
            });
            if better {
                best.insert(key, out);
            }
        }
    }
    let mut all: Vec<WorkspaceOut> = best.into_values().collect();
    all.sort_by(|a, b| (&a.name, &a.host_name).cmp(&(&b.name, &b.host_name)));
    all
}

async fn ticket(
    State(app): State<Arc<App>>,
    Client(client): Client,
    Path((host_id, name)): Path<(String, String)>,
) -> AppResult<Json<ShellTicketOut>> {
    let not_found = || AppError::not_found("No such workspace on that host.");
    let host_id = Uuid::parse_str(&host_id).map_err(|_| not_found())?;
    let host = app
        .store
        .get_host(&client.user_id, &host_id)
        .await?
        .ok_or_else(not_found)?;
    let workspace = workspaces_of(&host)
        .into_iter()
        .find(|w| w.name == name)
        .ok_or_else(not_found)?;
    if workspace.state == "stopped" {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            format!("{name} is stopped; `uc ws start {name}` starts it."),
        ));
    }
    if !online(&host, Utc::now()) || host.relay_url.is_none() {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            format!(
                "{} isn't connected for shells just now. It connects by itself while \
                 its usecode daemon runs and is new enough to; `uc daemon install {}` \
                 brings it up to date.",
                host.name, host.name
            ),
        ));
    }
    let ticket = Ticket::new(client.user_id, host_id, &name);
    Ok(Json(ShellTicketOut {
        ticket: ticket.encode(&app.settings.secret_key),
        expires_at: ticket.expires_at as f64,
    }))
}

#[derive(Deserialize)]
struct ShellQuery {
    #[serde(default)]
    ticket: String,
}

/// The browser's shell: served here when the host's relay socket is here,
/// else handed to the instance it is on.
async fn shell(
    State(app): State<Arc<App>>,
    Query(query): Query<ShellQuery>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let Some(ticket) = Ticket::decode(&query.ticket, &app.settings.secret_key) else {
        return AppError::unauthorized(
            "That shell ticket is unknown or ran out; open the shell again.",
        )
        .into_response();
    };
    if let Some(link) = app.relay.link(&ticket.host_id) {
        if link.user_id != ticket.user_id {
            return AppError::not_found("No such workspace on that host.").into_response();
        }
        if !app.relay.use_ticket(&ticket) {
            return AppError::unauthorized("That shell ticket was used; open the shell again.")
                .into_response();
        }
        let session = app.relay.next_session();
        return upgrade.on_upgrade(move |ws| serve_shell(link, session, ticket, ws));
    }

    let gone = || {
        AppError::bad_gateway(
            "The host isn't connected just now; it connects again by itself within a minute \
             while its usecode daemon runs.",
        )
        .into_response()
    };
    if headers.contains_key(RELAYED) {
        return gone();
    }
    let host = match app.store.get_host(&ticket.user_id, &ticket.host_id).await {
        Ok(Some(host)) => host,
        Ok(None) => return AppError::not_found("No such workspace on that host.").into_response(),
        Err(err) => return AppError::from(err).into_response(),
    };
    let Some(url) = host.relay_url.filter(|url| *url != app.relay.internal_url) else {
        return gone();
    };
    match hand_on(&url, &query.ticket).await {
        Ok(upstream) => upgrade.on_upgrade(move |ws| pipe(ws, upstream)),
        Err(tungstenite::Error::Http(answer)) => {
            let status =
                StatusCode::from_u16(answer.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let body = answer.into_body().unwrap_or_default();
            (
                status,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                body,
            )
                .into_response()
        }
        Err(err) => {
            tracing::warn!(%url, "handing a shell on: {err}");
            gone()
        }
    }
}

type Upstream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// The same shell, opened on the instance at `url`.
async fn hand_on(url: &str, ticket: &str) -> Result<Upstream, tungstenite::Error> {
    use tungstenite::client::IntoClientRequest;
    let ws_url = match url.split_once("://") {
        Some(("https", rest)) => format!("wss://{rest}"),
        Some((_, rest)) => format!("ws://{rest}"),
        None => format!("ws://{url}"),
    };
    let mut request = format!("{ws_url}/workspaces/shell?ticket={ticket}").into_client_request()?;
    request
        .headers_mut()
        .insert(RELAYED, axum::http::HeaderValue::from_static("1"));
    let connecting = tokio_tungstenite::connect_async(request);
    let (upstream, _) = tokio::time::timeout(Duration::from_secs(10), connecting)
        .await
        .map_err(|_| tungstenite::Error::Io(std::io::ErrorKind::TimedOut.into()))??;
    Ok(upstream)
}

/// The browser's socket and another instance's, joined.
async fn pipe(ws: WebSocket, upstream: Upstream) {
    let (mut to_browser, mut from_browser) = ws.split();
    let (mut to_upstream, mut from_upstream) = upstream.split();
    let down = async {
        while let Some(Ok(message)) = from_upstream.next().await {
            let message = match message {
                tungstenite::Message::Text(text) => Message::Text(text.as_str().into()),
                tungstenite::Message::Binary(bytes) => Message::Binary(bytes),
                tungstenite::Message::Close(_) => break,
                _ => continue,
            };
            if to_browser.send(message).await.is_err() {
                break;
            }
        }
        let _ = to_browser.close().await;
    };
    let up = async {
        while let Some(Ok(message)) = from_browser.next().await {
            let message = match message {
                Message::Text(text) => tungstenite::Message::Text(text.as_str().into()),
                Message::Binary(bytes) => tungstenite::Message::Binary(bytes),
                Message::Close(_) => break,
                _ => continue,
            };
            if to_upstream.send(message).await.is_err() {
                break;
            }
        }
        let _ = to_upstream.close().await;
    };
    tokio::select! {
        () = down => {},
        () = up => {},
    }
}

/// What a page's frame asks of its shell, in restty's PTY protocol.
enum FromPage {
    Input(String),
    Resize(u16, u16),
}

fn from_page(message: Message) -> Option<FromPage> {
    let text = match message {
        Message::Text(text) => text,
        Message::Binary(bytes) => {
            return Some(FromPage::Input(
                String::from_utf8_lossy(&bytes).into_owned(),
            ));
        }
        _ => return None,
    };
    let value: Value = serde_json::from_str(text.as_str()).ok()?;
    match value["type"].as_str()? {
        "input" => Some(FromPage::Input(value["data"].as_str()?.to_string())),
        "resize" => {
            let dim = |k: &str| value[k].as_u64().map(|n| n.clamp(1, 1000) as u16);
            Some(FromPage::Resize(dim("cols")?, dim("rows")?))
        }
        _ => None,
    }
}

/// A server message restty shows: `{"type": "error", "message": ...}`.
fn page_error(message: &str) -> Message {
    Message::Text(
        json!({"type": "error", "message": message})
            .to_string()
            .into(),
    )
}

/// One shell, between the page and the host's relay socket here.
async fn serve_shell(link: Arc<HostLink>, session: u32, ticket: Ticket, ws: WebSocket) {
    let (mut sink, mut stream) = ws.split();

    // restty says the page's size as soon as it is connected.
    let (mut cols, mut rows) = (80, 24);
    let first = tokio::time::timeout(FIRST_SIZE, async {
        while let Some(Ok(message)) = stream.next().await {
            if let Some(FromPage::Resize(c, r)) = from_page(message) {
                return Some((c, r));
            }
        }
        None
    });
    match first.await {
        Ok(Some(size)) => (cols, rows) = size,
        Ok(None) => return,
        Err(_) => {}
    }

    let (tx, mut rx) = mpsc::channel::<Down>(BACKLOG);
    if !link.add_session(session, tx) {
        let why = format!(
            "This host carries {} shells already; close one first.",
            crate::relay::MAX_SESSIONS
        );
        let _ = sink.send(page_error(&why)).await;
        return;
    }
    link.send(json!({
        "type": "open", "session": session, "workspace": ticket.workspace,
        "cols": cols, "rows": rows,
    }));

    let mut page_left = false;
    loop {
        tokio::select! {
            down = rx.recv() => match down {
                Some(Down::Output(bytes)) => {
                    if sink.send(Message::Binary(bytes)).await.is_err() {
                        page_left = true;
                        break;
                    }
                }
                Some(Down::Exited(code)) => {
                    let exit = json!({"type": "exit", "code": code}).to_string();
                    let _ = sink.send(Message::Text(exit.into())).await;
                    break;
                }
                Some(Down::Closed(why)) => {
                    let _ = sink.send(page_error(&why)).await;
                    break;
                }
                // Cut off: the page didn't keep up.
                None => {
                    let _ = sink.send(page_error("The page fell too far behind the shell.")).await;
                    page_left = true;
                    break;
                }
            },
            message = stream.next() => match message {
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => {
                    page_left = true;
                    break;
                }
                Some(Ok(message)) => match from_page(message) {
                    Some(FromPage::Input(data)) => {
                        link.send(json!({"type": "input", "session": session, "data": data}));
                    }
                    Some(FromPage::Resize(c, r)) if (c, r) != (cols, rows) => {
                        (cols, rows) = (c, r);
                        link.send(json!({
                            "type": "resize", "session": session, "cols": cols, "rows": rows,
                        }));
                    }
                    _ => {}
                },
            },
        }
    }
    if link.end_session(session).is_some() || page_left {
        link.send(json!({"type": "close", "session": session}));
    }
    let _ = sink.close().await;
}

/// The daemon's relay socket: `Authorization: Bearer <host secret>`.
async fn connect(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let Some(secret) = bearer(&headers) else {
        return AppError::unauthorized("Missing host secret").into_response();
    };
    let host = match app.store.host_by_secret(&secret).await {
        Ok(Some(host)) => host,
        Ok(None) => return AppError::unauthorized("This host is not registered").into_response(),
        Err(err) => return AppError::from(err).into_response(),
    };
    upgrade.on_upgrade(move |ws| relay_host(app, host, ws))
}

async fn relay_host(app: Arc<App>, host: HostRow, ws: WebSocket) {
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let link = app.relay.attach(host.id, host.user_id, tx);
    let here = app.relay.internal_url.clone();
    if let Err(err) = app
        .store
        .set_host_relay(&host.user_id, &host.id, Some(&here), None)
        .await
    {
        tracing::error!(host = %host.id, "saying where the relay is: {err:#}");
    }
    tracing::info!(host = %host.id, name = %host.name, "relay connected");

    let (mut sink, mut stream) = ws.split();
    let mut ping = tokio::time::interval(PING);
    ping.tick().await;
    let mut heard = Instant::now();
    loop {
        tokio::select! {
            out = rx.recv() => {
                let Some(text) = out else { break };
                if sink.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            _ = ping.tick() => {
                if heard.elapsed() > PING * 3 {
                    tracing::info!(host = %host.id, "relay went quiet");
                    break;
                }
                if sink.send(Message::Ping(Bytes::new())).await.is_err() {
                    break;
                }
            }
            message = stream.next() => {
                heard = Instant::now();
                match message {
                    Some(Ok(Message::Binary(frame))) => output(&link, frame),
                    Some(Ok(Message::Text(text))) => from_host(&app, &host, &link, text.as_str()).await,
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                    Some(Ok(_)) => {}
                }
            }
        }
    }

    if app.relay.detach(&host.id, &link)
        && let Err(err) = app
            .store
            .set_host_relay(&host.user_id, &host.id, None, Some(&here))
            .await
    {
        tracing::error!(host = %host.id, "forgetting the relay: {err:#}");
    }
    tracing::info!(host = %host.id, name = %host.name, "relay disconnected");
}

/// A binary frame from the daemon: a session's output.
fn output(link: &HostLink, frame: Bytes) {
    if frame.len() < 4 {
        return;
    }
    let session = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]);
    let Some(tx) = link.session(session) else {
        return;
    };
    if tx.try_send(Down::Output(frame.slice(4..))).is_err() {
        // The page is that far behind, or gone: let go of it, which ends it.
        link.end_session(session);
        link.send(json!({"type": "close", "session": session}));
    }
}

/// A text frame from the daemon.
async fn from_host(app: &App, host: &HostRow, link: &HostLink, text: &str) {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return;
    };
    match value["type"].as_str() {
        Some("closed") => {
            let Some(session) = value["session"].as_u64().map(|n| n as u32) else {
                return;
            };
            if let Some(tx) = link.end_session(session) {
                let down = match value["reason"].as_str().filter(|r| !r.is_empty()) {
                    Some(reason) => Down::Closed(clip(reason, 512)),
                    None => Down::Exited(value["code"].as_i64().unwrap_or(0) as i32),
                };
                let _ = tx.try_send(down);
            }
        }
        Some("workspaces") => {
            let list: Vec<WorkspaceIn> =
                serde_json::from_value(value["workspaces"].clone()).unwrap_or_default();
            let cluster_id = clip(value["cluster_id"].as_str().unwrap_or_default(), 64);
            let Ok(list) = reported(list) else { return };
            if let Err(err) = app
                .store
                .set_host_workspaces(&host.user_id, &host.id, &list, &cluster_id)
                .await
            {
                tracing::error!(host = %host.id, "keeping the workspaces: {err:#}");
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::types::Json as SqlJson;

    #[test]
    fn workspace_names_are_dns_labels() {
        assert!(valid_name("dev"));
        assert!(valid_name("ada-2"));
        assert!(!valid_name(""));
        assert!(!valid_name("-dev"));
        assert!(!valid_name("Dev"));
        assert!(!valid_name("a.b"));
        assert!(!valid_name(&"a".repeat(61)));
    }

    #[test]
    fn what_a_host_reports_is_cleaned_up() {
        let w = |name: &str, state: &str| WorkspaceIn {
            name: name.into(),
            state: state.into(),
            user: "ada".into(),
            image: "base".into(),
        };
        let kept = reported(vec![
            w("web", "running"),
            w("dev", "sleepy"),
            w("../etc", "running"),
            w("web", "stopped"),
        ])
        .unwrap();
        assert_eq!(
            kept,
            json!([
                {"name": "dev", "state": "unknown", "user": "ada", "image": "base"},
                {"name": "web", "state": "running", "user": "ada", "image": "base"},
            ])
        );
    }

    fn row(name: &str, cluster: &str, relay: bool, seen: DateTime<Utc>) -> HostRow {
        HostRow {
            id: Uuid::new_v4(),
            user_id: Uuid::nil(),
            name: name.into(),
            version: String::new(),
            arch: String::new(),
            os: String::new(),
            modules: SqlJson(json!([])),
            uptime_seconds: None,
            last_ip: None,
            last_seen_at: seen,
            created_at: seen,
            workspaces: SqlJson(
                json!([{"name": "dev", "state": "running", "user": "ada", "image": "i"}]),
            ),
            cluster_id: cluster.into(),
            relay_url: relay.then(|| "http://10.0.0.1:8000".to_string()),
        }
    }

    /// Two instances: the daemon's relay socket on one, the page on the
    /// other, which hands the shell on.
    #[tokio::test]
    async fn a_shell_rides_the_hosts_relay_socket_from_any_instance() {
        use axum::http::{Request, header};
        use tokio_tungstenite::connect_async;
        use tungstenite::Message as W;
        use tungstenite::client::IntoClientRequest;

        use crate::store::{Heartbeat, NewHost};
        use crate::testing::{self, Answer};

        let Some(t) = testing::app(|_| {}).await else {
            return;
        };
        let listen = || tokio::net::TcpListener::bind("127.0.0.1:0");
        let (la, lb) = (listen().await.unwrap(), listen().await.unwrap());
        let (a_addr, b_addr) = (la.local_addr().unwrap(), lb.local_addr().unwrap());
        let a = t.sibling(&format!("http://{a_addr}"));
        let b = t.sibling(&format!("http://{b_addr}"));
        for (listener, app) in [(la, a.clone()), (lb, b.clone())] {
            let service = crate::routes::router(app);
            tokio::spawn(async move { axum::serve(listener, service).await });
        }

        let (user, _) = b.store.get_or_create_user("+14155552673").await.unwrap();
        let key = b.store.issue_api_key(&user, "test").await.unwrap();
        let token = b
            .store
            .issue_host_enrollment(&user, Utc::now() + chrono::Duration::hours(1))
            .await
            .unwrap();
        let new = NewHost {
            name: "ctl-1",
            machine_id: "m1",
            version: "0.1.0",
            arch: "x86_64",
            os: "Debian",
            ip: None,
        };
        let (host, secret) = b.store.register_host(&token, new).await.unwrap().unwrap();
        let reported = json!([{"name": "dev", "state": "running", "user": "ada", "image": "base"}]);
        let beat = Heartbeat {
            version: "0.1.0",
            modules: &json!([]),
            uptime_seconds: None,
            ip: None,
            workspaces: Some((&reported, "k1")),
        };
        b.store.host_heartbeat(&secret, beat).await.unwrap();

        let call = |method: &str, path: String| {
            let request = Request::builder()
                .method(method)
                .uri(path)
                .header("x-api-key", &key)
                .body(axum::body::Body::empty())
                .unwrap();
            let b = b.clone();
            async move { testing::request_app(&b, request).await }
        };
        let ticket_path = format!("/workspaces/{}/dev/shell", host.id);

        // Not connected yet: no ticket.
        let early: Answer = call("POST", ticket_path.clone()).await;
        assert_eq!(early.status, StatusCode::CONFLICT, "{}", early.body);

        // The daemon connects to a.
        let mut request = format!("ws://{a_addr}/hosts/connect")
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            header::AUTHORIZATION,
            format!("Bearer {secret}").parse().unwrap(),
        );
        let (mut daemon, _) = connect_async(request).await.unwrap();
        for _ in 0..50 {
            let row = b.store.get_host(&user, &host.id).await.unwrap().unwrap();
            if row.relay_url.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let listed = call("GET", "/workspaces".into()).await;
        assert_eq!(listed.body["workspaces"][0]["name"], "dev");
        assert_eq!(listed.body["workspaces"][0]["connected"], true);
        assert_eq!(listed.body["workspaces"][0]["host_name"], "ctl-1");

        let issued = call("POST", ticket_path.clone()).await;
        assert_eq!(issued.status, StatusCode::OK, "{}", issued.body);
        let ticket = issued.body["ticket"].as_str().unwrap().to_string();

        // The page opens the shell on b, which hands it to a.
        let shell_url = format!("ws://{b_addr}/workspaces/shell?ticket={ticket}");
        let (mut page, _) = connect_async(&shell_url).await.unwrap();
        let resize = json!({"type": "resize", "cols": 100, "rows": 30}).to_string();
        page.send(W::Text(resize.into())).await.unwrap();

        let next_text = |m: Option<Result<W, tungstenite::Error>>| -> Value {
            match m {
                Some(Ok(W::Text(text))) => serde_json::from_str(text.as_str()).unwrap(),
                other => panic!("wanted text, got {other:?}"),
            }
        };
        let open = next_text(daemon.next().await);
        assert_eq!(open["type"], "open");
        assert_eq!(open["workspace"], "dev");
        assert_eq!(
            (open["cols"].as_u64(), open["rows"].as_u64()),
            (Some(100), Some(30))
        );
        let session = open["session"].as_u64().unwrap() as u32;

        let mut frame = session.to_be_bytes().to_vec();
        frame.extend_from_slice(b"hello\r\n");
        daemon.send(W::Binary(frame.into())).await.unwrap();
        match page.next().await {
            Some(Ok(W::Binary(bytes))) => assert_eq!(&bytes[..], b"hello\r\n"),
            other => panic!("wanted the output, got {other:?}"),
        }

        let input = json!({"type": "input", "data": "ls\r"}).to_string();
        page.send(W::Text(input.into())).await.unwrap();
        let typed = next_text(daemon.next().await);
        assert_eq!(
            typed,
            json!({"type": "input", "session": session, "data": "ls\r"})
        );

        let closed = json!({"type": "closed", "session": session, "code": 3}).to_string();
        daemon.send(W::Text(closed.into())).await.unwrap();
        assert_eq!(
            next_text(page.next().await),
            json!({"type": "exit", "code": 3})
        );

        // A ticket is good once, wherever it is used.
        match connect_async(&shell_url).await {
            Err(tungstenite::Error::Http(answer)) => {
                assert_eq!(answer.status(), StatusCode::UNAUTHORIZED)
            }
            other => panic!("a used ticket opened a shell: {:?}", other.map(|_| ())),
        }

        // The daemon goes; the host isn't connected any more.
        daemon.close(None).await.unwrap();
        for _ in 0..50 {
            let row = b.store.get_host(&user, &host.id).await.unwrap().unwrap();
            if row.relay_url.is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let listed = call("GET", "/workspaces".into()).await;
        assert_eq!(listed.body["workspaces"][0]["connected"], false);
    }

    #[test]
    fn a_cluster_lists_each_workspace_once_from_its_best_host() {
        let now = Utc::now();
        let listed = listed(
            vec![
                row("c1", "k1", false, now),
                row("c2", "k1", true, now - chrono::Duration::seconds(60)),
                row("c3", "k1", true, now - chrono::Duration::hours(2)),
                row("solo", "", false, now),
            ],
            now,
        );
        let got: Vec<(&str, &str, bool)> = listed
            .iter()
            .map(|w| (w.name.as_str(), w.host_name.as_str(), w.connected))
            .collect();
        assert_eq!(got, [("dev", "c2", true), ("dev", "solo", false)]);
    }
}
