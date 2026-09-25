// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Cloud provider registry.
//!
//! Each provider module exposes:
//! - `validate_credentials(credentials)`, failing with `InvalidCredentials`
//!   if the shape is wrong.
//! - `list_servers(credentials) -> Vec<CloudServer>`.
//! - `list_server_types(credentials) -> Vec<Value>`, each object shaped
//!   `{"provider_server_type": str, "cpu": int, "memory_gb": float,
//!   "disk_gb": float, "cities": [str]}`.
//! - `list_locations(credentials) -> Vec<Value>`, each object including at
//!   least `{"code": str, "provider_location_code": str}` plus whatever else
//!   the provider returns for that location.
//! - `list_images(credentials) -> Vec<Value>`, each object including at
//!   least `{"code": str}` plus whatever else the provider returns for that
//!   image.
//! - `create_server(credentials, spec) -> CloudServer` and
//!   `delete_server(credentials, id)`.
//!
//! `credentials` is the provider-specific JSON object a user stored (e.g.
//! `{"apiKey": "..."}`). Add a new provider by writing a module here and
//! registering it in [`Provider`].

mod digitalocean;
mod hetzner;

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Method;
use serde_json::{Map, Value};

use crate::models::{CloudServer, CloudServerCreateIn};

/// Every provider name, sorted.
pub const PROVIDERS: &[&str] = &["digitalocean", "hetzner"];

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("{provider} API request failed ({status}): {detail}")]
    Api {
        provider: &'static str,
        status: u16,
        detail: String,
    },
    #[error("{provider} API request failed: {detail}")]
    Transport {
        provider: &'static str,
        detail: String,
    },
    #[error("{provider} API returned an unexpected response: {detail}")]
    Malformed {
        provider: &'static str,
        detail: String,
    },
    #[error("Invalid credentials for {provider}: {detail}")]
    InvalidCredentials {
        provider: &'static str,
        detail: String,
    },
    #[error("Unknown provider '{0}', expected one of ['digitalocean', 'hetzner']")]
    Unknown(String),
}

pub type ProviderResult<T> = Result<T, ProviderError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Hetzner,
    DigitalOcean,
}

impl Provider {
    pub fn parse(name: &str) -> ProviderResult<Self> {
        match name {
            "hetzner" => Ok(Self::Hetzner),
            "digitalocean" => Ok(Self::DigitalOcean),
            other => Err(ProviderError::Unknown(other.to_string())),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Hetzner => "hetzner",
            Self::DigitalOcean => "digitalocean",
        }
    }

    pub fn validate_credentials(self, credentials: &Value) -> ProviderResult<()> {
        require_field(self.name(), credentials, "apiKey").map(|_| ())
    }

    pub async fn list_servers(self, credentials: &Value) -> ProviderResult<Vec<CloudServer>> {
        match self {
            Self::Hetzner => hetzner::list_servers(credentials).await,
            Self::DigitalOcean => digitalocean::list_servers(credentials).await,
        }
    }

    pub async fn list_server_types(self, credentials: &Value) -> ProviderResult<Vec<Value>> {
        match self {
            Self::Hetzner => hetzner::list_server_types(credentials).await,
            Self::DigitalOcean => digitalocean::list_server_types(credentials).await,
        }
    }

    pub async fn list_locations(self, credentials: &Value) -> ProviderResult<Vec<Value>> {
        match self {
            Self::Hetzner => hetzner::list_locations(credentials).await,
            Self::DigitalOcean => digitalocean::list_locations(credentials).await,
        }
    }

    pub async fn list_images(self, credentials: &Value) -> ProviderResult<Vec<Value>> {
        match self {
            Self::Hetzner => hetzner::list_images(credentials).await,
            Self::DigitalOcean => digitalocean::list_images(credentials).await,
        }
    }

    pub async fn create_server(
        self,
        credentials: &Value,
        spec: &CloudServerCreateIn,
    ) -> ProviderResult<CloudServer> {
        match self {
            Self::Hetzner => hetzner::create_server(credentials, spec).await,
            Self::DigitalOcean => digitalocean::create_server(credentials, spec).await,
        }
    }

    pub async fn delete_server(self, credentials: &Value, server_id: &str) -> ProviderResult<()> {
        match self {
            Self::Hetzner => hetzner::delete_server(credentials, server_id).await,
            Self::DigitalOcean => digitalocean::delete_server(credentials, server_id).await,
        }
    }
}

/// Python-style truthiness, which is what the provider payloads' optional
/// flags ("deprecated", "available", ...) have always been read with.
pub(crate) fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

pub(crate) fn require_field(
    provider: &'static str,
    credentials: &Value,
    field: &str,
) -> ProviderResult<String> {
    let value = credentials.get(field);
    if !truthy(value) {
        return Err(ProviderError::InvalidCredentials {
            provider,
            detail: format!("missing '{field}'"),
        });
    }
    Ok(match value {
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => unreachable!("checked above"),
    })
}

/// A field's value as text: strings as-is, anything else (e.g. numeric ids)
/// in its JSON spelling.
pub(crate) fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

pub(crate) fn field<'a>(
    provider: &'static str,
    value: &'a Value,
    key: &str,
) -> ProviderResult<&'a Value> {
    value.get(key).ok_or_else(|| ProviderError::Malformed {
        provider,
        detail: format!("missing '{key}'"),
    })
}

pub(crate) fn items<'a>(
    provider: &'static str,
    body: &'a Value,
    key: &str,
) -> ProviderResult<&'a Vec<Value>> {
    field(provider, body, key)?
        .as_array()
        .ok_or_else(|| ProviderError::Malformed {
            provider,
            detail: format!("'{key}' is not a list"),
        })
}

/// `{**head, **tail}`: `head`'s keys first, `tail` overriding them.
pub(crate) fn merged(head: Vec<(&str, Value)>, tail: &Value) -> Value {
    let mut map: Map<String, Value> = head.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    if let Some(object) = tail.as_object() {
        for (key, value) in object {
            map.insert(key.clone(), value.clone());
        }
    }
    Value::Object(map)
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("HTTP client")
    })
}

/// The provider API's base URL; overridable (e.g. to point tests at a fake)
/// with `USECODE_AGENT_<PROVIDER>_API_BASE`.
pub(crate) fn api_base(provider: &str, default: &str) -> String {
    std::env::var(format!(
        "USECODE_AGENT_{}_API_BASE",
        provider.to_uppercase()
    ))
    .ok()
    .filter(|base| !base.is_empty())
    .unwrap_or_else(|| default.to_string())
}

pub(crate) struct Call<'a> {
    pub provider: &'static str,
    pub token: &'a str,
    pub method: Method,
    pub url: String,
    pub query: &'a [(&'a str, &'a str)],
    pub body: Option<&'a Value>,
    pub expect: &'a [u16],
}

/// Send one request and return the JSON body (`Null` for an empty one), or
/// `Api` when the status is not one of `expect`.
pub(crate) async fn call(call: Call<'_>) -> ProviderResult<Value> {
    let transport = |err: reqwest::Error| ProviderError::Transport {
        provider: call.provider,
        detail: err.to_string(),
    };
    let mut request = client()
        .request(call.method, &call.url)
        .bearer_auth(call.token)
        .query(call.query);
    if let Some(body) = call.body {
        request = request.json(body);
    }
    let response = request.send().await.map_err(transport)?;
    let status = response.status().as_u16();
    let body = response.text().await.map_err(transport)?;
    if !call.expect.contains(&status) {
        return Err(ProviderError::Api {
            provider: call.provider,
            status,
            detail: body,
        });
    }
    if body.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&body).map_err(|err| ProviderError::Malformed {
        provider: call.provider,
        detail: err.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn credentials_need_an_api_key() {
        let hetzner = Provider::parse("hetzner").unwrap();
        assert!(
            hetzner
                .validate_credentials(&json!({"apiKey": "x"}))
                .is_ok()
        );
        let err = hetzner
            .validate_credentials(&json!({"apiKey": ""}))
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Invalid credentials for hetzner: missing 'apiKey'"
        );
        assert_eq!(
            Provider::parse("aws").unwrap_err().to_string(),
            "Unknown provider 'aws', expected one of ['digitalocean', 'hetzner']"
        );
    }

    #[test]
    fn merged_keeps_head_first_and_lets_tail_override() {
        let value = merged(
            vec![("code", json!("fsn")), ("x", json!(1))],
            &json!({"x": 2, "name": "n"}),
        );
        assert_eq!(value.to_string(), r#"{"code":"fsn","x":2,"name":"n"}"#);
    }
}
