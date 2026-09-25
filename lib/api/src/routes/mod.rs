// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The HTTP surface: JSON API routers, the server-rendered web UI, static
//! files and the health check.

mod auth;
mod models;
mod providers;
mod servers;
mod tasks;

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};
use tower_http::cors::{AllowHeaders, AllowMethods, CorsLayer};

use crate::App;
use crate::error::AppError;
use crate::models::TaskOut;
use crate::store::TaskRecord;
use crate::web;

pub fn router(app: Arc<App>) -> Router {
    let origins: Vec<HeaderValue> = app
        .settings
        .cors_origins
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
        .merge(auth::router())
        .merge(models::router())
        .merge(providers::router())
        .merge(servers::router())
        .merge(tasks::router())
        .merge(web::router())
        .fallback(|| async { AppError::not_found("Not Found") })
        .method_not_allowed_fallback(|| async {
            AppError::new(StatusCode::METHOD_NOT_ALLOWED, "Method Not Allowed")
        })
        .layer(cors)
        .with_state(app)
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
