// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Settings read from the environment, prefixed `USECODE_MCP_`, with a `.env`
//! file in the working directory filling in anything not already set.

use std::env;

use crate::error::Result;

/// Default Caddy load balancers, matching `deploy/compose.yml`.
const DEFAULT_API_BASE_URLS: [&str; 2] =
    ["https://localhost:4430/api", "https://localhost:4431/api"];

#[derive(Debug, Clone)]
pub struct Settings {
    // The Caddy load balancers in front of usecode-agent-api, each routing /api/*
    // to the API instances behind it. Requests are spread over these
    // round-robin, and one that can't reach an endpoint is retried against
    // the next.
    //
    // This is the tier *above* Caddy's own load balancing: Caddy already
    // spreads requests over api-1/api-2, but a client pinned to a single
    // Caddy goes down with it. See deploy/compose.yml.
    pub api_base_urls: Vec<String>,

    // Single-endpoint override. Set USECODE_MCP_API_BASE_URL to talk to one
    // specific address (a remote deployment, or a bare usecode-agent-api with no
    // Caddy in front) — it replaces the list above rather than adding to it,
    // so the bot then has exactly the one endpoint asked for.
    pub api_base_url: Option<String>,

    // API key issued by POST /auth/otp/verify. Lets the bot act as an already
    // logged-in usecode agent client instead of running the OTP flow on every call.
    pub api_key: Option<String>,

    pub request_timeout_seconds: f64,

    // Verify the API server's TLS certificate. Caddy issues a self-signed
    // cert for local/non-public deployments, so set this to false in your
    // local .env if you hit a certificate verification failure talking to
    // localhost.
    pub api_verify_ssl: bool,

    // Path to the compose file used to run usecode agent locally. Defaults to
    // deploy/compose.yml at the root of this repo checkout.
    pub compose_file: Option<String>,

    // Container tooling to drive the compose file with: "podman" (uses
    // podman-compose, matching uc push and uc pull) or "docker"
    // (uses `docker compose`).
    pub container_cli: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            api_base_urls: DEFAULT_API_BASE_URLS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            api_base_url: None,
            api_key: None,
            request_timeout_seconds: 10.0,
            api_verify_ssl: true,
            compose_file: None,
            container_cli: "podman".to_string(),
        }
    }
}

fn var(name: &str) -> Option<String> {
    match env::var(format!("USECODE_MCP_{name}")) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

/// A list given either as a JSON array (what pydantic-settings accepted) or as
/// a plain comma-separated string.
fn parse_list(raw: &str) -> Result<Vec<String>> {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') {
        return Ok(serde_json::from_str(trimmed)?);
    }
    Ok(trimmed
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

fn parse_bool(raw: &str) -> bool {
    !matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off"
    )
}

impl Settings {
    /// Load from `.env` (values already in the environment win) and then the
    /// environment itself. Unparsable values fall back to their default.
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();

        let mut settings = Settings::default();
        if let Some(raw) = var("API_BASE_URLS")
            && let Ok(urls) = parse_list(&raw)
        {
            settings.api_base_urls = urls;
        }
        settings.api_base_url = var("API_BASE_URL");
        settings.api_key = var("API_KEY");
        if let Some(raw) = var("REQUEST_TIMEOUT_SECONDS")
            && let Ok(seconds) = raw.trim().parse()
        {
            settings.request_timeout_seconds = seconds;
        }
        if let Some(raw) = var("API_VERIFY_SSL") {
            settings.api_verify_ssl = parse_bool(&raw);
        }
        settings.compose_file = var("COMPOSE_FILE");
        if let Some(raw) = var("CONTAINER_CLI") {
            settings.container_cli = raw;
        }
        settings
    }

    /// The base URLs to spread requests over, in configuration order.
    pub fn endpoints(&self) -> Vec<String> {
        match &self.api_base_url {
            Some(url) => vec![url.clone()],
            None => self.api_base_urls.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_parses_json_and_csv() {
        assert_eq!(parse_list(r#"["a", "b"]"#).unwrap(), vec!["a", "b"]);
        assert_eq!(parse_list("a, b").unwrap(), vec!["a", "b"]);
        assert!(parse_list("").unwrap().is_empty());
    }

    #[test]
    fn single_endpoint_replaces_the_list() {
        let mut settings = Settings::default();
        assert_eq!(settings.endpoints().len(), 2);
        settings.api_base_url = Some("https://example.test/api".to_string());
        assert_eq!(settings.endpoints(), vec!["https://example.test/api"]);
    }

    #[test]
    fn bool_reads_the_usual_spellings() {
        assert!(parse_bool("true"));
        assert!(parse_bool("1"));
        assert!(!parse_bool("False"));
        assert!(!parse_bool("off"));
    }
}
