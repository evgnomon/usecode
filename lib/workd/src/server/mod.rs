// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! HTTP API: run processes and manage their outputs.

mod runner;
mod store;
pub mod tls;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub use store::RunInfo;
use store::{Filter, RunListItem, list_items, plan_clear};

use crate::util::{now_iso, sanitize_path_component};

pub type Db = Arc<Mutex<Vec<RunInfo>>>;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub base_dir: PathBuf,
}

#[derive(Deserialize)]
struct RunRequest {
    tenant_name: String,
    namespace: String,
    job_name: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    working_dir: Option<String>,
}

#[derive(Serialize)]
struct RunResponse {
    run_id: String,
    tenant_name: String,
    namespace: String,
    job_name: String,
    status: String,
    output_dir: String,
    created_at: String,
}

#[derive(Serialize)]
struct RunStatus {
    run_id: String,
    tenant_name: String,
    namespace: String,
    job_name: String,
    status: String,
    exit_code: Option<i32>,
    started_at: String,
    finished_at: Option<String>,
    output_dir: String,
}

/// FastAPI-style error: `{"detail": ...}` with a status code.
struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "detail": self.1 }))).into_response()
    }
}

fn not_found() -> ApiError {
    ApiError(StatusCode::NOT_FOUND, "Run not found".into())
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/runs", post(create_run).get(list_all_runs))
        .route(
            "/tenants/{tenant_name}/namespaces/{namespace}/jobs/{job_name}/runs",
            get(list_job_runs),
        )
        .route("/runs/{run_id}", get(get_run_status).delete(delete_run))
        .route("/runs/{run_id}/stdout", get(get_stdout))
        .route("/runs/{run_id}/stderr", get(get_stderr))
        .route("/clear", post(clear_old_runs))
        .route("/health", get(health_check))
        .fallback(|| async { ApiError(StatusCode::NOT_FOUND, "Not Found".into()) })
        .with_state(state)
}

fn lookup(state: &AppState, run_id: &str) -> Result<RunInfo, ApiError> {
    let runs = state.db.lock().expect("runs lock");
    runs.iter()
        .find(|r| r.run_id == run_id)
        .cloned()
        .ok_or_else(not_found)
}

async fn create_run(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<RunResponse>), ApiError> {
    let request: RunRequest = serde_json::from_slice(&body)
        .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;
    let tenant = sanitize_path_component(&request.tenant_name);
    let namespace = sanitize_path_component(&request.namespace);
    let job_name = sanitize_path_component(&request.job_name);
    let run_id = uuid::Uuid::new_v4().to_string();
    let output_dir = state
        .base_dir
        .join(&tenant)
        .join(&namespace)
        .join(&job_name)
        .join(&run_id);
    let output_dir_str = output_dir.to_string_lossy().to_string();
    let created_at = now_iso();

    state.db.lock().expect("runs lock").push(RunInfo {
        run_id: run_id.clone(),
        tenant_name: tenant.clone(),
        namespace: namespace.clone(),
        job_name: job_name.clone(),
        status: "pending".into(),
        exit_code: None,
        created_at: created_at.clone(),
        started_at: None,
        finished_at: None,
        output_dir: output_dir_str.clone(),
        error: None,
    });

    tokio::spawn(runner::execute(
        state.db.clone(),
        runner::Job {
            run_id: run_id.clone(),
            tenant_name: tenant.clone(),
            namespace: namespace.clone(),
            job_name: job_name.clone(),
            command: request.command,
            args: request.args,
            output_dir,
            env: request.env,
            working_dir: request.working_dir,
        },
    ));

    Ok((
        StatusCode::ACCEPTED,
        Json(RunResponse {
            run_id,
            tenant_name: tenant,
            namespace,
            job_name,
            status: "pending".into(),
            output_dir: output_dir_str,
            created_at,
        }),
    ))
}

async fn list_all_runs(
    State(state): State<AppState>,
    Query(filter): Query<Filter>,
) -> Json<Vec<RunListItem>> {
    let runs = state.db.lock().expect("runs lock");
    Json(list_items(&runs, |r| filter.matches(r)))
}

async fn list_job_runs(
    State(state): State<AppState>,
    Path((tenant_name, namespace, job_name)): Path<(String, String, String)>,
) -> Json<Vec<RunListItem>> {
    let tenant = sanitize_path_component(&tenant_name);
    let ns = sanitize_path_component(&namespace);
    let job = sanitize_path_component(&job_name);
    let runs = state.db.lock().expect("runs lock");
    Json(list_items(&runs, |r| {
        r.tenant_name == tenant && r.namespace == ns && r.job_name == job
    }))
}

async fn get_run_status(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<RunStatus>, ApiError> {
    let r = lookup(&state, &run_id)?;
    Ok(Json(RunStatus {
        run_id,
        tenant_name: r.tenant_name,
        namespace: r.namespace,
        job_name: r.job_name,
        status: r.status,
        exit_code: r.exit_code,
        started_at: r.started_at.unwrap_or(r.created_at),
        finished_at: r.finished_at,
        output_dir: r.output_dir,
    }))
}

fn read_log(state: &AppState, run_id: &str, name: &str) -> Result<String, ApiError> {
    let r = lookup(state, run_id)?;
    let path = PathBuf::from(&r.output_dir).join(name);
    match (path.exists(), r.status.as_str()) {
        (true, _) => std::fs::read(&path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .map_err(internal),
        (false, "pending") => Ok("Process has not started yet".into()),
        (false, _) => Err(ApiError(StatusCode::NOT_FOUND, format!("{name} not found"))),
    }
}

async fn get_stdout(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<String, ApiError> {
    read_log(&state, &run_id, "stdout.log")
}

async fn get_stderr(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<String, ApiError> {
    read_log(&state, &run_id, "stderr.log")
}

fn remove_dir(dir: &str) -> Result<(), ApiError> {
    let path = PathBuf::from(dir);
    match path.exists() {
        true => std::fs::remove_dir_all(&path).map_err(internal),
        false => Ok(()),
    }
}

async fn delete_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let r = lookup(&state, &run_id)?;
    if r.status == "running" {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "Cannot delete a running process".into(),
        ));
    }
    remove_dir(&r.output_dir)?;
    state
        .db
        .lock()
        .expect("runs lock")
        .retain(|x| x.run_id != run_id);
    Ok(Json(json!({ "message": "Run deleted", "run_id": run_id })))
}

async fn clear_old_runs(
    State(state): State<AppState>,
    Query(filter): Query<Filter>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut runs = state.db.lock().expect("runs lock");
    let plan = plan_clear(&runs, &filter);
    let mut deleted = Vec::new();
    for (rid, dir) in plan.deleted {
        remove_dir(&dir)?;
        runs.retain(|x| x.run_id != rid);
        deleted.push(rid);
    }
    Ok(Json(json!({ "deleted": deleted, "skipped": plan.skipped })))
}

async fn health_check(State(state): State<AppState>) -> Json<serde_json::Value> {
    let n = state.db.lock().expect("runs lock").len();
    Json(json!({ "status": "healthy", "active_runs": n }))
}
