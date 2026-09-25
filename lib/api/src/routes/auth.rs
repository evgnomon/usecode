// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! /auth: OTP login, the caller's identity, and API key management.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{delete, get, post};
use axum::{Json, Router};

use crate::App;
use crate::error::{AppError, AppResult};
use crate::extract::{Client, JsonBody, api_key};
use crate::models::{
    ApiKeyCreateIn, ApiKeyCreateOut, ApiKeyListOut, ApiKeyOut, AuthTokenOut, MeOut, OtpRequestIn,
    OtpRequestOut, OtpVerifyIn,
};
use crate::otp;

pub fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/auth/otp/request", post(request_otp))
        .route("/auth/otp/verify", post(verify_otp))
        .route("/auth/me", get(me))
        .route("/auth/logout", post(logout))
        .route("/auth/api-keys", post(create_api_key).get(list_api_keys))
        .route("/auth/api-keys/{key_id}", delete(revoke_api_key))
}

async fn request_otp(
    State(app): State<Arc<App>>,
    JsonBody(payload): JsonBody<OtpRequestIn>,
) -> AppResult<Json<OtpRequestOut>> {
    let payload = payload.validated()?;
    let code = otp::request(&app, &payload.phone)
        .await?
        .map_err(|(status, detail)| AppError::new(status, detail))?;
    let settings = &app.settings;
    Ok(Json(OtpRequestOut {
        phone: payload.phone,
        expires_in: settings.otp_ttl_seconds,
        resend_after: settings.otp_resend_cooldown_seconds,
        debug_code: settings.debug_expose_otp.then_some(code),
    }))
}

async fn verify_otp(
    State(app): State<Arc<App>>,
    JsonBody(payload): JsonBody<OtpVerifyIn>,
) -> AppResult<Json<AuthTokenOut>> {
    let payload = payload.validated()?;
    otp::verify(&app, &payload.phone, &payload.code)
        .await?
        .map_err(|(status, detail)| AppError::new(status, detail))?;
    let (user_id, _) = app.store.get_or_create_user(&payload.phone).await?;
    let api_key = app.store.issue_api_key(&user_id, "login").await?;
    Ok(Json(AuthTokenOut {
        api_key,
        phone: payload.phone,
    }))
}

async fn me(Client(client): Client) -> Json<MeOut> {
    Json(MeOut {
        phone: client.phone,
        created_at: client.created_at,
    })
}

async fn logout(State(app): State<Arc<App>>, headers: HeaderMap) -> AppResult<StatusCode> {
    if let Some(api_key) = api_key(&headers) {
        app.store.revoke_api_key(&api_key).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn create_api_key(
    State(app): State<Arc<App>>,
    Client(client): Client,
    JsonBody(payload): JsonBody<ApiKeyCreateIn>,
) -> AppResult<Json<ApiKeyCreateOut>> {
    let api_key = app
        .store
        .issue_api_key(&client.user_id, &payload.label)
        .await?;
    let created = app
        .store
        .get_api_key(&api_key)
        .await?
        .ok_or_else(|| anyhow::anyhow!("a freshly issued API key does not resolve"))?;
    Ok(Json(ApiKeyCreateOut {
        id: created.id,
        api_key,
        label: created.label,
        created_at: created.created_at,
    }))
}

async fn list_api_keys(
    State(app): State<Arc<App>>,
    Client(client): Client,
) -> AppResult<Json<ApiKeyListOut>> {
    let keys = app.store.list_api_keys(&client.user_id).await?;
    Ok(Json(ApiKeyListOut {
        api_keys: keys
            .into_iter()
            .map(|key| ApiKeyOut {
                id: key.id,
                label: key.label,
                created_at: key.created_at,
                last_used_at: key.last_used_at,
            })
            .collect(),
    }))
}

async fn revoke_api_key(
    State(app): State<Arc<App>>,
    Client(client): Client,
    Path(key_id): Path<String>,
) -> AppResult<StatusCode> {
    // The id of an API key is its hash — what routes to the instance holding
    // it. See schema::API_KEYS.
    if !app
        .store
        .revoke_api_key_for_user(&client.user_id, &key_id)
        .await?
    {
        return Err(AppError::not_found("API key not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}
