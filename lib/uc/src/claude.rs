// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc agent login claude`, and the Claude subscription as the coding
//! agent's other model provider next to OpenRouter.
//!
//! The login is the OAuth flow Claude Code itself uses (`claude setup-token`):
//! authorization code with PKCE, the user pastes the code the browser shows,
//! and the token that comes back, good for a year and for inference only, is
//! written as the `api_key` of the `claude` provider in `agent.yaml`, next to
//! the other providers' keys (see [`crate::providers`]).
//!
//! The chat speaks OpenAI's chat completions, which is what OpenRouter
//! takes. Anthropic's Messages API is not that, so [`stream`] and [`send`]
//! turn the chat's request into one and the reply back into the chunks the
//! chat already reads. A subscription token is only taken by the Messages
//! API when the request looks like Claude Code's: the beta header, the
//! Claude Code identity as the first system block, and a few tool names
//! spelled the way Claude Code spells them (see [`wire`]).

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use futures_util::{Stream, StreamExt};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::{BufRead, IsTerminal, Write};
use std::pin::Pin;
use std::sync::OnceLock;
use std::time::Duration;

/// What a login is saved as in `agent.yaml`, and how a 401 says to fix it.
const RELOGIN: &str = " (log in again: uc agent login claude)";

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const AUTHORIZE_URL: &str = "https://claude.com/cai/oauth/authorize";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const REDIRECT_URI: &str = "https://platform.claude.com/oauth/code/callback";
/// Inference only: all the chat needs, and all a long-lived token should hold.
const SCOPE: &str = "user:inference";
/// A year, the lifetime `claude setup-token` asks for.
const LIFETIME: u64 = 365 * 24 * 60 * 60;
const MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages?beta=true";
const IDENTITY: &str = "You are Claude Code, Anthropic's official CLI for Claude.";
const MAX_TOKENS: u64 = 32_000;

/// Tool names the subscription API refuses unless they are Claude Code's.
const NAMES: &[(&str, &str)] = &[("bash", "Bash"), ("grep", "Grep"), ("glob", "Glob")];

/// The name of a tool on the wire.
fn wire(name: &str) -> &str {
    NAMES
        .iter()
        .find(|(ours, _)| *ours == name)
        .map_or(name, |(_, theirs)| theirs)
}

/// The chat's name of a tool the wire called `name`.
fn unwire(name: &str) -> &str {
    NAMES
        .iter()
        .find(|(_, theirs)| *theirs == name)
        .map_or(name, |(ours, _)| ours)
}

/// The Anthropic id of a model the chat names the OpenRouter way:
/// `anthropic/claude-opus-5.5` is `claude-opus-5-5`.
pub fn model_id(model: &str) -> String {
    model
        .strip_prefix("anthropic/")
        .unwrap_or(model)
        .replace('.', "-")
}

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

// ---------------------------------------------------------------- login ----

/// A PKCE verifier and its S256 challenge.
fn pkce() -> Result<(String, String)> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| anyhow!("{e}"))?;
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    Ok((verifier, challenge))
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The page the user logs in on. `state` is the verifier, as Claude Code
/// does, so the code pasted back carries it for the exchange.
fn authorize_url(challenge: &str, state: &str) -> String {
    format!(
        "{AUTHORIZE_URL}?code=true&client_id={CLIENT_ID}&response_type=code&redirect_uri={}\
         &scope={}&code_challenge={challenge}&code_challenge_method=S256&state={state}",
        encode(REDIRECT_URI),
        encode(SCOPE),
    )
}

/// The code and state out of what the user pasted: the bare `code#state` the
/// page shows, or a URL or query string with `code=` in it.
fn parse_code(input: &str) -> Result<(String, Option<String>)> {
    let input = input.trim();
    let (mut code, mut state) = (input.to_string(), None);
    if let Some((_, query)) = input
        .split_once('?')
        .or_else(|| input.contains("code=").then_some(("", input)))
    {
        for pair in query.split('&') {
            match pair.split_once('=') {
                Some(("code", v)) => code = v.to_string(),
                Some(("state", v)) => state = Some(v.to_string()),
                _ => {}
            }
        }
    }
    if let Some((c, s)) = code.clone().split_once('#') {
        code = c.to_string();
        state = Some(s.to_string());
    }
    if code.is_empty() {
        bail!("no authorization code in what was pasted");
    }
    Ok((code, state))
}

/// What the token endpoint gave back.
#[derive(Debug)]
pub struct Token {
    pub access: String,
    pub expires_in: u64,
    pub scope: String,
}

/// Trades the code for a token.
async fn exchange(verifier: &str, input: &str) -> Result<Token> {
    let (code, state) = parse_code(input)?;
    if state.as_deref().is_some_and(|s| s != verifier) {
        bail!("the code is from another login (state mismatch); run the login again");
    }
    let reply = client()
        .post(TOKEN_URL)
        .json(&json!({
            "grant_type": "authorization_code",
            "code": code,
            "redirect_uri": REDIRECT_URI,
            "client_id": CLIENT_ID,
            "code_verifier": verifier,
            "state": verifier,
            "expires_in": LIFETIME,
        }))
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context("calling Anthropic's token endpoint")?;
    let status = reply.status();
    let text = reply.text().await.context("reading the token reply")?;
    if !status.is_success() {
        bail!("token exchange failed: HTTP {status}: {text}");
    }
    let value: Value = serde_json::from_str(&text).context("reading the token reply")?;
    token_from(&value)
}

fn token_from(value: &Value) -> Result<Token> {
    let access = value["access_token"]
        .as_str()
        .ok_or_else(|| anyhow!("the token reply had no access_token"))?
        .to_string();
    let scope = value["scope"].as_str().unwrap_or_default().to_string();
    if !scope.is_empty() && !scope.split_whitespace().any(|s| s == "user:inference") {
        bail!("the token has no user:inference scope (it has: {scope})");
    }
    Ok(Token {
        access,
        expires_in: value["expires_in"].as_u64().unwrap_or_default(),
        scope,
    })
}

/// `uc agent login claude`: log in with the Claude subscription and save the
/// token as the key of the provider `name` (`claude`). `--print` shows the
/// token instead of saving it.
pub async fn login(name: &str, print_only: bool) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        bail!("the login needs a terminal to paste the code into");
    }
    let (verifier, challenge) = pkce()?;
    let url = authorize_url(&challenge, &verifier);
    eprintln!("Open this URL in a browser and log in with your Claude account:\n\n{url}\n");
    let _ = std::process::Command::new("xdg-open")
        .arg(&url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    eprintln!("Then paste the code the page shows here:");
    eprint!("> ");
    std::io::stderr().flush()?;
    let mut input = String::new();
    std::io::stdin().lock().read_line(&mut input)?;
    if input.trim().is_empty() {
        bail!("no code entered");
    }
    eprintln!("Exchanging the code for a token...");
    let token = exchange(&verifier, &input).await?;
    if print_only {
        println!("{}", token.access);
        return Ok(());
    }
    let path = crate::providers::save_key(name, crate::providers::Kind::Claude, &token.access)?;
    let days = token.expires_in / 86_400;
    eprintln!("Saved the token as provider {name} in {}", path.display());
    if token.expires_in > 0 && days < 30 {
        eprintln!(
            "Note: this token lasts only {} hours, not a year; log in again when it stops working.",
            token.expires_in / 3600
        );
    }
    eprintln!("Run `uc` to chat on it.");
    Ok(())
}

// ------------------------------------------------------------- requests ----

/// The text of a message's content, be it a string or a list of parts.
fn blocks(content: &Value) -> Vec<Value> {
    match content {
        Value::String(text) if !text.is_empty() => vec![json!({"type": "text", "text": text})],
        Value::Array(parts) => parts
            .iter()
            .filter(|p| p["text"].as_str().is_some_and(|t| !t.is_empty()))
            .map(|p| {
                let mut block = json!({"type": "text", "text": p["text"]});
                if let Some(cache) = p.get("cache_control") {
                    block["cache_control"] = cache.clone();
                }
                block
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// An Anthropic Messages request out of the chat's OpenAI one.
pub fn request(body: &Value, stream: bool) -> Value {
    let mut system = vec![json!({"type": "text", "text": IDENTITY})];
    let mut messages: Vec<Value> = Vec::new();
    // A message after one of the same role joins it: tool results come as one
    // `tool` message each but must share the user turn that follows the call.
    let mut push = |role: &str, content: Vec<Value>| {
        if content.is_empty() {
            return;
        }
        match messages.last_mut() {
            Some(last) if last["role"] == role => {
                last["content"]
                    .as_array_mut()
                    .expect("blocks")
                    .extend(content);
            }
            _ => messages.push(json!({"role": role, "content": content})),
        }
    };
    for message in body["messages"].as_array().into_iter().flatten() {
        match message["role"].as_str().unwrap_or_default() {
            "system" => system.extend(blocks(&message["content"])),
            "user" => push("user", blocks(&message["content"])),
            "assistant" => {
                let mut content = blocks(&message["content"]);
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    let input: Value = call["function"]["arguments"]
                        .as_str()
                        .and_then(|a| serde_json::from_str(a).ok())
                        .filter(Value::is_object)
                        .unwrap_or_else(|| json!({}));
                    content.push(json!({
                        "type": "tool_use",
                        "id": call["id"],
                        "name": wire(call["function"]["name"].as_str().unwrap_or_default()),
                        "input": input,
                    }));
                }
                push("assistant", content);
            }
            "tool" => push(
                "user",
                vec![json!({
                    "type": "tool_result",
                    "tool_use_id": message["tool_call_id"],
                    // The chat marks its last message for the cache, which
                    // turns a tool's output into parts.
                    "content": blocks(&message["content"])
                        .iter()
                        .filter_map(|b| b["text"].as_str())
                        .collect::<String>(),
                })],
            ),
            _ => {}
        }
    }
    // The prompt is read from the cache up to the last system block and the
    // last message of the turn.
    if let Some(last) = system.last_mut() {
        last["cache_control"] = json!({"type": "ephemeral"});
    }
    if let Some(block) = messages
        .last_mut()
        .and_then(|m| m["content"].as_array_mut())
        .and_then(|c| c.last_mut())
    {
        block["cache_control"] = json!({"type": "ephemeral"});
    }
    let mut request = json!({
        "model": model_id(body["model"].as_str().unwrap_or_default()),
        "max_tokens": MAX_TOKENS,
        "system": system,
        "messages": messages,
        "stream": stream,
    });
    let tools: Vec<Value> = body["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| {
            json!({
                "name": wire(t["function"]["name"].as_str().unwrap_or_default()),
                "description": t["function"]["description"],
                "input_schema": t["function"]["parameters"],
            })
        })
        .collect();
    if !tools.is_empty() {
        request["tools"] = json!(tools);
    }
    request
}

/// How many times a request is tried, as for OpenRouter.
const ATTEMPTS: u32 = 4;

/// Posts the request, trying again after a pause on a network error, 429,
/// 529 or 5xx.
async fn post(token: &str, request: &Value) -> Result<reqwest::Response> {
    let mut attempt = 1;
    loop {
        let sent = client()
            .post(MESSAGES_URL)
            .bearer_auth(token)
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", "oauth-2025-04-20,claude-code-20250219")
            .header("user-agent", "claude-cli/2.1.311")
            .header("x-app", "cli")
            .json(request)
            .send()
            .await;
        let (retry, error) = match sent {
            Err(e) => (true, anyhow::Error::new(e).context("calling Claude")),
            Ok(response) if response.status().is_success() => return Ok(response),
            Ok(response) => {
                let status = response.status();
                let value: Value = response.json().await.unwrap_or(Value::Null);
                let msg = value["error"]["message"].as_str().unwrap_or("no message");
                let retry = matches!(status.as_u16(), 429 | 529) || status.is_server_error();
                let hint = if status.as_u16() == 401 {
                    RELOGIN.to_string()
                } else {
                    String::new()
                };
                (retry, anyhow!("Claude: {status}: {msg}{hint}"))
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

/// The model the usage probe asks: the cheapest, and one token is all it says.
const PROBE_MODEL: &str = "claude-haiku-4-5";

/// One rate-limit window of the subscription, as the `anthropic-ratelimit-unified-*`
/// headers describe it.
#[derive(Debug, Default, PartialEq)]
pub struct Window {
    /// What the window is called: `5h` or `7d`.
    pub name: &'static str,
    /// How much of it is used, from 0 to 1.
    pub utilization: Option<f64>,
    /// When it resets, in seconds since the epoch.
    pub resets_at: Option<i64>,
    /// `allowed`, `allowed_warning` or `rejected`.
    pub status: Option<String>,
}

/// The usage of a Claude subscription: its rate-limit windows.
#[derive(Debug, Default, PartialEq)]
pub struct Usage {
    pub windows: Vec<Window>,
    /// The window that counts against the request, as Claude names it.
    pub representative: Option<String>,
    /// Whether Claude turned the probe away for a limit that is hit.
    pub limited: bool,
}

/// A header's text, if it is there and readable.
fn header<'a>(headers: &'a reqwest::header::HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

/// A reset as seconds since the epoch: Claude sends a unix time, but an
/// RFC 3339 date is read too.
fn reset_time(text: &str) -> Option<i64> {
    if let Ok(n) = text.parse::<f64>() {
        // Milliseconds, should they ever come.
        return Some(if n > 1e11 {
            (n / 1000.0) as i64
        } else {
            n as i64
        });
    }
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|t| t.timestamp())
}

impl Usage {
    /// The usage the rate-limit headers of a reply tell. The headers are
    /// not documented, so any of them may be missing.
    pub fn from_headers(headers: &reqwest::header::HeaderMap, limited: bool) -> Usage {
        let prefix = "anthropic-ratelimit-unified-";
        let windows = [("5h", "5h"), ("7d", "7d")]
            .into_iter()
            .map(|(name, key)| Window {
                name,
                utilization: header(headers, &format!("{prefix}{key}-utilization"))
                    .and_then(|v| v.parse().ok()),
                resets_at: header(headers, &format!("{prefix}{key}-reset")).and_then(reset_time),
                status: header(headers, &format!("{prefix}{key}-status")).map(str::to_string),
            })
            .filter(|w| w.utilization.is_some() || w.resets_at.is_some() || w.status.is_some())
            .collect();
        Usage {
            windows,
            representative: header(headers, &format!("{prefix}representative-claim"))
                .map(str::to_string),
            limited,
        }
    }

    /// The plain text `uc agent usage` prints, as of now.
    pub fn report(&self) -> String {
        self.report_at(chrono::Utc::now().timestamp())
    }

    /// The report as of `now`, in seconds since the epoch.
    pub fn report_at(&self, now: i64) -> String {
        use std::fmt::Write;
        let mut out = String::from("Claude subscription\n");
        if self.windows.is_empty() {
            out.push_str("  Claude sent no usage figures for this token\n");
            return out;
        }
        for window in &self.windows {
            let label = match window.name {
                "5h" => "5-hour window:",
                _ => "weekly window:",
            };
            let used = window.utilization.map_or("usage unknown".to_string(), |u| {
                format!("{:.0}% used", u * 100.0)
            });
            let _ = write!(out, "  {label} {used}");
            if let Some(at) = window.resets_at {
                let _ = write!(out, ", resets {}", until(at - now));
            }
            match window.status.as_deref() {
                Some("rejected") => out.push_str(" (limit reached)"),
                Some("allowed_warning") => out.push_str(" (nearly there)"),
                _ => {}
            }
            out.push('\n');
        }
        if self.limited {
            out.push_str("  the limit is hit: Claude is not taking requests until it resets\n");
        }
        if let Some(claim) = &self.representative {
            let _ = writeln!(out, "  counting against: {}", claim.replace('_', " "));
        }
        out
    }
}

/// "in 2h 10m", "in 3d 4h", "now": the wait of `secs` seconds.
fn until(secs: i64) -> String {
    if secs <= 0 {
        return "now".to_string();
    }
    let (d, h, m) = (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60);
    let wait = match (d, h, m) {
        (0, 0, m) => format!("{}m", m.max(1)),
        (0, h, m) => format!("{h}h {m}m"),
        (d, h, _) => format!("{d}d {h}h"),
    };
    format!("in {wait}")
}

/// The usage of the subscription behind `token`. Claude has no endpoint for
/// this that an inference-only token may call, so a one-token request is sent
/// and the rate-limit headers of the reply are read. A 429 carries them too.
pub async fn usage_report(token: &str) -> Result<Usage> {
    let request = json!({
        "model": PROBE_MODEL,
        "max_tokens": 1,
        "system": [{"type": "text", "text": IDENTITY}],
        "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}],
    });
    let response = client()
        .post(MESSAGES_URL)
        .bearer_auth(token)
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", "oauth-2025-04-20,claude-code-20250219")
        .header("user-agent", "claude-cli/2.1.311")
        .header("x-app", "cli")
        .json(&request)
        .send()
        .await
        .context("calling Claude")?;
    let status = response.status();
    let limited = status.as_u16() == 429;
    let usage = Usage::from_headers(response.headers(), limited);
    if status.is_success() || (limited && !usage.windows.is_empty()) {
        return Ok(usage);
    }
    let value: Value = response.json().await.unwrap_or(Value::Null);
    let msg = value["error"]["message"].as_str().unwrap_or("no message");
    let hint = if status.as_u16() == 401 {
        RELOGIN.to_string()
    } else {
        String::new()
    };
    bail!("Claude: {status}: {msg}{hint}")
}

/// An OpenAI `usage` out of Anthropic's. Prompt tokens count the cached ones
/// too, as OpenAI's do.
fn usage(input: u64, cached: u64, written: u64, output: u64) -> Value {
    let prompt = input + cached + written;
    json!({
        "prompt_tokens": prompt,
        "completion_tokens": output,
        "total_tokens": prompt + output,
        "prompt_tokens_details": {"cached_tokens": cached},
    })
}

/// Anthropic's stream, event by event, as the chunks of an OpenAI one.
#[derive(Default)]
pub struct Translator {
    /// The tool calls so far, by the index of the content block they are in.
    calls: Vec<u64>,
    input: u64,
    cached: u64,
    written: u64,
    output: u64,
}

impl Translator {
    /// The chunks one event adds.
    pub fn event(&mut self, event: &Value) -> Vec<Value> {
        let chunk = |delta: Value| json!({"choices": [{"index": 0, "delta": delta}]});
        match event["type"].as_str().unwrap_or_default() {
            "message_start" => {
                let u = &event["message"]["usage"];
                self.input = u["input_tokens"].as_u64().unwrap_or_default();
                self.cached = u["cache_read_input_tokens"].as_u64().unwrap_or_default();
                self.written = u["cache_creation_input_tokens"]
                    .as_u64()
                    .unwrap_or_default();
                self.output = u["output_tokens"].as_u64().unwrap_or_default();
                Vec::new()
            }
            "content_block_start" if event["content_block"]["type"] == "tool_use" => {
                self.calls.push(event["index"].as_u64().unwrap_or_default());
                let name = event["content_block"]["name"].as_str().unwrap_or_default();
                vec![chunk(json!({"tool_calls": [{
                    "index": self.calls.len() - 1,
                    "id": event["content_block"]["id"],
                    "function": {"name": unwire(name), "arguments": ""},
                }]}))]
            }
            "content_block_delta" => {
                let delta = &event["delta"];
                match delta["type"].as_str().unwrap_or_default() {
                    "text_delta" => vec![chunk(json!({"content": delta["text"]}))],
                    "input_json_delta" => {
                        let index = event["index"].as_u64().unwrap_or_default();
                        match self.calls.iter().position(|c| *c == index) {
                            Some(i) => vec![chunk(json!({"tool_calls": [{
                                "index": i,
                                "function": {"arguments": delta["partial_json"]},
                            }]}))],
                            None => Vec::new(),
                        }
                    }
                    _ => Vec::new(),
                }
            }
            "message_delta" => {
                if let Some(output) = event["usage"]["output_tokens"].as_u64() {
                    self.output = output;
                }
                Vec::new()
            }
            "message_stop" => vec![json!({
                "choices": [],
                "usage": usage(self.input, self.cached, self.written, self.output),
            })],
            "error" => vec![json!({"error": {
                "message": event["error"]["message"].as_str().unwrap_or("no message"),
            }})],
            _ => Vec::new(),
        }
    }
}

/// The reply bytes the chat reads: OpenAI-style server-sent events.
pub type Bytes = Pin<Box<dyn Stream<Item = std::io::Result<Vec<u8>>> + Send>>;

/// Posts the chat's streaming request to Claude and gives back its reply as
/// the SSE chunks the chat reads from OpenRouter.
pub async fn stream(token: &str, body: &Value) -> Result<Bytes> {
    let response = post(token, &request(body, true)).await?;
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

/// Posts a request that doesn't stream; the reply is OpenAI's: the message
/// in `choices[0].message`.
pub async fn send(token: &str, body: &Value) -> Result<Value> {
    let value: Value = post(token, &request(body, false))
        .await?
        .json()
        .await
        .context("reading Claude's reply")?;
    let text: String = value["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect();
    let u = &value["usage"];
    Ok(json!({
        "choices": [{"message": {"role": "assistant", "content": text}}],
        "usage": usage(
            u["input_tokens"].as_u64().unwrap_or_default(),
            u["cache_read_input_tokens"].as_u64().unwrap_or_default(),
            u["cache_creation_input_tokens"].as_u64().unwrap_or_default(),
            u["output_tokens"].as_u64().unwrap_or_default(),
        ),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &'static str)]) -> reqwest::header::HeaderMap {
        let mut map = reqwest::header::HeaderMap::new();
        for (k, v) in pairs {
            map.insert(*k, reqwest::header::HeaderValue::from_static(v));
        }
        map
    }

    const FULL: &[(&str, &str)] = &[
        ("anthropic-ratelimit-unified-5h-utilization", "0.26"),
        ("anthropic-ratelimit-unified-5h-reset", "17800"),
        ("anthropic-ratelimit-unified-5h-status", "allowed"),
        ("anthropic-ratelimit-unified-7d-utilization", "0.13"),
        ("anthropic-ratelimit-unified-7d-reset", "360000"),
        ("anthropic-ratelimit-unified-7d-status", "allowed"),
        (
            "anthropic-ratelimit-unified-representative-claim",
            "five_hour",
        ),
    ];

    #[test]
    fn the_rate_limit_headers_make_the_report() {
        let usage = Usage::from_headers(&headers(FULL), false);
        assert_eq!(usage.windows.len(), 2);
        assert_eq!(usage.windows[0].utilization, Some(0.26));
        assert_eq!(usage.windows[1].resets_at, Some(360_000));
        let report = usage.report_at(10_000);
        assert!(report.starts_with("Claude subscription\n"));
        assert!(report.contains("5-hour window: 26% used, resets in 2h 10m"));
        assert!(report.contains("weekly window: 13% used, resets in 4d 1h"));
        assert!(report.contains("counting against: five hour"));
        assert!(!report.contains("limit is hit"));
    }

    #[test]
    fn missing_headers_are_told_plainly_and_a_hit_limit_is_noted() {
        let none = Usage::from_headers(&headers(&[]), false);
        assert!(none.report_at(0).contains("no usage figures"));
        let hit = Usage::from_headers(
            &headers(&[
                ("anthropic-ratelimit-unified-5h-utilization", "1"),
                ("anthropic-ratelimit-unified-5h-status", "rejected"),
            ]),
            true,
        );
        let report = hit.report_at(0);
        assert!(report.contains("100% used (limit reached)"));
        assert!(report.contains("the limit is hit"));
    }

    #[test]
    fn waits_read_well() {
        assert_eq!(until(-5), "now");
        assert_eq!(until(20), "in 1m");
        assert_eq!(until(7800), "in 2h 10m");
        assert_eq!(until(3 * 86_400 + 4 * 3600 + 99), "in 3d 4h");
        assert_eq!(reset_time("2030-01-01T00:00:00Z"), Some(1_893_456_000));
    }

    #[test]
    fn models_are_named_the_anthropic_way() {
        assert_eq!(model_id("anthropic/claude-opus-5.5"), "claude-opus-5-5");
        assert_eq!(model_id("claude-haiku-5-5"), "claude-haiku-5-5");
    }

    #[test]
    fn the_login_url_carries_the_challenge_and_state() {
        let url = authorize_url("CHAL", "VER");
        assert!(url.starts_with("https://claude.com/cai/oauth/authorize?code=true&client_id="));
        assert!(url.contains("code_challenge=CHAL&code_challenge_method=S256&state=VER"));
        assert!(url.contains("scope=user%3Ainference"));
        assert!(
            url.contains(
                "redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback"
            )
        );
    }

    #[test]
    fn pkce_challenge_is_the_hash_of_the_verifier() {
        let (verifier, challenge) = pkce().unwrap();
        assert_eq!(
            challenge,
            URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
        );
        assert!(verifier.len() >= 43);
    }

    #[test]
    fn the_pasted_code_comes_in_any_shape() {
        assert_eq!(
            parse_code("abc#st").unwrap(),
            ("abc".into(), Some("st".into()))
        );
        assert_eq!(parse_code("  abc ").unwrap(), ("abc".into(), None));
        let url = "https://platform.claude.com/oauth/code/callback?code=abc&state=st";
        assert_eq!(parse_code(url).unwrap(), ("abc".into(), Some("st".into())));
        assert_eq!(
            parse_code("code=abc&state=st").unwrap(),
            ("abc".into(), Some("st".into()))
        );
        assert!(parse_code("  ").is_err());
    }

    #[test]
    fn a_token_without_inference_is_refused() {
        let ok = json!({"access_token": "t", "expires_in": 100, "scope": "user:inference"});
        assert_eq!(token_from(&ok).unwrap().access, "t");
        let bad = json!({"access_token": "t", "scope": "user:profile"});
        assert!(token_from(&bad).is_err());
        assert!(token_from(&json!({})).is_err());
    }

    fn chat() -> Value {
        json!({
            "model": "anthropic/claude-opus-5.5",
            "messages": [
                {"role": "system", "content": [
                    {"type": "text", "text": "be uc", "cache_control": {"type": "ephemeral"}}]},
                {"role": "user", "content": "list files"},
                {"role": "assistant", "content": null, "tool_calls": [
                    {"id": "t1", "type": "function",
                     "function": {"name": "bash", "arguments": "{\"command\":\"ls\"}"}},
                    {"id": "t2", "type": "function",
                     "function": {"name": "read_file", "arguments": "{\"path\":\"a\"}"}}]},
                {"role": "tool", "tool_call_id": "t1", "content": "a b"},
                {"role": "tool", "tool_call_id": "t2", "content": "text"},
            ],
            "tools": [{"type": "function", "function": {
                "name": "bash", "description": "run", "parameters": {"type": "object"}}}],
        })
    }

    #[test]
    fn the_request_is_claude_codes() {
        let r = request(&chat(), true);
        assert_eq!(r["model"], "claude-opus-5-5");
        assert_eq!(r["stream"], true);
        assert_eq!(r["system"][0]["text"], IDENTITY);
        assert_eq!(r["system"][1]["text"], "be uc");
        assert_eq!(r["system"][1]["cache_control"]["type"], "ephemeral");
        assert_eq!(r["tools"][0]["name"], "Bash");
        assert_eq!(r["tools"][0]["input_schema"]["type"], "object");
        let m = r["messages"].as_array().unwrap();
        assert_eq!(m.len(), 3);
        assert_eq!(m[1]["content"][0]["name"], "Bash");
        assert_eq!(m[1]["content"][0]["input"]["command"], "ls");
        assert_eq!(m[1]["content"][1]["name"], "read_file");
        // Both results share the one user turn after the calls.
        assert_eq!(m[2]["role"], "user");
        assert_eq!(m[2]["content"].as_array().unwrap().len(), 2);
        assert_eq!(m[2]["content"][0]["tool_use_id"], "t1");
        assert_eq!(m[2]["content"][1]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn a_tool_output_marked_for_the_cache_keeps_its_text() {
        let body = json!({"model": "anthropic/claude-opus-5.5", "messages": [
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "a", "function": {"name": "bash", "arguments": "{}"}}]},
            {"role": "tool", "tool_call_id": "a", "content": [
                {"type": "text", "text": "the output", "cache_control": {"type": "ephemeral"}}]},
        ]});
        let r = request(&body, true);
        let result = &r["messages"][2]["content"][0];
        assert_eq!(result["type"], "tool_result");
        assert_eq!(result["content"], "the output");
        assert_eq!(result["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn the_stream_becomes_chat_chunks() {
        let mut t = Translator::default();
        let events = [
            json!({"type": "message_start", "message": {"usage": {
                "input_tokens": 10, "cache_read_input_tokens": 90, "output_tokens": 1}}}),
            json!({"type": "content_block_start", "index": 0,
                   "content_block": {"type": "text", "text": ""}}),
            json!({"type": "content_block_delta", "index": 0,
                   "delta": {"type": "text_delta", "text": "hi"}}),
            json!({"type": "content_block_start", "index": 1,
                   "content_block": {"type": "tool_use", "id": "t1", "name": "Bash"}}),
            json!({"type": "content_block_delta", "index": 1,
                   "delta": {"type": "input_json_delta", "partial_json": "{\"command\""}}),
            json!({"type": "content_block_delta", "index": 1,
                   "delta": {"type": "input_json_delta", "partial_json": ":\"ls\"}"}}),
            json!({"type": "message_delta", "usage": {"output_tokens": 7}}),
            json!({"type": "message_stop"}),
        ];
        let chunks: Vec<Value> = events.iter().flat_map(|e| t.event(e)).collect();
        assert_eq!(chunks[0]["choices"][0]["delta"]["content"], "hi");
        let start = &chunks[1]["choices"][0]["delta"]["tool_calls"][0];
        assert_eq!(start["index"], 0);
        assert_eq!(start["id"], "t1");
        assert_eq!(start["function"]["name"], "bash");
        assert_eq!(
            chunks[3]["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"],
            ":\"ls\"}"
        );
        let usage = &chunks.last().unwrap()["usage"];
        assert_eq!(usage["prompt_tokens"], 100);
        assert_eq!(usage["completion_tokens"], 7);
        assert_eq!(usage["total_tokens"], 107);
        assert_eq!(usage["prompt_tokens_details"]["cached_tokens"], 90);
    }

    #[test]
    fn a_stream_error_is_a_chat_error() {
        let mut t = Translator::default();
        let chunks = t.event(&json!({"type": "error", "error": {"message": "overloaded"}}));
        assert_eq!(chunks[0]["error"]["message"], "overloaded");
    }
}
