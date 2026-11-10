// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The HTTP surface: JSON API routers, the server-rendered web UI, static
//! files and the health check.

pub(crate) mod account;
mod auth;
mod hosts;
mod models;
mod oidc;
mod providers;
mod servers;
mod tasks;
mod workspaces;

use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};
use tower_http::cors::{AllowHeaders, AllowMethods, CorsLayer};

use crate::App;
use crate::config::Settings;
use crate::error::AppError;
use crate::models::TaskOut;
use crate::store::TaskRecord;
use crate::web;

pub fn router(app: Arc<App>) -> Router {
    let origins: Vec<HeaderValue> = allowed_origins(&app.settings)
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(origins)
        .allow_credentials(true)
        .allow_methods(AllowMethods::mirror_request())
        .allow_headers(AllowHeaders::mirror_request());

    Router::new()
        .route("/health", get(health))
        .route("/static/{*path}", get(static_file))
        .merge(account::router())
        .merge(auth::router())
        .merge(hosts::router())
        .merge(models::router())
        .merge(oidc::router())
        .merge(providers::router())
        .merge(servers::router())
        .merge(tasks::router())
        .merge(workspaces::router())
        .merge(web::router())
        .fallback(|| async { AppError::not_found("Not Found") })
        .method_not_allowed_fallback(|| async {
            AppError::new(StatusCode::METHOD_NOT_ALLOWED, "Method Not Allowed")
        })
        .layer(middleware::from_fn_with_state(app.clone(), same_site_only))
        .layer(cors)
        .with_state(app)
}

/// The origins whose pages may call the API with the browser's cookies:
/// the console's, and any in USECODE_AGENT_CORS_ORIGINS.
fn allowed_origins(settings: &Settings) -> Vec<String> {
    let mut origins = settings.cors_origins.clone();
    origins.push(settings.console_url.clone());
    origins
}

/// A browser posting from a page the API doesn't know is turned away, so
/// another site can't act with someone's cookies (CSRF): an unsafe request
/// with an `Origin` has to come from an allowed origin or the API's own
/// host. Requests without one (`uc`, scripts, Stripe) don't carry a
/// browser's cookies.
async fn same_site_only(State(app): State<Arc<App>>, request: Request, next: Next) -> Response {
    let safe = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok());
    if let (false, Some(origin)) = (safe, origin) {
        let host = request
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let own = origin.split_once("://").is_some_and(|(_, h)| h == host);
        if !own && !allowed_origins(&app.settings).iter().any(|o| o == origin) {
            return AppError::new(StatusCode::FORBIDDEN, "Not from a page we know.")
                .into_response();
        }
    }
    next.run(request).await
}

async fn health(State(app): State<Arc<App>>) -> Json<Value> {
    // Caddy load-balances over the nodes and health-checks this endpoint, so
    // name the node in the response — it's the simplest way to see which one
    // served a given request.
    Json(json!({"status": "ok", "node": app.settings.node_name}))
}

const STATIC_FILES: &[(&str, &str, &str)] = &[(
    "css/app.css",
    "text/css; charset=utf-8",
    include_str!("../../static/css/app.css"),
)];

async fn static_file(Path(path): Path<String>) -> Response {
    match STATIC_FILES.iter().find(|(name, _, _)| *name == path) {
        Some((_, content_type, body)) => {
            ([(header::CONTENT_TYPE, *content_type)], *body).into_response()
        }
        None => (StatusCode::NOT_FOUND, Json(json!({"detail": "Not Found"}))).into_response(),
    }
}

pub fn task_out(task: TaskRecord) -> TaskOut {
    TaskOut {
        id: task.id.to_string(),
        kind: task.kind,
        assignee: task.assignee,
        state: task.state,
        resources: task.resources,
        error: task.error,
        created_at: task.created_at,
        updated_at: task.updated_at,
    }
}
