// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Calling home: the host registering itself with usecode.dev (or your
//! own usecode API) and saying it's alive every few minutes, so it shows
//! up in the console with its version and how each module is doing.
//!
//! It is opt-in. `uc daemon install`, given a usecode.dev API key, asks
//! the API for a one-time enrollment token and delivers it as the bundle's
//! `callhome` section ([`Settings`], written to [`SETTINGS_PATH`]). The
//! daemon's callhome module ([`crate::daemon::callhome`]) registers with
//! it, keeps the host's own secret in [`STATE_PATH`] and drops the token.
//! The user's API key never goes on the host.
//!
//! What a host sends: the name it has in the inventory, a hash of its
//! machine id (so the same machine registering again is the same host, not
//! the id itself), the daemon's version, the architecture, the OS's name,
//! the uptime and each module's `ok` or error. Nothing from the mesh, the
//! firewall or Kubernetes' config.

use std::fs;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::error::{Context, Result};

/// Where the enrollment token waits for the daemon to register with it.
pub const SETTINGS_PATH: &str = "/etc/uc/callhome.toml";
/// Where the daemon keeps the host's id and secret once registered.
pub const STATE_DIR: &str = "/var/lib/usecode";
pub const STATE_PATH: &str = "/var/lib/usecode/callhome.toml";

/// The usecode API hosts call home to unless told otherwise.
pub const DEFAULT_ENDPOINT: &str = "https://api.usecode.dev";

/// What `uc daemon install` delivers: where to call and the token to
/// register with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub endpoint: String,
    pub token: String,
    /// The host's name in the inventory, what it is listed as.
    #[serde(default)]
    pub name: String,
}

impl Settings {
    /// The settings at `path`, if a token is waiting there.
    pub fn load(path: &str) -> Result<Option<Settings>> {
        let Some(body) = read(path)? else {
            return Ok(None);
        };
        let settings: Settings = toml::from_str(&body).with_ctx(|| format!("parse {path}"))?;
        Ok(Some(settings).filter(|s| !s.token.is_empty()))
    }

    pub fn encode(&self) -> Result<String> {
        let body = toml::to_string(self).ctx("encode callhome settings")?;
        Ok(format!(
            "# Written by `uc daemon install`: a one-time token for the usecode daemon\n\
             # to register this host with. The daemon removes it once it has.\n\n{body}"
        ))
    }
}

/// Put the delivered settings in place; whether anything changed.
pub fn accept(settings: &Settings) -> Result<bool> {
    crate::daemon::host::put(SETTINGS_PATH, settings.encode()?.as_bytes(), 0o600)
}

/// A registered host: who it is, and the secret it calls with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub endpoint: String,
    pub id: String,
    pub secret: String,
    /// How often the API wants to hear from the host.
    pub heartbeat_seconds: u64,
}

impl State {
    pub fn load(path: &str) -> Result<Option<State>> {
        let Some(body) = read(path)? else {
            return Ok(None);
        };
        let state: State = toml::from_str(&body).with_ctx(|| format!("parse {path}"))?;
        Ok(Some(state).filter(|s| !s.secret.is_empty()))
    }

    pub fn encode(&self) -> Result<String> {
        let body = toml::to_string(self).ctx("encode callhome state")?;
        Ok(format!(
            "# The usecode daemon's registration with {}. Remove the host in the\n\
             # console (or this file) to stop it calling home.\n\n{body}",
            self.endpoint
        ))
    }

    /// The heartbeat interval, kept between a minute and an hour whatever
    /// the API says.
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.heartbeat_seconds.clamp(60, 3600))
    }
}

fn read(path: &str) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(body) => Ok(Some(body)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => bail!("read {path}: {e}"),
    }
}

/// What the host says about itself when it registers.
#[derive(Debug, Clone, PartialEq)]
pub struct Facts {
    pub name: String,
    pub machine_id: String,
    pub version: String,
    pub arch: String,
    pub os: String,
}

impl Facts {
    /// This host's facts; `name` is what it is called in the inventory,
    /// the hostname when that is empty.
    pub fn gather(name: &str) -> Result<Facts> {
        let machine_id = ["/etc/machine-id", "/var/lib/dbus/machine-id"]
            .iter()
            .find_map(|p| fs::read_to_string(p).ok())
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .ok_or_else(|| err!("no /etc/machine-id to tell this host apart by"))?;
        let name = match name.trim() {
            "" => fs::read_to_string("/proc/sys/kernel/hostname")
                .unwrap_or_default()
                .trim()
                .to_string(),
            name => name.to_string(),
        };
        Ok(Facts {
            name,
            machine_id: hashed_machine_id(&machine_id),
            version: env!("CARGO_PKG_VERSION").to_string(),
            arch: std::env::consts::ARCH.to_string(),
            os: fs::read_to_string("/etc/os-release")
                .map(|body| os_name(&body))
                .unwrap_or_default(),
        })
    }
}

/// The machine id as it leaves the host: hashed, so the API can tell a
/// machine registering again from a new one without learning the id.
fn hashed_machine_id(id: &str) -> String {
    Sha256::digest(format!("usecode:{id}").as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `PRETTY_NAME` from os-release, else `NAME`.
fn os_name(os_release: &str) -> String {
    let field = |key: &str| {
        os_release.lines().find_map(|line| {
            let value = line.strip_prefix(key)?.strip_prefix('=')?;
            Some(value.trim().trim_matches('"').to_string())
        })
    };
    field("PRETTY_NAME")
        .or_else(|| field("NAME"))
        .unwrap_or_default()
}

/// Seconds since the host booted.
pub fn uptime() -> Option<i64> {
    let body = fs::read_to_string("/proc/uptime").ok()?;
    let secs: f64 = body.split_whitespace().next()?.parse().ok()?;
    Some(secs as i64)
}

/// An HTTP client for the usecode API, on the pure-Rust TLS provider.
pub fn client() -> Result<reqwest::blocking::Client> {
    let _ = rustls_graviola::default_provider().install_default();
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("usecoded/", env!("CARGO_PKG_VERSION")))
        .build()
        .ctx("make an HTTP client")
}

/// How a call to the API went.
pub enum Answer<T> {
    Ok(T),
    /// The API turned the credential down (401): an enrollment token that
    /// was used or ran out, or a host that was removed.
    Refused(String),
}

fn url(endpoint: &str, path: &str) -> String {
    format!("{}{path}", endpoint.trim_end_matches('/'))
}

/// POST `body` and read the JSON answer; a 401 is [`Answer::Refused`].
fn post(
    client: &reqwest::blocking::Client,
    url: &str,
    auth: (&str, &str),
    body: &Value,
) -> Result<Answer<Value>> {
    let response = client
        .post(url)
        .header(auth.0, auth.1)
        .json(body)
        .send()
        .with_ctx(|| format!("reach {url}"))?;
    let status = response.status();
    let answer: Value = response.json().unwrap_or(Value::Null);
    let detail = answer["detail"].as_str().unwrap_or_default().to_string();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Ok(Answer::Refused(detail));
    }
    if !status.is_success() {
        bail!("{url}: {status} {detail}");
    }
    Ok(Answer::Ok(answer))
}

/// Control node: a one-time enrollment token for a host, paid for with the
/// user's API key.
pub fn enroll(endpoint: &str, api_key: &str) -> Result<String> {
    let client = client()?;
    match post(
        &client,
        &url(endpoint, "/hosts/enrollments"),
        ("x-api-key", api_key),
        &json!({}),
    )? {
        Answer::Ok(answer) => answer["token"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| err!("{endpoint} gave no enrollment token")),
        Answer::Refused(detail) => bail!("{endpoint} turned the API key down: {detail}"),
    }
}

/// Host: register with the delivered token.
pub fn register(settings: &Settings, facts: &Facts) -> Result<Answer<State>> {
    let client = client()?;
    let body = json!({
        "token": settings.token,
        "name": facts.name,
        "machine_id": facts.machine_id,
        "version": facts.version,
        "arch": facts.arch,
        "os": facts.os,
    });
    let answer = post(
        &client,
        &url(&settings.endpoint, "/hosts/register"),
        ("x-uc-host", &facts.name),
        &body,
    )?;
    Ok(match answer {
        Answer::Refused(detail) => Answer::Refused(detail),
        Answer::Ok(answer) => Answer::Ok(State {
            endpoint: settings.endpoint.clone(),
            id: answer["id"].as_str().unwrap_or_default().to_string(),
            secret: answer["secret"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| err!("{} gave no host secret", settings.endpoint))?
                .to_string(),
            heartbeat_seconds: answer["heartbeat_seconds"].as_u64().unwrap_or(300),
        }),
    })
}

/// How one module did, for the heartbeat.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModuleStatus {
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Host: say it's alive. The answer is how often the API wants to hear
/// from it.
pub fn heartbeat(state: &State, modules: &[ModuleStatus]) -> Result<Answer<u64>> {
    let client = client()?;
    let body = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": uptime(),
        "modules": modules,
    });
    let auth = format!("Bearer {}", state.secret);
    Ok(
        match post(
            &client,
            &url(&state.endpoint, "/hosts/heartbeat"),
            ("authorization", &auth),
            &body,
        )? {
            Answer::Refused(detail) => Answer::Refused(detail),
            Answer::Ok(answer) => Answer::Ok(
                answer["heartbeat_seconds"]
                    .as_u64()
                    .unwrap_or(state.heartbeat_seconds),
            ),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_os_is_its_pretty_name() {
        let body = "NAME=\"Debian GNU/Linux\"\nPRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\n";
        assert_eq!(os_name(body), "Debian GNU/Linux 13 (trixie)");
        assert_eq!(os_name("NAME=Alpine\n"), "Alpine");
        assert_eq!(os_name(""), "");
    }

    #[test]
    fn the_machine_id_leaves_hashed() {
        let id = hashed_machine_id("0123456789abcdef0123456789abcdef");
        assert_eq!(id.len(), 64);
        assert!(!id.contains("0123456789abcdef"));
        assert_eq!(id, hashed_machine_id("0123456789abcdef0123456789abcdef"));
    }

    #[test]
    fn settings_and_state_round_trip() {
        let settings = Settings {
            endpoint: DEFAULT_ENDPOINT.into(),
            token: "1a2b.t".into(),
            name: "edge".into(),
        };
        let back: Settings = toml::from_str(&settings.encode().unwrap()).unwrap();
        assert_eq!(back, settings);

        let state = State {
            endpoint: DEFAULT_ENDPOINT.into(),
            id: "id".into(),
            secret: "1a2b.s".into(),
            heartbeat_seconds: 5,
        };
        let back: State = toml::from_str(&state.encode().unwrap()).unwrap();
        assert_eq!(back, state);
        assert_eq!(back.interval(), Duration::from_secs(60));
    }

    /// Against a running usecode API, when UC_CALLHOME_TEST_API and
    /// UC_CALLHOME_TEST_KEY (an API key there) are set; skipped otherwise.
    #[test]
    fn against_a_live_api() {
        let (Ok(endpoint), Ok(key)) = (
            std::env::var("UC_CALLHOME_TEST_API"),
            std::env::var("UC_CALLHOME_TEST_KEY"),
        ) else {
            return;
        };
        let settings = Settings {
            token: enroll(&endpoint, &key).unwrap(),
            endpoint: endpoint.clone(),
            name: "e2e".into(),
        };
        let facts = Facts {
            name: "e2e".into(),
            machine_id: hashed_machine_id("e2e"),
            version: env!("CARGO_PKG_VERSION").into(),
            arch: std::env::consts::ARCH.into(),
            os: "test".into(),
        };
        let Answer::Ok(state) = register(&settings, &facts).unwrap() else {
            panic!("a fresh token was turned down");
        };
        assert!(matches!(
            register(&settings, &facts).unwrap(),
            Answer::Refused(_)
        ));
        let modules = [ModuleStatus {
            name: "mesh".into(),
            status: "ok".into(),
            detail: None,
        }];
        let Answer::Ok(seconds) = heartbeat(&state, &modules).unwrap() else {
            panic!("the new secret was turned down");
        };
        assert!(seconds > 0);

        let removed = client()
            .unwrap()
            .delete(url(&endpoint, &format!("/hosts/{}", state.id)))
            .header("x-api-key", &key)
            .send()
            .unwrap();
        assert!(removed.status().is_success());
        assert!(matches!(
            heartbeat(&state, &modules).unwrap(),
            Answer::Refused(_)
        ));
    }

    #[test]
    fn a_missing_file_is_nothing_to_do() {
        assert_eq!(Settings::load("/nonexistent/callhome.toml").unwrap(), None);
        assert_eq!(State::load("/nonexistent/callhome.toml").unwrap(), None);
    }
}
