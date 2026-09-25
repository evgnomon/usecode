// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Settings, read from `USECODE_AGENT_*` environment variables (and a `.env`
//! file in the working directory, which never overrides the environment).

use std::collections::BTreeMap;
use std::env;

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub struct Settings {
    // -- Instance identity -------------------------------------------------
    // Name of *this* API instance, e.g. "api-1" or "worker-1". Required: the
    // process refuses to start without it, because a task's assignee is a
    // node name (see `schema::TASKS`) — and the assignee is also the task's
    // address, the key `tasks` is partitioned on. An unnamed instance could
    // neither claim work nor say where its work is stored, so running one
    // would silently strand tasks.
    pub node_name: String,

    // Address the HTTP server listens on.
    pub bind: String,

    pub otp_length: usize,
    pub otp_ttl_seconds: i64,
    pub otp_resend_cooldown_seconds: i64,
    pub otp_max_attempts: i32,

    // When true, the OTP code is echoed back in the request-otp response
    // instead of requiring a real SMS provider. Meant for local development
    // only.
    pub debug_expose_otp: bool,

    // Optional generic HTTP SMS provider. Left unset, the server just logs
    // the code.
    pub sms_api_url: Option<String>,
    pub sms_api_key: Option<String>,
    pub sms_sender_name: String,

    pub cors_origins: Vec<String>,

    // -- Databases ---------------------------------------------------------
    // Connection string for the **main/first** PostgreSQL instance — the one
    // a null partition key resolves to, and the one holding the
    // `shard_ranges` table that says which instance owns which virtual
    // shards. The SQLAlchemy spelling `postgresql+asyncpg://` is accepted
    // for compatibility with existing deployments.
    pub database_url: String,

    // The *other* database instances, keyed by partition key, as
    // "host:port/database" — e.g. {"b": "postgres-2:5432/usecode_agent"}.
    // Every shard runs the identical schema under the identical role, so
    // the driver and credentials are reused from `database_url`. This is
    // bootstrap configuration: it is what the `shard_ranges` table on the
    // main database is seeded from the *first* time a deployment boots and
    // what migrations are applied to. At runtime, resolution reads that
    // table, not this setting.
    pub shards: BTreeMap<String, String>,

    // Container tooling used to run the AI model (llama-server) container:
    // "podman" (default) or "docker".
    pub model_container_cli: String,

    // Used to encrypt per-user secrets (e.g. Hetzner API tokens) at rest.
    // Change this in production and keep it stable, rotating it invalidates
    // every stored secret.
    pub secret_key: String,
}

const PREFIX: &str = "USECODE_AGENT_";

fn var(name: &str) -> Option<String> {
    env::var(format!("{PREFIX}{name}")).ok()
}

fn non_empty(name: &str) -> Option<String> {
    var(name).filter(|value| !value.trim().is_empty())
}

fn parsed<T: std::str::FromStr>(name: &str, default: T) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    match non_empty(name) {
        None => Ok(default),
        Some(raw) => raw
            .trim()
            .parse()
            .map_err(|err| anyhow::anyhow!("{PREFIX}{name}={raw:?} is invalid: {err}")),
    }
}

fn parse_bool(name: &str, default: bool) -> Result<bool> {
    let Some(raw) = non_empty(name) else {
        return Ok(default);
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "t" | "yes" | "y" | "on" => Ok(true),
        "0" | "false" | "f" | "no" | "n" | "off" => Ok(false),
        _ => bail!("{PREFIX}{name}={raw:?} is not a boolean"),
    }
}

fn parse_json<T: serde::de::DeserializeOwned>(name: &str, default: T) -> Result<T> {
    match non_empty(name) {
        None => Ok(default),
        Some(raw) => serde_json::from_str(&raw)
            .with_context(|| format!("{PREFIX}{name} must be JSON, got {raw:?}")),
    }
}

impl Settings {
    pub fn from_env() -> Result<Self> {
        // Values already in the environment win over the file.
        let _ = dotenvy::from_filename(".env");

        // Enforced as non-empty rather than merely present, because
        // USECODE_AGENT_NODE_NAME="" would otherwise be just as unusable as
        // an absent one: every such node's tasks would be assigned to "" and
        // would all hash to the same bucket, so the nodes would sweep each
        // other's work.
        let node_name = var("NODE_NAME").unwrap_or_default().trim().to_string();
        if node_name.is_empty() {
            bail!(
                "USECODE_AGENT_NODE_NAME must be set to a non-empty node name, e.g. 'api-1'. \
                 A task's assignee is a node name and is what addresses the task, so an \
                 unnamed node could neither claim work nor find it again."
            );
        }

        Ok(Self {
            node_name,
            bind: non_empty("BIND").unwrap_or_else(|| "0.0.0.0:8000".to_string()),
            otp_length: parsed("OTP_LENGTH", 6)?,
            otp_ttl_seconds: parsed("OTP_TTL_SECONDS", 300)?,
            otp_resend_cooldown_seconds: parsed("OTP_RESEND_COOLDOWN_SECONDS", 60)?,
            otp_max_attempts: parsed("OTP_MAX_ATTEMPTS", 5)?,
            debug_expose_otp: parse_bool("DEBUG_EXPOSE_OTP", true)?,
            sms_api_url: non_empty("SMS_API_URL"),
            sms_api_key: non_empty("SMS_API_KEY"),
            sms_sender_name: var("SMS_SENDER_NAME").unwrap_or_else(|| "usecode agent".to_string()),
            cors_origins: parse_json(
                "CORS_ORIGINS",
                vec![
                    "http://localhost:5173".to_string(),
                    "http://127.0.0.1:5173".to_string(),
                ],
            )?,
            database_url: non_empty("DATABASE_URL").unwrap_or_else(|| {
                "postgresql://usecode_agent:usecode_agent@localhost:5432/usecode_agent".to_string()
            }),
            shards: parse_json("SHARDS", BTreeMap::new())?,
            model_container_cli: non_empty("MODEL_CONTAINER_CLI")
                .unwrap_or_else(|| "podman".to_string()),
            secret_key: var("SECRET_KEY")
                .unwrap_or_else(|| "dev-insecure-secret-key-change-me".to_string()),
        })
    }
}
