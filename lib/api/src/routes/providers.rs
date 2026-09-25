// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! /providers: the caller's cloud provider credentials, encrypted at rest.

use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde_json::Value;

use crate::App;
use crate::error::{AppError, AppResult};
use crate::extract::{Client, JsonBody};
use crate::models::{
    ProviderCredentialsIn, ProviderCredentialsListOut, ProviderCredentialsStatusOut,
};
use crate::providers::{PROVIDERS, Provider, ProviderError};

pub fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/providers/credentials", get(list_credentials_status))
        .route(
            "/providers/{provider}/credentials",
            put(set_credentials)
                .get(get_credentials_status)
                .delete(delete_credentials),
        )
}

/// A known provider, or the 404 every provider-scoped endpoint answers with.
pub fn known_provider(name: &str) -> AppResult<Provider> {
    Provider::parse(name).map_err(|err| AppError::not_found(err.to_string()))
}

/// The status and message a provider failure is reported to callers with.
pub fn provider_error(err: ProviderError) -> AppError {
    match err {
        ProviderError::InvalidCredentials { .. } => AppError::bad_request(err.to_string()),
        ProviderError::Unknown(_) => AppError::not_found(err.to_string()),
        _ => AppError::bad_gateway(err.to_string()),
    }
}

async fn set_credentials(
    State(app): State<Arc<App>>,
    Client(client): Client,
    Path(provider): Path<String>,
    JsonBody(payload): JsonBody<ProviderCredentialsIn>,
) -> AppResult<Json<ProviderCredentialsStatusOut>> {
    let provider = known_provider(&provider)?;
    let payload = payload.validated()?;
    let credentials = Value::Object(payload.credentials);
    provider
        .validate_credentials(&credentials)
        .map_err(provider_error)?;
    let encrypted = app.cipher.encrypt_json(&credentials);
    app.store
        .set_provider_credentials(
            &client.partition,
            &client.user_id,
            provider.name(),
            &encrypted,
        )
        .await?;
    Ok(Json(ProviderCredentialsStatusOut {
        provider: provider.name().to_string(),
        configured: true,
    }))
}

async fn get_credentials_status(
    State(app): State<Arc<App>>,
    Client(client): Client,
    Path(provider): Path<String>,
) -> AppResult<Json<ProviderCredentialsStatusOut>> {
    let provider = known_provider(&provider)?;
    let stored = app
        .store
        .get_provider_credentials(&client.partition, &client.user_id, provider.name())
        .await?;
    Ok(Json(ProviderCredentialsStatusOut {
        provider: provider.name().to_string(),
        configured: stored.is_some(),
    }))
}

async fn delete_credentials(
    State(app): State<Arc<App>>,
    Client(client): Client,
    Path(provider): Path<String>,
) -> AppResult<StatusCode> {
    let provider = known_provider(&provider)?;
    app.store
        .delete_provider_credentials(&client.partition, &client.user_id, provider.name())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_credentials_status(
    State(app): State<Arc<App>>,
    Client(client): Client,
) -> AppResult<Json<ProviderCredentialsListOut>> {
    let configured: HashSet<String> = app
        .store
        .list_configured_providers(&client.partition, &client.user_id)
        .await?
        .into_iter()
        .collect();
    Ok(Json(ProviderCredentialsListOut {
        providers: PROVIDERS
            .iter()
            .map(|provider| ProviderCredentialsStatusOut {
                provider: provider.to_string(),
                configured: configured.contains(*provider),
            })
            .collect(),
    }))
}
