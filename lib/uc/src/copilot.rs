// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc agent login copilot`, and GitHub Copilot as one of the coding agent's
//! model providers, next to OpenRouter and the Claude subscription.
//!
//! The login is GitHub's device flow with the client id Copilot's editor
//! plugins use: the user types a short code on github.com, and the GitHub
//! token that comes back is written to `~/.bashrc.d/copilot.sh` as
//! `COPILOT_GITHUB_TOKEN`. That token is not what the chat sends: it is
//! traded for a Copilot token that lasts about half an hour, kept here and
//! traded again before it runs out (see [`token`]).
//!
//! Copilot speaks OpenAI's chat completions, like OpenRouter, so the chat's
//! request goes almost as it is: [`request`] only drops what is OpenRouter's
//! own (`reasoning`, `usage`, the reasoning handed back on each message).

use anyhow::{Context, Result, anyhow, bail};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::claude::Bytes;

/// The variable the GitHub token lives in.
pub const TOKEN_VAR: &str = "COPILOT_GITHUB_TOKEN";

/// The OAuth app of Copilot's editor plugins: its tokens are the ones the
/// Copilot token endpoint takes.
const CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const DEVICE_URL: &str = "https://github.com/login/device/code";
const ACCESS_URL: &str = "https://github.com/login/oauth/access_token";
const TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
/// Where the chat goes when the token doesn't name an endpoint.
const DEFAULT_API: &str = "https://api.githubcopilot.com";
/// Copilot only answers what looks like one of its editors.
const HEADERS: &[(&str, &str)] = &[
    ("editor-version", "vscode/1.99.0"),
    ("editor-plugin-version", "copilot-chat/0.26.0"),
    ("copilot-integration-id", "vscode-chat"),
    ("user-agent", "GitHubCopilotChat/0.26.0"),
    ("openai-intent", "conversation-panel"),
];

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        let _ = rustls_graviola::default_provider().install_default();
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(600))
            .build()
            .expect("HTTP client")
    })
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

// ---------------------------------------------------------------- login ----

/// Where the token is saved: next to the other keys.
pub fn env_file() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?;
    Ok(PathBuf::from(home).join(".bashrc.d/copilot.sh"))
}

/// Writes the token as an `export` the shell sources, readable by the user
/// only.
pub fn save(token: &str) -> Result<PathBuf> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let path = env_file()?;
    std::fs::create_dir_all(path.parent().expect("a parent"))?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .with_context(|| format!("writing {}", path.display()))?;
    // `mode` only applies to a file that is new.
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    writeln!(
        file,
        "# GitHub token for Copilot in `uc` chat (uc agent login copilot)\nexport {TOKEN_VAR}={token}"
    )?;
    Ok(path)
}

/// `fields` as a URL-encoded form.
fn encode(fields: &[(&str, &str)]) -> String {
    let part = |s: &str| {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect::<String>()
    };
    fields
        .iter()
        .map(|(k, v)| format!("{}={}", part(k), part(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Posts a form to GitHub's login and reads the JSON it answers with.
async fn form(url: &str, fields: &[(&str, &str)]) -> Result<Value> {
    client()
        .post(url)
        .header("accept", "application/json")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(encode(fields))
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context("calling GitHub")?
        .error_for_status()?
        .json()
        .await
        .context("reading GitHub's reply")
}

/// `uc agent login copilot`: GitHub's device flow, then the token saved.
/// `--print` shows the token instead of saving it.
pub async fn login(print_only: bool) -> Result<()> {
    let device = form(
        DEVICE_URL,
        &[("client_id", CLIENT_ID), ("scope", "read:user")],
    )
    .await?;
    let (Some(code), Some(user_code)) =
        (device["device_code"].as_str(), device["user_code"].as_str())
    else {
        bail!("GitHub gave no device code: {device}");
    };
    let uri = device["verification_uri"]
        .as_str()
        .unwrap_or("https://github.com/login/device");
    eprintln!("Open {uri} in a browser and enter the code {user_code}");
    let _ = std::process::Command::new("xdg-open")
        .arg(uri)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let mut interval = device["interval"].as_u64().unwrap_or(5).max(1);
    let until = now() + device["expires_in"].as_u64().unwrap_or(900);
    let token = loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        if now() > until {
            bail!("the code ran out before it was entered; run the login again");
        }
        let reply = form(
            ACCESS_URL,
            &[
                ("client_id", CLIENT_ID),
                ("device_code", code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ],
        )
        .await?;
        if let Some(token) = reply["access_token"].as_str() {
            break token.to_string();
        }
        match reply["error"].as_str() {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            Some(other) => bail!(
                "GitHub: {other}: {}",
                reply["error_description"]
                    .as_str()
                    .unwrap_or("no description")
            ),
            None => bail!("GitHub sent no token: {reply}"),
        }
    };
    // A GitHub account without Copilot gets its token, but no Copilot one.
    copilot_token(&token)
        .await
        .context("the login worked, but this GitHub account has no Copilot")?;
    if print_only {
        println!("export {TOKEN_VAR}={token}");
        return Ok(());
    }
    let path = save(&token)?;
    eprintln!("Saved {TOKEN_VAR} to {}", path.display());
    eprintln!(
        "Open a new shell, or `source {}`, and run `uc`.",
        path.display()
    );
    Ok(())
}

// ------------------------------------------------------------- requests ----

/// A Copilot token, where it is to be used, and when it runs out.
#[derive(Clone)]
struct Session {
    token: String,
    api: String,
    expires_at: u64,
}

/// Trades the GitHub token for a Copilot one.
async fn copilot_token(github: &str) -> Result<Session> {
    let response = client()
        .get(TOKEN_URL)
        .header("authorization", format!("token {github}"))
        .header("accept", "application/json")
        .headers(headers())
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context("calling GitHub for a Copilot token")?;
    let status = response.status();
    let value: Value = response.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let msg = value["message"].as_str().unwrap_or("no message");
        bail!("Copilot token: {status}: {msg} (log in again: uc agent login copilot)");
    }
    Ok(Session {
        token: value["token"]
            .as_str()
            .ok_or_else(|| anyhow!("GitHub sent no Copilot token"))?
            .to_string(),
        api: value["endpoints"]["api"]
            .as_str()
            .unwrap_or(DEFAULT_API)
            .trim_end_matches('/')
            .to_string(),
        expires_at: value["expires_at"].as_u64().unwrap_or(now() + 600),
    })
}

/// The Copilot token for `github`, traded again a minute before it runs out.
async fn token(github: &str) -> Result<Session> {
    static KEPT: Mutex<Option<(String, Session)>> = Mutex::new(None);
    if let Some((for_github, session)) = KEPT.lock().unwrap().as_ref()
        && for_github == github
        && session.expires_at > now() + 60
    {
        return Ok(session.clone());
    }
    let session = copilot_token(github).await?;
    *KEPT.lock().unwrap() = Some((github.to_string(), session.clone()));
    Ok(session)
}

fn headers() -> reqwest::header::HeaderMap {
    HEADERS
        .iter()
        .map(|(k, v)| {
            (
                reqwest::header::HeaderName::from_static(k),
                reqwest::header::HeaderValue::from_static(v),
            )
        })
        .collect()
}

/// The chat models the account may pick in Copilot, by their Copilot ids.
pub async fn models(github: &str) -> Result<Vec<String>> {
    let session = token(github).await?;
    let list: Value = client()
        .get(format!("{}/models", session.api))
        .bearer_auth(&session.token)
        .headers(headers())
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .context("calling Copilot")?
        .error_for_status()?
        .json()
        .await
        .context("reading Copilot's model list")?;
    Ok(chat_models(&list))
}

/// The ids in a `/models` reply that can chat with tools and are offered in
/// the model picker, each once, Copilot's default chat model first.
fn chat_models(list: &Value) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for model in list["data"].as_array().into_iter().flatten() {
        let caps = &model["capabilities"];
        if caps["type"] != "chat"
            || model["model_picker_enabled"] == false
            || caps["supports"]["tool_calls"] == false
            || model["policy"]["state"]
                .as_str()
                .is_some_and(|s| s != "enabled")
        {
            continue;
        }
        if let Some(id) = model["id"]
            .as_str()
            .filter(|id| !ids.iter().any(|i| i == id))
        {
            if model["is_chat_default"] == true {
                ids.insert(0, id.to_string());
            } else {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

/// The chat's OpenRouter request as Copilot takes it: without OpenRouter's
/// own fields, and asking for the usage at the end of a stream.
pub fn request(body: &Value) -> Value {
    let mut request = body.clone();
    let object = request.as_object_mut().expect("a request is an object");
    object.remove("reasoning");
    object.remove("usage");
    if body["stream"] == true {
        object.insert("stream_options".into(), json!({"include_usage": true}));
    }
    for message in object
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if let Some(message) = message.as_object_mut() {
            message.remove("reasoning_details");
        }
    }
    request
}

/// How many times a request is tried, as for OpenRouter.
const ATTEMPTS: u32 = 4;

/// Posts the request, trying again after a pause on a network error, 429 or
/// 5xx.
async fn post(github: &str, request: &Value) -> Result<reqwest::Response> {
    let mut attempt = 1;
    loop {
        let session = token(github).await?;
        let sent = client()
            .post(format!("{}/chat/completions", session.api))
            .bearer_auth(&session.token)
            .headers(headers())
            .json(request)
            .send()
            .await;
        let (retry, error) = match sent {
            Err(e) => (true, anyhow::Error::new(e).context("calling Copilot")),
            Ok(response) if response.status().is_success() => return Ok(response),
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                let msg = value["error"]["message"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| text.trim().chars().take(300).collect());
                let retry = status.as_u16() == 429 || status.is_server_error();
                (retry, anyhow!("Copilot: {status}: {msg}"))
            }
        };
        if !retry || attempt == ATTEMPTS {
            return Err(error);
        }
        eprintln!("\x1b[2m· {error:#}; trying again\x1b[0m");
        tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
        attempt += 1;
    }
}

/// Posts the chat's streaming request; the reply is already the SSE chunks
/// the chat reads.
pub async fn stream(github: &str, body: &Value) -> Result<Bytes> {
    let response = post(github, &request(body)).await?;
    Ok(Box::pin(response.bytes_stream().map(|bytes| {
        bytes.map(|b| b.to_vec()).map_err(std::io::Error::other)
    })))
}

/// Posts a request that doesn't stream; the reply is OpenAI's.
pub async fn send(github: &str, body: &Value) -> Result<Value> {
    post(github, &request(body))
        .await?
        .json()
        .await
        .context("reading Copilot's reply")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_is_url_encoded() {
        assert_eq!(
            encode(&[("scope", "read:user"), ("grant_type", "a b")]),
            "scope=read%3Auser&grant_type=a%20b"
        );
    }

    #[test]
    fn request_drops_what_is_openrouters() {
        let body = json!({
            "model": "gpt-4.1",
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": "yo", "reasoning_details": [{"type": "x"}]},
            ],
            "reasoning": {"effort": "high"},
            "usage": {"include": true},
            "stream": true,
        });
        let request = request(&body);
        assert!(request.get("reasoning").is_none());
        assert!(request.get("usage").is_none());
        assert_eq!(request["stream_options"]["include_usage"], true);
        assert!(request["messages"][1].get("reasoning_details").is_none());
        assert_eq!(request["messages"][1]["content"], "yo");
        assert!(
            super::request(&json!({"messages": []}))
                .get("stream_options")
                .is_none()
        );
    }

    #[test]
    fn chat_models_are_the_pickable_ones_with_tools() {
        let list = json!({"data": [
            {"id": "gpt-4.1", "model_picker_enabled": true,
             "capabilities": {"type": "chat", "supports": {"tool_calls": true}}},
            {"id": "gpt-4.1", "model_picker_enabled": true,
             "capabilities": {"type": "chat", "supports": {"tool_calls": true}}},
            {"id": "claude-opus-5.5", "model_picker_enabled": true,
             "capabilities": {"type": "chat", "supports": {"tool_calls": true}},
             "policy": {"state": "enabled"}},
            {"id": "gpt-5-mini", "model_picker_enabled": true, "is_chat_default": true,
             "capabilities": {"type": "chat"}},
            {"id": "text-embedding-3-small", "capabilities": {"type": "embeddings"}},
            {"id": "gpt-4o-mini", "model_picker_enabled": false,
             "capabilities": {"type": "chat"}},
            {"id": "o1", "model_picker_enabled": true,
             "capabilities": {"type": "chat", "supports": {"tool_calls": false}}},
            {"id": "gemini-9", "model_picker_enabled": true,
             "capabilities": {"type": "chat"}, "policy": {"state": "disabled"}},
        ]});
        assert_eq!(
            chat_models(&list),
            ["gpt-5-mini", "gpt-4.1", "claude-opus-5.5"]
        );
    }
}
