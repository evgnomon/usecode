// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Request and response bodies of the JSON API.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::AppError;

// Raw provider/internal server states that count as "up" — everything else
// (off, new, archive, deleting, migrating, rebuilding, unknown, ...) is
// "paused". Hetzner and DigitalOcean use disjoint vocabularies, so this maps
// both into the two states our API ever exposes.
const UP_STATUSES: &[&str] = &["running", "active", "starting", "initializing"];

pub fn normalize_server_status(raw: &str) -> &'static str {
    if UP_STATUSES.contains(&raw.to_lowercase().as_str()) {
        "up"
    } else {
        "paused"
    }
}

pub const PHONE_FORMAT_ERROR: &str = "Phone number must be in E.164-like format, e.g. +14155552671";

/// Strip spaces and dashes and require `+?[1-9]\d{7,14}`; the result always
/// carries the leading "+".
pub fn normalize_phone(value: &str) -> Result<String, &'static str> {
    let value: String = value
        .trim()
        .chars()
        .filter(|c| *c != ' ' && *c != '-')
        .collect();
    let digits = value.strip_prefix('+').unwrap_or(&value);
    let valid = (8..=15).contains(&digits.len())
        && digits.chars().all(|c| c.is_ascii_digit())
        && !digits.starts_with('0');
    if !valid {
        return Err(PHONE_FORMAT_ERROR);
    }
    Ok(format!("+{digits}"))
}

fn phone_field(phone: &str) -> Result<String, AppError> {
    normalize_phone(phone)
        .map_err(|msg| AppError::invalid(&["body", "phone"], format!("Value error, {msg}")))
}

#[derive(Deserialize)]
pub struct OtpRequestIn {
    pub phone: String,
}

impl OtpRequestIn {
    pub fn validated(self) -> Result<Self, AppError> {
        Ok(Self {
            phone: phone_field(&self.phone)?,
        })
    }
}

#[derive(Serialize)]
pub struct OtpRequestOut {
    pub phone: String,
    pub expires_in: i64,
    pub resend_after: i64,
    pub debug_code: Option<String>,
}

#[derive(Deserialize)]
pub struct OtpVerifyIn {
    pub phone: String,
    pub code: String,
}

impl OtpVerifyIn {
    pub fn validated(self) -> Result<Self, AppError> {
        let code = self.code.trim().to_string();
        if code.is_empty() || !code.chars().all(|c| c.is_ascii_digit()) {
            return Err(AppError::invalid(
                &["body", "code"],
                "Value error, Code must be numeric",
            ));
        }
        Ok(Self {
            phone: phone_field(&self.phone)?,
            code,
        })
    }
}

#[derive(Serialize)]
pub struct AuthTokenOut {
    pub api_key: String,
    pub phone: String,
}

#[derive(Serialize)]
pub struct MeOut {
    pub phone: String,
    pub created_at: f64,
}

#[derive(Deserialize, Default)]
pub struct ApiKeyCreateIn {
    #[serde(default)]
    pub label: String,
}

#[derive(Serialize)]
pub struct ApiKeyCreateOut {
    pub id: String,
    pub api_key: String,
    pub label: String,
    pub created_at: f64,
}

#[derive(Serialize)]
pub struct ApiKeyOut {
    pub id: String,
    pub label: String,
    pub created_at: f64,
    pub last_used_at: Option<f64>,
}

#[derive(Serialize)]
pub struct ApiKeyListOut {
    pub api_keys: Vec<ApiKeyOut>,
}

#[derive(Serialize)]
pub struct ModelOptionsOut {
    pub fields: Vec<Value>,
}

#[derive(Serialize)]
pub struct ModelStatusOut {
    pub running: bool,
    pub config: Option<Map<String, Value>>,
    pub state: Option<Value>,
}

/// Raw shape returned by a provider client (internal use only — never
/// exposed directly over the public API, which speaks our own server
/// IDs/types instead).
#[derive(Debug, Clone)]
pub struct CloudServer {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub status: String,
    pub server_type: String,
    pub location: String,
    pub public_ip4: Option<String>,
    pub public_ip6: Option<String>,
}

/// Provider-facing create spec (internal use only). It is stored in the
/// `create_server` task payload, so its shape is part of the task format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudServerCreateIn {
    #[serde(default = "default_provider")]
    pub provider: String,
    pub name: String,
    pub server_type: String,
    pub image: String,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub ssh_keys: Vec<String>,
}

fn default_provider() -> String {
    "hetzner".to_string()
}

#[derive(Deserialize)]
pub struct ServerCreateIn {
    // Our own type terminology, e.g. "x1-fsn" or "y1-nyc" — series (x1, x2,
    // x4, x8 for Hetzner; y1, y2, y4, y8 for DigitalOcean) plus our own city
    // code.
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(default = "default_image")]
    pub image: String,
    #[serde(default)]
    pub ssh_keys: Vec<String>,
}

fn default_image() -> String {
    "ubuntu-24.04".to_string()
}

#[derive(Serialize)]
pub struct ServerOut {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub status: &'static str,
    pub public_ip4: Option<String>,
    pub public_ip6: Option<String>,
    pub created: String,
}

#[derive(Serialize)]
pub struct ServerListOut {
    pub servers: Vec<ServerOut>,
}

#[derive(Serialize)]
pub struct ServerTypeOut {
    // Our own series identifier, e.g. "x1" or "y2" (no city — specs don't
    // vary by city, only by series).
    #[serde(rename = "type")]
    pub r#type: String,
    pub cpu: i64,
    pub memory_gb: f64,
    pub disk_gb: f64,
    // Our own city codes this series is available in, e.g. ["fsn", "ash"] —
    // combine one with `type` (as "{type}-{city}") to build a value valid
    // for POST /servers' `type` field.
    pub cities: Vec<String>,
}

#[derive(Serialize)]
pub struct ServerTypeListOut {
    pub types: Vec<ServerTypeOut>,
}

#[derive(Serialize)]
pub struct ServerSyncOut {
    pub added: usize,
    pub updated: usize,
    pub servers: Vec<ServerOut>,
}

#[derive(Serialize)]
pub struct ProviderResourceOut {
    pub provider: String,
    pub kind: String,
    pub code: String,
    pub data: Value,
}

#[derive(Serialize)]
pub struct ProviderResourceListOut {
    pub resources: Vec<ProviderResourceOut>,
}

#[derive(Serialize)]
pub struct TaskOut {
    pub id: String,
    pub kind: String,
    // Node name of the API instance carrying this task to completion — and
    // where the task row lives, since `tasks` is partitioned on it.
    pub assignee: String,
    pub state: String,
    pub resources: Value,
    pub error: Option<String>,
    pub created_at: f64,
    pub updated_at: f64,
}

#[derive(Serialize)]
pub struct TaskListOut {
    pub tasks: Vec<TaskOut>,
}

#[derive(Deserialize)]
pub struct ProviderCredentialsIn {
    // Shape is provider-specific, e.g. {"apiKey": "..."} for Hetzner and
    // DigitalOcean.
    pub credentials: Map<String, Value>,
}

impl ProviderCredentialsIn {
    pub fn validated(self) -> Result<Self, AppError> {
        if self.credentials.is_empty() {
            return Err(AppError::invalid(
                &["body", "credentials"],
                "Value error, credentials must not be empty",
            ));
        }
        Ok(self)
    }
}

#[derive(Serialize)]
pub struct ProviderCredentialsStatusOut {
    pub provider: String,
    pub configured: bool,
}

#[derive(Serialize)]
pub struct ProviderCredentialsListOut {
    pub providers: Vec<ProviderCredentialsStatusOut>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phones_normalize_like_before() {
        assert_eq!(normalize_phone(" 1 415-555-2671 ").unwrap(), "+14155552671");
        assert_eq!(normalize_phone("+14155552671").unwrap(), "+14155552671");
        assert!(normalize_phone("+0123456789").is_err());
        assert!(normalize_phone("1234567").is_err());
        assert!(normalize_phone("1234567890123456").is_err());
        assert!(normalize_phone("++14155552671").is_err());
        assert!(normalize_phone("+1415abc2671").is_err());
    }

    #[test]
    fn statuses_collapse_to_up_or_paused() {
        assert_eq!(normalize_server_status("Running"), "up");
        assert_eq!(normalize_server_status("active"), "up");
        assert_eq!(normalize_server_status("off"), "paused");
    }
}
