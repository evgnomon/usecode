// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! /models: the AI model (llama-server) container on this API host.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Map, Value};

use crate::App;
use crate::error::{AppError, AppResult};
use crate::extract::{Client, JsonBody};
use crate::model_config::known_options;
use crate::model_process::{self, ModelError};
use crate::models::{ModelOptionsOut, ModelStatusOut};

pub fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/models/options", get(options))
        .route("/models/status", get(status))
        .route("/models/start", post(start))
        .route("/models/stop", post(stop))
}

fn model_error(err: ModelError) -> AppError {
    match err {
        ModelError::Invalid(detail) => AppError::bad_request(detail),
        process => AppError::bad_gateway(process.to_string()),
    }
}

async fn options(_: Client) -> Json<ModelOptionsOut> {
    Json(ModelOptionsOut {
        fields: known_options(),
    })
}

async fn status(State(app): State<Arc<App>>, _: Client) -> Json<ModelStatusOut> {
    let (running, state) = model_process::status(&app.settings).await;
    Json(ModelStatusOut {
        running,
        config: None,
        state,
    })
}

async fn start(
    State(app): State<Arc<App>>,
    _: Client,
    JsonBody(overrides): JsonBody<Map<String, Value>>,
) -> AppResult<Json<ModelStatusOut>> {
    let config = model_process::start(&app.settings, &overrides)
        .await
        .map_err(model_error)?;
    Ok(Json(ModelStatusOut {
        running: true,
        config: Some(config),
        state: None,
    }))
}

async fn stop(State(app): State<Arc<App>>, _: Client) -> AppResult<StatusCode> {
    model_process::stop(&app.settings)
        .await
        .map_err(model_error)?;
    Ok(StatusCode::NO_CONTENT)
}
