// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Thin async wrapper around the usecode-agent-api HTTP endpoints, spread over
//! the configured Caddy load balancers (see `Settings::endpoints`).

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use reqwest::{Method, StatusCode};
use serde_json::{Map, Value, json};
use thiserror::Error;

use crate::config::Settings;

/// Statuses Caddy returns when *it* is up but had no healthy API instance to
/// forward to. The request may or may not have been processed, so these are
/// only retried for methods that can be repeated without doubling an effect.
const GATEWAY_STATUSES: [u16; 3] = [502, 503, 504];

fn is_repeatable(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::OPTIONS | Method::PUT | Method::DELETE
    )
}

/// Round-robin cursor. Shared across clients on purpose: the server builds a
/// fresh `Client` for every tool call, so a per-instance cursor would start at
/// the same endpoint every time and never rotate.
static CURSOR: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Error)]
#[error("usecode agent API error {status_code}: {detail}")]
pub struct ApiError {
    pub status_code: u16,
    pub detail: String,
}

impl ApiError {
    pub fn new(status_code: u16, detail: impl Into<String>) -> Self {
        Self {
            status_code,
            detail: detail.into(),
        }
    }

    /// No configured endpoint could be reached at all. Reported as a 503 so
    /// every tool's existing error handling covers it — from a caller's point
    /// of view "every load balancer is down" is just another unavailable
    /// answer.
    fn unreachable(endpoints: &[String], last_error: Option<&dyn fmt::Display>) -> Self {
        let joined = if endpoints.is_empty() {
            "<none configured>".to_string()
        } else {
            endpoints.join(", ")
        };
        let cause = match last_error {
            Some(error) => error.to_string(),
            None => "None".to_string(),
        };
        Self::new(
            503,
            format!("No usecode agent endpoint reachable ({joined}): {cause}"),
        )
    }

    /// The `{"error": ..., "status_code": ...}` shape every tool reports.
    pub fn to_value(&self) -> Value {
        json!({"error": self.detail, "status_code": self.status_code})
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// One call to the API, before it is aimed at a particular load balancer.
struct Call {
    method: Method,
    path: String,
    json: Option<Value>,
    query: Vec<(String, String)>,
    api_key: Option<String>,
}

impl Call {
    fn new(method: Method, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            json: None,
            query: Vec::new(),
            api_key: None,
        }
    }

    fn json(mut self, body: Value) -> Self {
        self.json = Some(body);
        self
    }

    fn query(mut self, pairs: Vec<(String, String)>) -> Self {
        self.query = pairs;
        self
    }

    fn api_key(mut self, api_key: Option<String>) -> Self {
        self.api_key = api_key;
        self
    }
}

#[derive(Clone)]
pub struct Client {
    settings: Arc<Settings>,
    http: reqwest::Client,
}

impl Client {
    pub fn new(settings: Arc<Settings>) -> crate::error::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs_f64(settings.request_timeout_seconds))
            .tls_danger_accept_invalid_certs(!settings.api_verify_ssl)
            .build()?;
        Ok(Self { settings, http })
    }

    /// The endpoints to try, starting at the next one in the rotation so
    /// consecutive calls land on different load balancers, then continuing
    /// through the rest as failover.
    fn attempt_order(&self) -> Vec<String> {
        let endpoints = self.settings.endpoints();
        if endpoints.is_empty() {
            return endpoints;
        }
        let start = CURSOR.fetch_add(1, Ordering::Relaxed) % endpoints.len();
        (0..endpoints.len())
            .map(|i| endpoints[(start + i) % endpoints.len()].clone())
            .collect()
    }

    async fn send(&self, base_url: &str, call: &Call) -> reqwest::Result<reqwest::Response> {
        let url = format!("{}{}", base_url.trim_end_matches('/'), call.path);
        let mut request = self.http.request(call.method.clone(), url);
        if let Some(key) = call.api_key.as_deref().or(self.settings.api_key.as_deref()) {
            request = request.header("X-API-Key", key);
        }
        if !call.query.is_empty() {
            request = request.query(&call.query);
        }
        if let Some(body) = &call.json {
            request = request.json(body);
        }
        request.send().await
    }

    async fn api_error(response: reqwest::Response) -> ApiError {
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|body| body.get("detail").cloned())
            .map(|detail| match detail {
                Value::String(message) => message,
                other => other.to_string(),
            })
            .unwrap_or(text);
        ApiError::new(status, detail)
    }

    async fn request(&self, call: Call) -> ApiResult<reqwest::Response> {
        let order = self.attempt_order();
        if order.is_empty() {
            return Err(ApiError::unreachable(&order, None));
        }

        let mut last_error: Option<ApiError> = None;
        for (attempt, base_url) in order.iter().enumerate() {
            let is_last = attempt == order.len() - 1;
            let response = match self.send(base_url, &call).await {
                Ok(response) => response,
                // A failure that proves the request never reached an API
                // instance: the connection was refused, or timed out before
                // it was established. Retrying against another load balancer
                // is always safe, whatever the method.
                Err(error) if error.is_connect() => {
                    if is_last {
                        return Err(ApiError::unreachable(&order, Some(&error)));
                    }
                    last_error = Some(ApiError::new(503, error.to_string()));
                    continue;
                }
                Err(error) => return Err(ApiError::new(503, error.to_string())),
            };

            let status = response.status();
            if status.is_client_error() || status.is_server_error() {
                let error = Self::api_error(response).await;
                if !is_last
                    && GATEWAY_STATUSES.contains(&error.status_code)
                    && is_repeatable(&call.method)
                {
                    last_error = Some(error);
                    continue;
                }
                return Err(error);
            }
            return Ok(response);
        }

        // Only reachable if every endpoint answered with a gateway status.
        Err(last_error.unwrap_or_else(|| ApiError::unreachable(&order, None)))
    }

    async fn json(&self, call: Call) -> ApiResult<Value> {
        let response = self.request(call).await?;
        let status = response.status().as_u16();
        response
            .json()
            .await
            .map_err(|error| ApiError::new(status, error.to_string()))
    }

    async fn empty(&self, call: Call) -> ApiResult<()> {
        self.request(call).await.map(|_| ())
    }

    pub async fn request_otp(&self, phone: &str) -> ApiResult<Value> {
        self.json(Call::new(Method::POST, "/auth/otp/request").json(json!({"phone": phone})))
            .await
    }

    pub async fn verify_otp(&self, phone: &str, code: &str) -> ApiResult<Value> {
        self.json(
            Call::new(Method::POST, "/auth/otp/verify").json(json!({"phone": phone, "code": code})),
        )
        .await
    }

    pub async fn me(&self, api_key: Option<String>) -> ApiResult<Value> {
        self.json(Call::new(Method::GET, "/auth/me").api_key(api_key))
            .await
    }

    pub async fn logout(&self, api_key: Option<String>) -> ApiResult<()> {
        self.empty(Call::new(Method::POST, "/auth/logout").api_key(api_key))
            .await
    }

    pub async fn create_api_key(&self, label: &str, api_key: Option<String>) -> ApiResult<Value> {
        self.json(
            Call::new(Method::POST, "/auth/api-keys")
                .json(json!({"label": label}))
                .api_key(api_key),
        )
        .await
    }

    pub async fn list_api_keys(&self, api_key: Option<String>) -> ApiResult<Value> {
        self.json(Call::new(Method::GET, "/auth/api-keys").api_key(api_key))
            .await
    }

    pub async fn revoke_api_key(&self, key_id: &str, api_key: Option<String>) -> ApiResult<()> {
        self.empty(Call::new(Method::DELETE, format!("/auth/api-keys/{key_id}")).api_key(api_key))
            .await
    }

    /// Check every configured endpoint rather than just the next one in the
    /// rotation, so a single dead load balancer is visible instead of being
    /// silently failed over. The API names the node that answered (`node`),
    /// which is also how to see Caddy spreading requests over api-1/api-2.
    pub async fn health_all(&self) -> Vec<Value> {
        let call = Call::new(Method::GET, "/health");
        let mut results = Vec::new();
        for base_url in self.settings.endpoints() {
            let mut entry = Map::new();
            entry.insert("endpoint".to_string(), json!(base_url));
            match self.send(&base_url, &call).await {
                Err(error) => {
                    entry.insert("reachable".to_string(), json!(false));
                    entry.insert("error".to_string(), json!(error.to_string()));
                }
                Ok(response) if is_error(response.status()) => {
                    let error = Self::api_error(response).await;
                    entry.insert("reachable".to_string(), json!(false));
                    entry.insert("status_code".to_string(), json!(error.status_code));
                    entry.insert("error".to_string(), json!(error.detail));
                }
                Ok(response) => {
                    entry.insert("reachable".to_string(), json!(true));
                    match response.json::<Value>().await {
                        Ok(Value::Object(body)) => entry.extend(body),
                        Ok(other) => {
                            entry.insert("body".to_string(), other);
                        }
                        Err(error) => {
                            entry.insert("error".to_string(), json!(error.to_string()));
                        }
                    }
                }
            }
            results.push(Value::Object(entry));
        }
        results
    }

    pub async fn model_options(&self, api_key: Option<String>) -> ApiResult<Value> {
        self.json(Call::new(Method::GET, "/models/options").api_key(api_key))
            .await
    }

    pub async fn model_status(&self, api_key: Option<String>) -> ApiResult<Value> {
        self.json(Call::new(Method::GET, "/models/status").api_key(api_key))
            .await
    }

    pub async fn model_start(&self, overrides: Value, api_key: Option<String>) -> ApiResult<Value> {
        self.json(
            Call::new(Method::POST, "/models/start")
                .json(overrides)
                .api_key(api_key),
        )
        .await
    }

    pub async fn model_stop(&self, api_key: Option<String>) -> ApiResult<()> {
        self.empty(Call::new(Method::POST, "/models/stop").api_key(api_key))
            .await
    }

    pub async fn set_provider_credentials(
        &self,
        provider: &str,
        credentials: Value,
        api_key: Option<String>,
    ) -> ApiResult<Value> {
        self.json(
            Call::new(Method::PUT, format!("/providers/{provider}/credentials"))
                .json(json!({"credentials": credentials}))
                .api_key(api_key),
        )
        .await
    }

    pub async fn provider_credentials_status(
        &self,
        provider: &str,
        api_key: Option<String>,
    ) -> ApiResult<Value> {
        self.json(
            Call::new(Method::GET, format!("/providers/{provider}/credentials")).api_key(api_key),
        )
        .await
    }

    pub async fn delete_provider_credentials(
        &self,
        provider: &str,
        api_key: Option<String>,
    ) -> ApiResult<()> {
        self.empty(
            Call::new(Method::DELETE, format!("/providers/{provider}/credentials"))
                .api_key(api_key),
        )
        .await
    }

    pub async fn list_provider_credentials(&self, api_key: Option<String>) -> ApiResult<Value> {
        self.json(Call::new(Method::GET, "/providers/credentials").api_key(api_key))
            .await
    }

    pub async fn list_servers(&self, api_key: Option<String>) -> ApiResult<Value> {
        self.json(Call::new(Method::GET, "/servers").api_key(api_key))
            .await
    }

    pub async fn list_server_types(&self, api_key: Option<String>) -> ApiResult<Value> {
        self.json(Call::new(Method::GET, "/servers/types").api_key(api_key))
            .await
    }

    pub async fn get_server(&self, server_id: &str, api_key: Option<String>) -> ApiResult<Value> {
        self.json(Call::new(Method::GET, format!("/servers/{server_id}")).api_key(api_key))
            .await
    }

    pub async fn create_server(
        &self,
        name: &str,
        server_type: &str,
        image: &str,
        ssh_keys: Vec<String>,
        api_key: Option<String>,
    ) -> ApiResult<Value> {
        self.json(
            Call::new(Method::POST, "/servers")
                .json(json!({
                    "name": name,
                    "type": server_type,
                    "image": image,
                    "ssh_keys": ssh_keys,
                }))
                .api_key(api_key),
        )
        .await
    }

    pub async fn delete_server(
        &self,
        server_id: &str,
        api_key: Option<String>,
    ) -> ApiResult<Value> {
        self.json(Call::new(Method::DELETE, format!("/servers/{server_id}")).api_key(api_key))
            .await
    }

    pub async fn get_task(&self, task_id: &str, api_key: Option<String>) -> ApiResult<Value> {
        self.json(Call::new(Method::GET, format!("/tasks/{task_id}")).api_key(api_key))
            .await
    }

    pub async fn list_tasks(&self, api_key: Option<String>) -> ApiResult<Value> {
        self.json(Call::new(Method::GET, "/tasks").api_key(api_key))
            .await
    }

    pub async fn sync_servers(&self, api_key: Option<String>) -> ApiResult<Value> {
        self.json(Call::new(Method::POST, "/servers/sync").api_key(api_key))
            .await
    }

    pub async fn list_catalog(
        &self,
        provider: Option<String>,
        kind: Option<String>,
        api_key: Option<String>,
    ) -> ApiResult<Value> {
        let query = [("provider", provider), ("kind", kind)]
            .into_iter()
            .filter_map(|(name, value)| value.map(|value| (name.to_string(), value)))
            .filter(|(_, value)| !value.is_empty())
            .collect();
        self.json(
            Call::new(Method::GET, "/servers/catalog")
                .query(query)
                .api_key(api_key),
        )
        .await
    }
}

fn is_error(status: StatusCode) -> bool {
    status.is_client_error() || status.is_server_error()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_repeatable_methods_retry_a_gateway_status() {
        assert!(is_repeatable(&Method::GET));
        assert!(is_repeatable(&Method::DELETE));
        assert!(!is_repeatable(&Method::POST));
        assert!(!is_repeatable(&Method::PATCH));
    }

    #[test]
    fn unreachable_names_every_endpoint() {
        let error = ApiError::unreachable(&["a".to_string(), "b".to_string()], None);
        assert_eq!(error.status_code, 503);
        assert!(error.detail.contains("a, b"));
    }

    #[test]
    fn attempt_order_rotates_and_covers_every_endpoint() {
        let settings = Arc::new(Settings::default());
        let client = Client::new(settings).unwrap();
        let first = client.attempt_order();
        let second = client.attempt_order();
        assert_eq!(first.len(), 2);
        assert_ne!(first[0], second[0]);
        assert_ne!(first[0], first[1]);
    }
}
