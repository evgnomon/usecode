// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Request extractors: the authenticated API client, and JSON bodies that
//! fail with the API's own 422 shape.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::App;
use crate::error::AppError;
use crate::store::ApiKeyRecord;

/// The caller, authenticated by the `X-API-Key` header.
pub struct Client(pub ApiKeyRecord);

impl FromRequestParts<Arc<App>> for Client {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, app: &Arc<App>) -> Result<Self, AppError> {
        let api_key = api_key(&parts.headers)
            .ok_or_else(|| AppError::unauthorized("Missing X-API-Key header"))?;
        app.store
            .get_api_key(&api_key)
            .await?
            .map(Client)
            .ok_or_else(|| AppError::unauthorized("Invalid or expired API key"))
    }
}

pub fn api_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// A JSON request body. An empty body reads as `{}`, so a body whose fields
/// all have defaults may be omitted.
pub struct JsonBody<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for JsonBody<T> {
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, AppError> {
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(|err| AppError::new(StatusCode::BAD_REQUEST, err.body_text()))?;
        let bytes: &[u8] = if bytes.iter().all(u8::is_ascii_whitespace) {
            b"{}"
        } else {
            &bytes
        };
        serde_json::from_slice(bytes).map(JsonBody).map_err(|err| {
            let kind = if err.is_data() {
                "value_error"
            } else {
                "json_invalid"
            };
            AppError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                detail: json!([{"type": kind, "loc": ["body"], "msg": err.to_string()}]),
            }
        })
    }
}

/// A URL-encoded form body, failing with the API's 422 shape.
pub struct FormBody<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for FormBody<T> {
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, AppError> {
        axum::Form::<T>::from_request(req, state)
            .await
            .map(|axum::Form(value)| FormBody(value))
            .map_err(|err| AppError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                detail: json!([{"type": "missing", "loc": ["body"], "msg": err.body_text()}]),
            })
    }
}
