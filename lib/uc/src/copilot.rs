// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc agent login copilot`, and GitHub Copilot as one of the coding agent's
//! model providers, next to OpenRouter and the Claude subscription.
//!
//! The login is GitHub's device flow with the client id Copilot's editor
//! plugins use: the user types a short code on github.com, and the GitHub
//! token that comes back is written as the `api_key` of the `copilot`
//! provider in `agent.yaml` (see [`crate::providers`]). That token is not what the chat sends: it is
//! traded for a Copilot token that lasts about half an hour, kept here and
//! traded again before it runs out (see [`token`]).
//!
//! Copilot speaks OpenAI's chat completions, like OpenRouter, so the chat's
//! request goes almost as it is: [`request`] only drops what is OpenRouter's
//! own (`reasoning`, `usage`, the reasoning handed back on each message).
//! Some models (OpenAI's newest) are only served through the Responses API:
//! when `/chat/completions` turns one away, the request goes to `/responses`
//! instead ([`responses_request`]) and the reply is turned back into the
//! chunks the chat reads ([`Translator`]).

use anyhow::{Context, Result, anyhow, bail};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::claude::Bytes;

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

/// `uc agent login copilot`: GitHub's device flow, then the token saved as
/// the key of the provider `name` (`copilot`). `--print` shows the token
/// instead of saving it.
pub async fn login(name: &str, print_only: bool) -> Result<()> {
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
        println!("{token}");
        return Ok(());
    }
    let path = crate::providers::save_key(name, crate::providers::Kind::Copilot, &token)?;
    eprintln!("Saved the token as provider {name} in {}", path.display());
    eprintln!("Run `uc` to chat on it.");
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

/// The chat's OpenRouter request as Copilot, and any plain OpenAI-style
/// API, takes it: without OpenRouter's own fields, and asking for the usage
/// at the end of a stream.
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

/// Posts the request to `path`, trying again after a pause on a network
/// error, 429 or 5xx. `None` is Copilot saying the model isn't served there.
async fn post(github: &str, path: &str, request: &Value) -> Result<Option<reqwest::Response>> {
    let mut attempt = 1;
    loop {
        let session = token(github).await?;
        let sent = client()
            .post(format!("{}{path}", session.api))
            .bearer_auth(&session.token)
            .headers(headers())
            .json(request)
            .send()
            .await;
        let (retry, error) = match sent {
            Err(e) => (true, anyhow::Error::new(e).context("calling Copilot")),
            Ok(response) if response.status().is_success() => return Ok(Some(response)),
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                if value["error"]["code"] == "unsupported_api_for_model" {
                    return Ok(None);
                }
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

/// The models Copilot only serves through `/responses`, learnt the first
/// time `/chat/completions` turns one away.
static RESPONSES_ONLY: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Where the request goes: the chat's own request to `/chat/completions`,
/// or, for a model that is only served there (OpenAI's newest), its
/// Responses form to `/responses`. The flag tells which one answered.
async fn open(github: &str, body: &Value) -> Result<(reqwest::Response, bool)> {
    let model = body["model"].as_str().unwrap_or_default().to_string();
    let known = RESPONSES_ONLY.lock().unwrap().contains(&model);
    if !known {
        if let Some(response) = post(github, "/chat/completions", &request(body)).await? {
            return Ok((response, false));
        }
        RESPONSES_ONLY.lock().unwrap().push(model.clone());
    }
    match post(github, "/responses", &responses_request(body)).await? {
        Some(response) => Ok((response, true)),
        None => bail!("Copilot serves {model} through neither /chat/completions nor /responses"),
    }
}

/// Posts the chat's streaming request; the reply is the SSE chunks the chat
/// reads, as they come from `/chat/completions` or turned into them.
pub async fn stream(github: &str, body: &Value) -> Result<Bytes> {
    let (response, responses) = open(github, body).await?;
    if !responses {
        return Ok(Box::pin(response.bytes_stream().map(|bytes| {
            bytes.map(|b| b.to_vec()).map_err(std::io::Error::other)
        })));
    }
    let mut upstream = response.bytes_stream();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<std::io::Result<Vec<u8>>>();
    tokio::spawn(async move {
        let mut translator = Translator::default();
        let mut pending: Vec<u8> = Vec::new();
        while let Some(bytes) = upstream.next().await {
            let bytes = match bytes {
                Ok(bytes) => bytes,
                Err(err) => {
                    let _ = tx.send(Err(std::io::Error::other(err)));
                    return;
                }
            };
            pending.extend_from_slice(&bytes);
            while let Some(end) = pending.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = pending.drain(..=end).collect();
                let line = String::from_utf8_lossy(&line);
                let Some(data) = line.trim().strip_prefix("data:").map(str::trim) else {
                    continue;
                };
                let Ok(event) = serde_json::from_str::<Value>(data) else {
                    continue;
                };
                for chunk in translator.event(&event) {
                    if tx
                        .send(Ok(format!("data: {chunk}\n").into_bytes()))
                        .is_err()
                    {
                        return;
                    }
                }
            }
        }
        let _ = tx.send(Ok(b"data: [DONE]\n".to_vec()));
    });
    Ok(Box::pin(futures_util::stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|item| (item, rx))
    })))
}

/// Posts a request that doesn't stream; the reply is OpenAI's.
pub async fn send(github: &str, body: &Value) -> Result<Value> {
    let mut body = body.clone();
    body["stream"] = json!(false);
    let (response, responses) = open(github, &body).await?;
    let value: Value = response.json().await.context("reading Copilot's reply")?;
    if !responses {
        return Ok(value);
    }
    let text: String = value["output"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "message")
        .flat_map(|item| item["content"].as_array().into_iter().flatten())
        .filter_map(|part| part["text"].as_str())
        .collect();
    Ok(json!({
        "choices": [{"message": {"role": "assistant", "content": text}}],
        "usage": usage(&value["usage"]),
    }))
}

// ------------------------------------------------------------ responses ----

/// The text of a message's content, be it a string or a list of parts.
fn text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect(),
        _ => String::new(),
    }
}

/// The chat's request in the form of OpenAI's Responses API, which is all
/// Copilot serves some models through: the messages become `input` items,
/// tool calls and their results items of their own.
fn responses_request(body: &Value) -> Value {
    let mut input: Vec<Value> = Vec::new();
    for message in body["messages"].as_array().into_iter().flatten() {
        let content = text(&message["content"]);
        match message["role"].as_str().unwrap_or_default() {
            role @ ("system" | "user") => input.push(json!({"role": role, "content": content})),
            "assistant" => {
                if !content.trim().is_empty() {
                    input.push(json!({"role": "assistant", "content": content}));
                }
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    input.push(json!({
                        "type": "function_call",
                        "call_id": call["id"],
                        "name": call["function"]["name"],
                        "arguments": call["function"]["arguments"].as_str().unwrap_or("{}"),
                    }));
                }
            }
            "tool" => input.push(json!({
                "type": "function_call_output",
                "call_id": message["tool_call_id"],
                "output": content,
            })),
            _ => {}
        }
    }
    let mut request = json!({
        "model": body["model"],
        "input": input,
        "stream": body["stream"] == true,
        "store": false,
    });
    if let Some(effort) = body["reasoning"]["effort"].as_str() {
        request["reasoning"] = json!({"effort": effort});
    }
    let tools: Vec<Value> = body["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| {
            json!({
                "type": "function",
                "name": t["function"]["name"],
                "description": t["function"]["description"],
                "parameters": t["function"]["parameters"],
            })
        })
        .collect();
    if !tools.is_empty() {
        request["tools"] = json!(tools);
    }
    request
}

/// An OpenAI chat `usage` out of a Responses one.
fn usage(u: &Value) -> Value {
    let input = u["input_tokens"].as_u64().unwrap_or_default();
    let output = u["output_tokens"].as_u64().unwrap_or_default();
    json!({
        "prompt_tokens": input,
        "completion_tokens": output,
        "total_tokens": input + output,
        "prompt_tokens_details": {
            "cached_tokens": u["input_tokens_details"]["cached_tokens"].as_u64().unwrap_or_default(),
        },
    })
}

/// A Responses stream, event by event, as the chunks of a chat one.
#[derive(Default)]
struct Translator {
    /// The tool calls so far, by the output index of the item they are.
    calls: Vec<u64>,
}

impl Translator {
    /// The chunks one event adds.
    fn event(&mut self, event: &Value) -> Vec<Value> {
        let chunk = |delta: Value| json!({"choices": [{"index": 0, "delta": delta}]});
        match event["type"].as_str().unwrap_or_default() {
            "response.output_text.delta" => vec![chunk(json!({"content": event["delta"]}))],
            "response.output_item.added" if event["item"]["type"] == "function_call" => {
                self.calls
                    .push(event["output_index"].as_u64().unwrap_or_default());
                vec![chunk(json!({"tool_calls": [{
                    "index": self.calls.len() - 1,
                    "id": event["item"]["call_id"],
                    "type": "function",
                    "function": {"name": event["item"]["name"], "arguments": ""},
                }]}))]
            }
            "response.function_call_arguments.delta" => {
                let index = event["output_index"].as_u64().unwrap_or_default();
                match self.calls.iter().position(|c| *c == index) {
                    Some(i) => vec![chunk(json!({"tool_calls": [{
                        "index": i,
                        "function": {"arguments": event["delta"]},
                    }]}))],
                    None => Vec::new(),
                }
            }
            "response.completed" | "response.incomplete" => vec![json!({
                "choices": [],
                "usage": usage(&event["response"]["usage"]),
            })],
            "response.failed" => vec![json!({"error": {
                "message": event["response"]["error"]["message"].as_str().unwrap_or("the response failed"),
            }})],
            "error" => vec![json!({"error": {
                "message": event["message"]
                    .as_str()
                    .or(event["error"]["message"].as_str())
                    .unwrap_or("no message"),
            }})],
            _ => Vec::new(),
        }
    }
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

    #[test]
    fn a_responses_request_has_items_for_the_calls() {
        let body = json!({
            "model": "gpt-6-luna",
            "messages": [
                {"role": "system", "content": "be uc"},
                {"role": "user", "content": "list"},
                {"role": "assistant", "content": null, "tool_calls": [
                    {"id": "c1", "type": "function",
                     "function": {"name": "bash", "arguments": "{\"command\":\"ls\"}"}}]},
                {"role": "tool", "tool_call_id": "c1", "content": [{"type": "text", "text": "a b"}]},
            ],
            "tools": [{"type": "function", "function": {
                "name": "bash", "description": "run", "parameters": {"type": "object"}}}],
            "reasoning": {"effort": "high"},
            "usage": {"include": true},
            "stream": true,
        });
        let r = responses_request(&body);
        assert_eq!(r["stream"], true);
        assert_eq!(r["store"], false);
        assert_eq!(r["reasoning"]["effort"], "high");
        assert!(r.get("usage").is_none());
        assert_eq!(r["tools"][0]["name"], "bash");
        assert_eq!(r["tools"][0]["parameters"]["type"], "object");
        let input = r["input"].as_array().unwrap();
        assert_eq!(input.len(), 4);
        assert_eq!(input[0]["role"], "system");
        assert_eq!(input[2]["type"], "function_call");
        assert_eq!(input[2]["call_id"], "c1");
        assert_eq!(input[2]["arguments"], "{\"command\":\"ls\"}");
        assert_eq!(input[3]["type"], "function_call_output");
        assert_eq!(input[3]["output"], "a b");
    }

    #[test]
    fn a_responses_stream_becomes_chat_chunks() {
        let mut t = Translator::default();
        let events = [
            json!({"type": "response.created"}),
            json!({"type": "response.output_text.delta", "output_index": 0, "delta": "hi"}),
            json!({"type": "response.output_item.added", "output_index": 1,
                   "item": {"type": "function_call", "call_id": "c1", "name": "bash"}}),
            json!({"type": "response.function_call_arguments.delta", "output_index": 1,
                   "delta": "{\"command\":\"ls\"}"}),
            json!({"type": "response.completed", "response": {"usage": {
                "input_tokens": 100, "input_tokens_details": {"cached_tokens": 90},
                "output_tokens": 7}}}),
        ];
        let chunks: Vec<Value> = events.iter().flat_map(|e| t.event(e)).collect();
        assert_eq!(chunks[0]["choices"][0]["delta"]["content"], "hi");
        let start = &chunks[1]["choices"][0]["delta"]["tool_calls"][0];
        assert_eq!(start["index"], 0);
        assert_eq!(start["id"], "c1");
        assert_eq!(start["function"]["name"], "bash");
        assert_eq!(
            chunks[2]["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"],
            "{\"command\":\"ls\"}"
        );
        let usage = &chunks[3]["usage"];
        assert_eq!(usage["prompt_tokens"], 100);
        assert_eq!(usage["total_tokens"], 107);
        assert_eq!(usage["prompt_tokens_details"]["cached_tokens"], 90);
        let failed = t.event(&json!({"type": "response.failed",
            "response": {"error": {"message": "boom"}}}));
        assert_eq!(failed[0]["error"]["message"], "boom");
    }
}
