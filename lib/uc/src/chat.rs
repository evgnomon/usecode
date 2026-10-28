// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc` with no command: a coding agent on the terminal.
//!
//! Each line typed goes to a model, which answers or asks to
//! run a shell command, read, write or edit a file in the current directory,
//! search the web (Parallel when `agent.yaml` has its key under `search:`,
//! else Exa when it has that one, else DuckDuckGo's HTML page, no key;
//! `--parallel`, `--exa` or `--duckduckgo` picks one) or read a page
//! (rendered by Cloudflare when it has an account under `fetch:`, else
//! fetched as is).
//! Commands, writes and edits are shown and run straight away; with `--ask`
//! they are confirmed first (`a` allows the rest of the session, a line ending
//! in `-y` allows what its answer does). Deleting files is always confirmed.
//! A command still running after its timeout carries on as a job the model
//! follows and stops itself. Only the model's replies are printed, not what
//! the commands print, unless `--output` is given. A command the model runs
//! shows as a few words on what it does, like `· list the rust files`, unless
//! `--commands` is given, which shows the command line itself. A line that is a command
//! (`ls -al`, `git status`, or anything after `!`) runs on the terminal
//! straight away, with the same confirmations, without the model hearing
//! about it (`cd` is the exception: it tells the model the directory changed);
//! the user's bash aliases and functions work there too;
//! `--no-shell` (or `UC_NO_SHELL=1`, or `no_shell: true` under `uc:` in the
//! user config, see [`crate::flag`]) sends every line to the model. The model can drive the chat
//! itself with the `chat` tool: `model` switches the model (no `model` lists
//! them), `clear` clears the screen
//! and the conversation context like Ctrl-L,
//! and `exit` leaves uc. Esc or Ctrl-C stops the model,
//! and the command it is waiting on, and goes back to the prompt; Ctrl-C at
//! the prompt quits. Holding Space at an empty prompt records the microphone
//! and sends what was said instead, see `voice`. Ctrl-L at the prompt clears
//! the screen and the model's context with it. `uc agent model` picks one of
//! the models of every provider with a key in the picker (it lists them without
//! a terminal),
//! `uc agent model <number|name> [provider]` switches to one straight away; either
//! way the lines after it go to that model, with the context kept. The same
//! words typed in a shell save the choice for the new sessions instead
//! (`provider:` and `model:` in `agent.yaml`), and the sessions a switch is typed in save it too:
//! the other `uc` processes running keep their model, the ones started after
//! follow. The
//! lists of models are kept in `~/.cache/usecode/models.json` so that
//! starting uc does not wait on the providers; `uc agent model-update` fetches them
//! again. A model
//! more than one provider serves is
//! listed once for each, with the provider beside it. `uc agent exit` (or
//! `quit`) leaves uc, `uc agent compact` sums up the earlier conversation,
//! and `uc agent context`
//! prints how much of the model's context the conversation has taken: the
//! message count, an estimate before the first turn, the real token count of
//! the last one, the model's window, and what the session has spent.
//! What the model is told about all this is the main skill, `chat/main.md`, always in its system prompt.
//! The providers and their keys are in `agent.yaml` in the user config, see
//! [`providers`]: OpenRouter, a Claude subscription (see [`claude`]), GitHub
//! Copilot (see [`copilot`]) and any server that speaks OpenAI's chat
//! completions; the session starts on the first of them with a key, or the
//! one `provider:` there (or `UC_PROVIDER`) names. Without any the AI
//! is off and every line runs in
//! bash, like a plain shell with uc's history and confirmations. The model
//! is `model:` in `agent.yaml` (or `UC_MODEL`), else the provider's default
//! (GLM 5.3 Flash on OpenRouter), and the reasoning effort `effort:` (or
//! `UC_EFFORT`). The session
//! keeps one model: each line is one request to it, and it changes only
//! when `uc agent model` or `chat model` says so.

use anyhow::{Context, Result, anyhow, bail};
use futures_util::StreamExt;
use rustyline::error::ReadlineError;
use rustyline::history::{DefaultHistory, SearchDirection};
use rustyline::{
    Cmd, CompletionType, ConditionalEventHandler, Config, Editor, Event, EventContext,
    EventHandler, KeyCode, KeyEvent, Modifiers, Movement, RepeatCount,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::{ExitCode, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::task::JoinHandle;

use crate::{claude, copilot, providers};

mod diff;
mod voice;

const DEFAULT_MODEL: &str = "z-ai/glm-5.3-flash";
/// The model on the Claude subscription unless `UC_MODEL` says otherwise.
const DEFAULT_CLAUDE_MODEL: &str = "anthropic/claude-sonnet-5.5";
const DEFAULT_EFFORT: &str = "high";
/// OpenRouter's models `uc agent model` offers: everyday, reading, patches, features,
/// short hard thinking and the strongest.
const MODELS: &[&str] = &[
    DEFAULT_MODEL,
    "anthropic/claude-haiku-5.5",
    "z-ai/glm-5.3",
    "meta/muse-spark-1.3",
    "openai/gpt-6-luna",
    "anthropic/claude-opus-5.5",
];
/// What the Claude subscription serves, in OpenRouter's names (see
/// `claude::model_id`).
const CLAUDE_MODELS: &[&str] = &[
    "anthropic/claude-haiku-5.5",
    DEFAULT_CLAUDE_MODEL,
    "anthropic/claude-opus-5.5",
];
const DUCKDUCKGO: &str = "https://html.duckduckgo.com/html/";
const EXA: &str = "https://api.exa.ai/search";
const PARALLEL: &str = "https://api.parallel.ai/v1beta/search";
/// How long a search or a page may take.
const WEB_TIMEOUT: Duration = Duration::from_secs(60);
/// Tool output beyond this is cut, so one noisy command can't fill the context.
const MAX_OUTPUT: usize = 30_000;
/// How long bash waits before a command carries on as a job.
const DEFAULT_TIMEOUT: u64 = 120;
/// The most of a job's output kept; a server can run for hours.
const MAX_KEPT: usize = 1 << 20;
/// Commands that delete files: the user is always asked before these.
const DELETES: &[&str] = &["rm", "rmdir", "unlink", "shred", "-delete"];
/// Always in the system prompt: how commands run, jobs, what needs asking.
const MAIN_SKILL: &str = include_str!("chat/main.md");
/// Builtins worth running when typed at the prompt; `cd` changes uc's own
/// directory. Keywords like `if` and `for` are left out: they start sentences.
const BUILTINS: &[&str] = &["cd", "echo", "printf", "pwd", "type", "command", "time"];
/// Programs whose name is usually an English word when typed at the prompt:
/// `yes` answers the model, it doesn't print y forever. `!yes` still runs it.
const WORDS: &[&str] = &["yes", "more", "look", "write", "see", "at", "go"];
/// Words that make a line a sentence for the model, not a command line.
const PROSE: &[&str] = &[
    "the", "a", "an", "it", "its", "it's", "this", "that", "these", "those", "is", "are", "was",
    "be", "to", "for", "of", "and", "but", "with", "at", "on", "why", "how", "what", "when",
    "where", "me", "my", "i", "you", "your", "we", "our", "please", "can", "could", "should",
    "would", "not", "ahead",
];

/// A command runs on the terminal: Ctrl-C stops it, not uc.
static FOREGROUND: AtomicBool = AtomicBool::new(false);

/// The model is working on a line: Ctrl-C stops it, like Esc, not uc.
static BUSY: AtomicBool = AtomicBool::new(false);

/// Told when Ctrl-C stops the model.
static STOP: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// A prompt or a command has the terminal: Esc isn't watched for.
static LENT: AtomicBool = AtomicBool::new(false);

/// The terminal settings from before the Esc watcher changed them; Some while
/// they are changed.
static SAVED: Mutex<Option<libc::termios>> = Mutex::new(None);

/// Locks `SAVED`, poisoned or not. The watcher holds it only to look at or
/// change the settings, never across `poll` or `read`: they can block for
/// seconds, and a thread that hands the terminal back (Ctrl-C, `chat exit`)
/// would wait on it while the user answers nothing.
fn saved() -> std::sync::MutexGuard<'static, Option<libc::termios>> {
    SAVED.lock().unwrap_or_else(|e| e.into_inner())
}

/// The process groups of the jobs still running, stopped when uc exits.
static GROUPS: Mutex<BTreeSet<i32>> = Mutex::new(BTreeSet::new());

/// Set when Ctrl-L cleared the screen: the context goes with it, before the
/// line being typed is sent.
static CLEARED: AtomicBool = AtomicBool::new(false);

/// The AI is on and typed commands run straight away: what the prompt loop
/// decides from, kept for the line editor to color the line the same way.
static AI: AtomicBool = AtomicBool::new(false);
static SHELL: AtomicBool = AtomicBool::new(false);

/// The last command the user typed failed: the ₿ turns red, like in bash.
static FAILED: AtomicBool = AtomicBool::new(false);

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        // graviola is pure Rust, see `kube::cloudflare::client`.
        let _ = rustls_graviola::default_provider().install_default();
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(600))
            .build()
            .expect("HTTP client")
    })
}

/// `ask` confirms every command and change instead of running it straight away;
/// `output` shows what the commands print as well as the model's replies;
/// `commands` shows the model's command lines instead of their titles;
/// `shell` runs a line that is a command instead of sending it to the model;
/// `search` is where web_search looks, the first with a key if not given.
pub fn main(
    ask: bool,
    output: bool,
    commands: bool,
    shell: bool,
    search: Option<Search>,
) -> ExitCode {
    // Worker threads keep reading the jobs' pipes while the prompt waits.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");
    let result = runtime.block_on(run(ask, output, commands, shell, search));
    stop_jobs();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// A search engine web_search can use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Search {
    Parallel,
    Exa,
    DuckDuckGo,
}

/// The engine web_search uses, with its key.
#[derive(Debug, PartialEq, Eq)]
enum Engine {
    Parallel(String),
    Exa(String),
    DuckDuckGo,
}

impl Engine {
    /// The one asked for, else Parallel, then Exa, whichever has a key, else
    /// DuckDuckGo.
    fn pick(search: Option<Search>) -> Result<Engine> {
        let settings = providers::settings();
        let (parallel, exa) = (settings.parallel_key.clone(), settings.exa_key.clone());
        let key = |key: Option<String>, name: &str, site: &str| {
            key.ok_or_else(|| {
                anyhow!(
                    "add search: {name}: api_key: ({site}) to {} to search with it",
                    providers::path().display()
                )
            })
        };
        Ok(match search {
            Some(Search::Parallel) => {
                Engine::Parallel(key(parallel, "parallel", "https://parallel.ai")?)
            }
            Some(Search::Exa) => Engine::Exa(key(exa, "exa", "https://exa.ai")?),
            Some(Search::DuckDuckGo) => Engine::DuckDuckGo,
            None => parallel
                .map(Engine::Parallel)
                .or_else(|| exa.map(Engine::Exa))
                .unwrap_or(Engine::DuckDuckGo),
        })
    }
}

/// The providers a session can talk to, from `agent.yaml`; see
/// [`crate::providers`].
pub(crate) use crate::providers::{Kind, Provider};

/// A model on a provider: what `uc agent model` lists and switches to.
#[derive(Clone, Debug)]
pub(crate) struct Choice {
    /// The provider's name in `agent.yaml`.
    pub(crate) provider: String,
    /// The provider's id for it; the Claude subscription's are OpenRouter's.
    pub(crate) model: String,
    /// From the provider's whole list, not the few it offers first: `opus`
    /// means the picked Opus, not every Opus OpenRouter ever served.
    pub(crate) extra: bool,
}

impl PartialEq for Choice {
    fn eq(&self, other: &Choice) -> bool {
        self.provider == other.provider && self.model == other.model
    }
}

impl Eq for Choice {}

impl Choice {
    pub(crate) fn new(provider: &str, model: &str) -> Choice {
        Choice {
            provider: provider.to_string(),
            model: model.to_string(),
            extra: false,
        }
    }

    fn extra(provider: &str, model: &str) -> Choice {
        Choice {
            extra: true,
            ..Choice::new(provider, model)
        }
    }
}

/// The name a model goes by whoever serves it: `anthropic/claude-opus-5.5`
/// and Copilot's `claude-opus-5-5` are both `claude-opus-5.5`.
fn common_name(model: &str) -> String {
    let name = model.rsplit('/').next().unwrap_or(model).to_lowercase();
    let chars: Vec<char> = name.chars().collect();
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let digits = |j: Option<usize>| {
                j.and_then(|j| chars.get(j))
                    .is_some_and(char::is_ascii_digit)
            };
            if *c == '-' && digits(i.checked_sub(1)) && digits(Some(i + 1)) {
                '.'
            } else {
                *c
            }
        })
        .collect()
}

/// A model list from the cache under `key`, unless `refresh`; else from
/// `fetch`, and kept in the cache.
async fn cached_list<F>(key: &str, refresh: bool, fetch: F) -> Result<Vec<String>>
where
    F: std::future::Future<Output = Result<Vec<String>>>,
{
    let cached = (!refresh)
        .then(|| cache_get(key))
        .flatten()
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok());
    if let Some(models) = cached {
        return Ok(models);
    }
    let models = fetch.await?;
    cache_set(key, json!(models));
    Ok(models)
}

/// The models of every provider that can be talked to: the ones each
/// offers first (`models:` in the file, else uc's picks for OpenRouter and
/// the Claude subscription), then the rest of what it lists. The lists come
/// from the cache (see [`cache_path`]); a list that isn't there yet is
/// fetched, and `refresh` fetches them all again.
pub(crate) async fn pool(providers: &[Provider], refresh: bool) -> Vec<Choice> {
    let mut pool = Vec::new();
    for provider in providers {
        let name = provider.name.as_str();
        let picks: Vec<String> = if !provider.models.is_empty() {
            provider.models.clone()
        } else {
            match provider.kind {
                Kind::OpenRouter => MODELS.iter().map(|m| m.to_string()).collect(),
                Kind::Claude => CLAUDE_MODELS.iter().map(|m| m.to_string()).collect(),
                Kind::OpenAi | Kind::Copilot => Vec::new(),
            }
        };
        pool.extend(picks.iter().map(|m| Choice::new(name, m)));
        if !provider.list_models {
            continue;
        }
        let listed: Result<Vec<String>> = match provider.kind {
            Kind::OpenRouter => {
                let list = if refresh {
                    refresh_models().await
                } else {
                    models().await
                };
                list.map(|list| {
                    list.iter()
                        .filter_map(chat_model_id)
                        .map(String::from)
                        .collect()
                })
            }
            // The subscription has no list to ask for.
            Kind::Claude => Ok(Vec::new()),
            Kind::Copilot => {
                cached_list(
                    &format!("copilot:{name}"),
                    refresh,
                    copilot::models(&provider.key),
                )
                .await
            }
            Kind::OpenAi => {
                cached_list(&format!("openai:{name}"), refresh, openai_models(provider)).await
            }
        };
        match listed {
            Ok(models) => {
                // Copilot's list is what the account may pick: all of it is
                // offered, none of it extra.
                let extra = provider.kind != Kind::Copilot;
                for model in models.iter().filter(|m| !picks.contains(m)) {
                    pool.push(if extra {
                        Choice::extra(name, model)
                    } else {
                        Choice::new(name, model)
                    });
                }
            }
            Err(err) => eprintln!("· {}'s models: {err:#}", provider.label()),
        }
    }
    pool
}

/// The ids an OpenAI-style provider's `/models` lists.
async fn openai_models(provider: &Provider) -> Result<Vec<String>> {
    let mut request = client()
        .get(format!("{}/models", provider.base_url))
        .timeout(Duration::from_secs(20));
    if !provider.key.is_empty() {
        request = request.bearer_auth(&provider.key);
    }
    for (name, value) in &provider.headers {
        request = request.header(name, value);
    }
    let list: Value = request
        .send()
        .await
        .with_context(|| format!("calling {}", provider.label()))?
        .error_for_status()?
        .json()
        .await
        .context("reading the model list")?;
    let mut ids: Vec<String> = list["data"]
        .as_array()
        .ok_or_else(|| anyhow!("the model list came without data"))?
        .iter()
        .filter_map(|m| m["id"].as_str().map(String::from))
        .collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// The model a session on `provider` starts on: the one the file names,
/// else uc's default for OpenRouter and the Claude subscription, else the
/// first it lists.
fn default_model(provider: &Provider, pool: &[Choice]) -> String {
    if let Some(model) = &provider.default_model {
        return model.clone();
    }
    match provider.kind {
        Kind::OpenRouter if provider.models.is_empty() => DEFAULT_MODEL.to_string(),
        Kind::Claude if provider.models.is_empty() => DEFAULT_CLAUDE_MODEL.to_string(),
        _ => pool
            .iter()
            .find(|c| c.provider == provider.name)
            .map(|c| c.model.clone())
            .unwrap_or_default(),
    }
}

/// The provider named `name`, in any case.
fn named<'a>(providers: &'a [Provider], name: &str) -> Option<&'a Provider> {
    providers
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name.trim()))
}

/// The provider a session starts on: the one `UC_PROVIDER` names, else
/// `provider:` in `agent.yaml`, when it can be talked to; else the first in
/// the file that can.
fn start_provider(providers: &[Provider]) -> Option<&Provider> {
    let wanted = env("UC_PROVIDER").or_else(|| providers::settings().provider.clone());
    let found = wanted.as_deref().and_then(|name| named(providers, name));
    if let (Some(name), None) = (&wanted, found) {
        eprintln!(
            "· the provider to start on is {name}, but {} has no usable provider {name}",
            providers::path().display()
        );
    }
    found.or(providers.first())
}

/// The model a session on `start` starts on: `UC_MODEL`, else `model:` in
/// `agent.yaml` when it was saved for that provider, else the provider's
/// default.
fn start_model(start: &Provider, pool: &[Choice]) -> String {
    let settings = providers::settings();
    env("UC_MODEL")
        .or_else(|| {
            settings.model.clone().filter(|_| {
                settings
                    .provider
                    .as_deref()
                    .is_none_or(|p| p.eq_ignore_ascii_case(&start.name))
            })
        })
        .unwrap_or_else(|| default_model(start, pool))
}

/// What to say when no provider can be talked to.
fn no_provider() -> String {
    format!(
        "no provider has a key: add one under providers: in {}, or log in with \
         `uc agent login claude` or `uc agent login copilot`",
        providers::path().display()
    )
}

struct Session {
    /// The providers that can be talked to, from `agent.yaml`.
    providers: Vec<Provider>,
    /// The name of the one `model` runs on.
    provider: String,
    /// The model every line goes to.
    model: String,
    /// What `uc agent model` lists and picks from.
    pool: Vec<Choice>,
    ask: bool,
    /// OpenRouter's reasoning effort: low, medium or high.
    effort: String,
    messages: Vec<Value>,
    /// Auto mode, or `a` at a confirmation: run everything without asking.
    allow_all: bool,
    /// The line ended in `-y`: run what this turn asks for without asking.
    allow_turn: bool,
    /// Print what commands and jobs print, not only the model's replies.
    show_output: bool,
    /// Print the model's command lines, not the few words on what they do.
    show_commands: bool,
    jobs: BTreeMap<usize, Job>,
    next_job: usize,
    search: Engine,
    /// The job bash is waiting on, stopped if Esc is pressed meanwhile.
    waiting: Option<usize>,
    /// What OpenRouter's last reply said the turn took in tokens and cost.
    last_usage: Option<Value>,
    /// What every turn has cost so far, from `usage.cost`.
    spent: f64,
}

/// A command started by bash, followed with job_output and stopped with job_kill.
struct Job {
    child: tokio::process::Child,
    /// Its process group, which is its pid.
    group: i32,
    output: Arc<Mutex<Output>>,
    /// How much of the output the model has seen.
    seen: usize,
    /// Copy the output to the terminal as it comes: while bash waits for it.
    echo: Arc<AtomicBool>,
    readers: Vec<JoinHandle<()>>,
    status: Option<ExitStatus>,
}

/// A job's stdout and stderr together; only the last `MAX_KEPT` bytes stay.
#[derive(Default)]
struct Output {
    bytes: Vec<u8>,
    /// Bytes dropped from the front.
    start: usize,
}

impl Output {
    fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend_from_slice(chunk);
        if self.bytes.len() > MAX_KEPT {
            let cut = self.bytes.len() - MAX_KEPT;
            self.bytes.drain(..cut);
            self.start += cut;
        }
    }

    /// What came after `seen`, and the new `seen`.
    fn since(&self, seen: usize) -> (String, usize) {
        let mut text = String::new();
        if seen < self.start {
            text.push_str("[earlier output dropped]\n");
        }
        let from = seen.saturating_sub(self.start).min(self.bytes.len());
        text.push_str(&String::from_utf8_lossy(&self.bytes[from..]));
        (text, self.start + self.bytes.len())
    }
}

/// Copies one of a job's pipes into its output until it closes.
fn pump<R: AsyncRead + Unpin + Send + 'static>(
    from: Option<R>,
    output: Arc<Mutex<Output>>,
    echo: Arc<AtomicBool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let Some(mut from) = from else { return };
        let mut buf = [0; 8192];
        while let Ok(n @ 1..) = from.read(&mut buf).await {
            if echo.load(Ordering::Relaxed) {
                let mut err = std::io::stderr().lock();
                let _ = err.write_all(&buf[..n]);
                let _ = err.flush();
            }
            output.lock().unwrap().push(&buf[..n]);
        }
    })
}

fn signal_group(group: i32, signal: i32) {
    // SAFETY: kill only sends a signal; a negative pid names the group.
    unsafe { libc::kill(-group, signal) };
}

/// Waits up to `secs` for every process of a group to be gone.
async fn group_gone(group: i32, secs: u64) -> bool {
    let until = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        // SAFETY: signal 0 only checks whether the group still exists.
        if unsafe { libc::kill(-group, 0) } == -1 {
            return true;
        }
        if tokio::time::Instant::now() >= until {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn stop_jobs() {
    for group in std::mem::take(&mut *GROUPS.lock().unwrap()) {
        signal_group(group, libc::SIGTERM);
    }
}

/// Whether a command line deletes files. It errs on the side of asking:
/// `git rm` and `echo rm` count too.
fn deletes(command: &str) -> bool {
    command
        .split(|c: char| c.is_whitespace() || ";|&()<>`'\"{}$".contains(c))
        .map(|word| word.trim_start_matches('\\'))
        .map(|word| word.rsplit('/').next().unwrap_or(word))
        .any(|word| DELETES.contains(&word))
}

/// Whether a command gets the terminal: only `ssh-add`, which asks for a key's
/// passphrase.
fn on_terminal(command: &str) -> bool {
    let word = command.split_whitespace().next().unwrap_or("");
    word.rsplit('/').next() == Some("ssh-add")
}

async fn run(
    ask: bool,
    output: bool,
    commands: bool,
    shell: bool,
    search: Option<Search>,
) -> Result<()> {
    // Without a provider the AI is off and every line runs in bash. The
    // session starts on the one `UC_PROVIDER` names, else the first in
    // `agent.yaml`; the others' models are a `uc agent model` away.
    let providers = providers::usable();
    let start = start_provider(&providers).cloned();
    let ai = start.is_some();
    AI.store(ai, Ordering::Relaxed);
    SHELL.store(shell, Ordering::Relaxed);
    let pool = pool(&providers, false).await;
    let model = start
        .as_ref()
        .map(|p| start_model(p, &pool))
        .unwrap_or_default();
    let effort = env("UC_EFFORT")
        .or_else(|| providers::settings().effort.clone())
        .unwrap_or_else(|| DEFAULT_EFFORT.to_string());
    let mut session = Session {
        messages: vec![json!({"role": "system", "content": system_prompt(&model, ask, output)})],
        providers,
        provider: start.map(|p| p.name).unwrap_or_default(),
        pool,
        model,
        ask,
        effort,
        allow_all: !ask,
        allow_turn: false,
        show_output: output,
        show_commands: commands,
        jobs: BTreeMap::new(),
        next_job: 1,
        search: Engine::pick(search)?,
        waiting: None,
        last_usage: None,
        spent: 0.0,
    };
    if shell || !ai {
        // Read the user's aliases while they type, not when the first line comes.
        std::thread::spawn(user_shell);
    }
    tokio::spawn(async {
        while tokio::signal::ctrl_c().await.is_ok() {
            if FOREGROUND.load(Ordering::Relaxed) {
                continue;
            }
            if BUSY.load(Ordering::Relaxed) {
                STOP.notify_waiters();
                continue;
            }
            stop_jobs();
            restore_terminal(&mut saved());
            eprintln!();
            std::process::exit(130);
        }
    });
    let mut shown: Option<String> = None;
    loop {
        let Some(line) = read_prompt(&prompt())? else {
            eprintln!();
            return Ok(());
        };
        // Ctrl-L comes while the line is typed, so the line goes fresh.
        if CLEARED.swap(false, Ordering::Relaxed) {
            session.messages.retain(|m| m["role"] == "system");
            eprintln!("· context cleared");
        }
        let line = line.trim();
        let agent = agent_command(line);
        if line == "exit" || matches!(agent, Some("exit" | "quit")) {
            return Ok(());
        }
        if !ai && (agent_model_line(line).is_some() || agent.is_some()) {
            eprintln!("· AI off: {}, to turn it on", no_provider());
            continue;
        }
        if line == "uc agent model-update" {
            session.update_models().await;
            continue;
        }
        if let Some(rest) = agent_model_line(line) {
            let name = rest.trim();
            if name.is_empty() || choice_named(&session.pool, name, &session.provider).is_none() {
                session.pick_model(name)
            } else {
                session.switch_and_save(name);
            }
            shown = Some(session.models());
            continue;
        }
        if agent == Some("context") {
            session.context().await;
            continue;
        }
        if agent == Some("compact") {
            if let Err(err) = session.compact().await {
                eprintln!("· {err:#}");
            }
            continue;
        }
        let (line, approved) = split_approval(line);
        if line.is_empty() {
            continue;
        }
        session.allow_turn = approved;
        // A typed command runs straight away, without the model: no `· model`
        // print, and nothing about it in the next turn.
        let command = direct_command(line);
        if command.is_none() && !ai {
            continue;
        }
        // The model the line goes to, printed when it is new; the same
        // again stays silent.
        if command.is_none() && shown.as_deref() != Some(session.models().as_str()) {
            eprintln!("· {}", session.models());
        }
        let shown_after = async {
            match command {
                Some(command) => session.run_typed(command).await,
                None => session.ask(line).await,
            }
        };
        let ctrl_c = STOP.notified();
        tokio::pin!(ctrl_c);
        ctrl_c.as_mut().enable();
        BUSY.store(true, Ordering::Relaxed);
        let (done, stopped) = tokio::select! {
            done = shown_after => match done {
                Err(err) if err.is::<Stopped>() => (Ok(()), true),
                done => (done, false),
            },
            // A typed command has the terminal and Ctrl-C to stop it: no Esc
            // watcher to take keys from it or leave it in cbreak.
            () = escape(), if command.is_none() => (Ok(()), true),
            () = ctrl_c => (Ok(()), true),
        };
        BUSY.store(false, Ordering::Relaxed);
        if stopped {
            eprintln!("\r\x1b[2K· stopped");
            session.stopped();
        }
        if let Err(err) = done {
            eprintln!("uc: {err:#}");
        }
        session.allow_turn = false;
        // A tool call may have switched the model or cleared the context
        // without the prompt loop seeing it; what the model last ran on is
        // what the next line starts from.
        shown = Some(session.models());
        if CLEARED.swap(false, Ordering::Relaxed) {
            session.messages.retain(|m| m["role"] == "system");
            eprintln!("· context cleared");
        }
    }
}

/// The command line a typed line runs straight away, without the model: with
/// the AI off every line does, with it on only one that reads like a command.
/// Models that cache a prompt only up to a `cache_control` breakpoint; the
/// others (OpenAI, DeepSeek, GLM, ...) cache a repeated prefix by themselves.
fn caches_on_request(model: &str) -> bool {
    model.starts_with("anthropic/") || model.starts_with("google/")
}

/// `messages` for a request: for a model that needs it, the system prompt and
/// the last message carry a cache breakpoint, so each turn reads the earlier
/// ones from the cache. The history itself stays as it is.
fn cached(model: &str, messages: &[Value]) -> Vec<Value> {
    let mut messages = messages.to_vec();
    if !caches_on_request(model) {
        return messages;
    }
    let last = messages.len().saturating_sub(1);
    for (i, message) in messages.iter_mut().enumerate() {
        if i != 0 && i != last {
            continue;
        }
        // A tool call with no text has no content to mark.
        if let Some(text) = message["content"].as_str().filter(|t| !t.is_empty()) {
            message["content"] = json!([
                {"type": "text", "text": text, "cache_control": {"type": "ephemeral"}}
            ]);
        }
    }
    messages
}

/// How many times a request is tried: a rate limit or a server hiccup is
/// worth a pause, not the end of the user's turn.
const ATTEMPTS: u32 = 4;

/// Posts `body` to the chat completions of an OpenAI-style `provider`
/// (OpenRouter is one) and returns the response once it is a success,
/// trying again after a pause on a network error, 429 or 5xx.
async fn open(provider: &Provider, body: &Value) -> Result<reqwest::Response> {
    let name = provider.label();
    let mut attempt = 1;
    loop {
        let mut request = client()
            .post(format!("{}/chat/completions", provider.base_url))
            .json(body);
        if !provider.key.is_empty() {
            request = request.bearer_auth(&provider.key);
        }
        for (header, value) in &provider.headers {
            request = request.header(header, value);
        }
        let (retry, error) = match request.send().await {
            Err(e) => (
                true,
                anyhow::Error::new(e).context(format!("calling {name}")),
            ),
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
                (retry, anyhow::anyhow!("{name}: {status}: {msg}"))
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

/// Posts `body` and returns the whole reply, for requests that don't stream.
async fn send(provider: &Provider, body: &Value) -> Result<Value> {
    let value: Value = open(provider, body)
        .await?
        .json()
        .await
        .with_context(|| format!("reading {}'s reply", provider.label()))?;
    if let Some(error) = value.get("error") {
        bail!(
            "{}: {}",
            provider.label(),
            error["message"].as_str().unwrap_or("no message")
        );
    }
    Ok(value)
}

/// The chat's request as a plain OpenAI-style API takes it: without
/// OpenRouter's own fields, asking for the usage at the end of a stream.
fn plain_request(body: &Value) -> Value {
    copilot::request(body)
}

/// A reply as its chunks come in.
#[derive(Default)]
struct Reply {
    text: String,
    /// The tool calls by their index in the reply: id, name, arguments so far.
    calls: Vec<(String, String, String)>,
    /// Reasoning to hand back with the tool calls, merged by `index`.
    details: Vec<Value>,
    usage: Option<Value>,
}

impl Reply {
    /// Takes one chunk of the stream; returns the text it adds.
    fn add(&mut self, chunk: &Value) -> Result<String> {
        if let Some(error) = chunk.get("error") {
            bail!(
                "the model: {}",
                error["message"].as_str().unwrap_or("no message")
            );
        }
        if let Some(usage) = chunk.get("usage").filter(|u| !u.is_null()) {
            self.usage = Some(usage.clone());
        }
        let delta = &chunk["choices"][0]["delta"];
        for call in delta["tool_calls"].as_array().into_iter().flatten() {
            let i = call["index"].as_u64().unwrap_or(0) as usize;
            while self.calls.len() <= i {
                self.calls.push(Default::default());
            }
            let slot = &mut self.calls[i];
            if let Some(id) = call["id"].as_str() {
                slot.0 = id.into();
            }
            if let Some(name) = call["function"]["name"].as_str() {
                slot.1.push_str(name);
            }
            if let Some(args) = call["function"]["arguments"].as_str() {
                slot.2.push_str(args);
            }
        }
        for part in delta["reasoning_details"].as_array().into_iter().flatten() {
            let same = |d: &&mut Value| d["index"] == part["index"] && d["type"] == part["type"];
            match self.details.iter_mut().find(same) {
                Some(have) => {
                    for (k, v) in part.as_object().into_iter().flatten() {
                        match (have[k].as_str(), v.as_str()) {
                            // Text and encrypted data arrive in pieces; the rest is whole.
                            (Some(a), Some(b))
                                if matches!(k.as_str(), "text" | "summary" | "data") =>
                            {
                                have[k] = json!(format!("{a}{b}"));
                            }
                            _ => have[k] = v.clone(),
                        }
                    }
                }
                None => self.details.push(part.clone()),
            }
        }
        let text = delta["content"].as_str().unwrap_or_default();
        self.text.push_str(text);
        Ok(text.to_string())
    }

    /// The reply as a message for the history.
    fn message(self) -> Value {
        let mut message = json!({
            "role": "assistant",
            "content": if self.text.trim().is_empty() { Value::Null } else { json!(self.text.trim()) },
        });
        let calls: Vec<Value> = self
            .calls
            .into_iter()
            .filter(|c| !c.1.is_empty())
            .map(|(id, name, arguments)| {
                json!({"id": id, "type": "function", "function": {"name": name, "arguments": arguments}})
            })
            .collect();
        if !calls.is_empty() {
            message["tool_calls"] = json!(calls);
        }
        if !self.details.is_empty() {
            message["reasoning_details"] = json!(self.details);
        }
        message
    }
}

/// The most lines `read_file` returns, and the longest a line may be.
const MAX_LINES: usize = 2000;
const MAX_LINE: usize = 2000;
/// The most paths `glob` returns, and the most matches `grep` can be asked for.
const MAX_PATHS: usize = 500;
const MAX_MATCHES: usize = 500;

/// Lines `offset` (from 1) onward, numbered, at most `limit` of them.
fn read_file(path: &str, offset: Option<u64>, limit: Option<u64>) -> Result<String> {
    let text = std::fs::read_to_string(path).context("reading the file")?;
    let start = offset.unwrap_or(1).max(1) as usize - 1;
    let limit = (limit.unwrap_or(MAX_LINES as u64) as usize).clamp(1, MAX_LINES);
    let total = text.lines().count();
    let mut out = String::new();
    for (i, line) in text.lines().enumerate().skip(start).take(limit) {
        let line = match line.char_indices().nth(MAX_LINE) {
            Some((end, _)) => format!("{}… [line cut]", &line[..end]),
            None => line.to_string(),
        };
        out.push_str(&format!("{:>6}\t{line}\n", i + 1));
    }
    if start + limit < total {
        out.push_str(&format!(
            "[{} more lines; read from offset {}]\n",
            total - start - limit,
            start + limit + 1
        ));
    } else if start >= total && total > 0 {
        out.push_str(&format!("[the file has {total} lines]\n"));
    }
    Ok(out)
}

/// The files under `root` that .gitignore leaves in, matching `glob` if given.
/// (The walker's own override globs would win over .gitignore, so the glob is
/// a filter of its own.)
fn walk(root: &str, glob: &str) -> Result<impl Iterator<Item = ignore::DirEntry>> {
    let root = if root.is_empty() { "." } else { root };
    let mut builder = ignore::WalkBuilder::new(root);
    builder.sort_by_file_path(|a, b| a.cmp(b));
    let mut matcher = ignore::gitignore::GitignoreBuilder::new(root);
    if !glob.is_empty() {
        matcher.add_line(None, glob).context("reading the glob")?;
    }
    let matcher = matcher.build().context("reading the glob")?;
    let all = glob.is_empty();
    Ok(builder.build().flatten().filter(move |e| {
        e.file_type().is_some_and(|t| t.is_file())
            && (all || matcher.matched(e.path(), false).is_ignore())
    }))
}

fn grep(pattern: &str, path: &str, glob: &str, limit: Option<u64>) -> Result<String> {
    let re = regex::Regex::new(pattern).context("reading the pattern")?;
    let limit = (limit.unwrap_or(100) as usize).clamp(1, MAX_MATCHES);
    let mut out = String::new();
    let mut found = 0;
    'files: for entry in walk(path, glob)? {
        // Binary files and unreadable ones are not text to search.
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            if !re.is_match(line) {
                continue;
            }
            let line = match line.char_indices().nth(300) {
                Some((end, _)) => format!("{}…", &line[..end]),
                None => line.to_string(),
            };
            out.push_str(&format!("{}:{}:{line}\n", entry.path().display(), i + 1));
            found += 1;
            if found == limit {
                out.push_str(&format!("[stopped at {limit} matches]\n"));
                break 'files;
            }
        }
    }
    Ok(if out.is_empty() {
        "no matches\n".into()
    } else {
        out
    })
}

fn glob(pattern: &str, path: &str) -> Result<String> {
    let mut out = String::new();
    for (found, entry) in walk(path, pattern)?.enumerate() {
        if found == MAX_PATHS {
            out.push_str(&format!("[stopped at {MAX_PATHS} files]\n"));
            break;
        }
        out.push_str(&format!("{}\n", entry.path().display()));
    }
    Ok(if out.is_empty() {
        "no files\n".into()
    } else {
        out
    })
}

/// The share of the model's window the doer may fill before uc compacts.
const COMPACT_AT: f64 = 0.7;
/// A tool's output older than the last `KEEP_RECENT` messages is cut to this.
const PRUNED: usize = 500;
const KEEP_RECENT: usize = 8;
const SUMMARY_PROMPT: &str = "You summarise a conversation between a user and a coding agent \
    so the agent can carry on from the summary alone. Write a few short paragraphs or a list: \
    what the user wants, what has been done and found (file paths, commands, decisions, errors \
    and how they were fixed), what is still to do, and anything the user said to keep in mind. \
    Keep names, paths and numbers exact. Leave out pleasantries and raw output.";

/// Cuts the output of tools in all but the latest messages to its start.
fn prune_tool_output(messages: &mut [Value]) {
    let end = messages.len().saturating_sub(KEEP_RECENT);
    for message in &mut messages[..end] {
        let Some(text) = message["content"].as_str().filter(|t| t.len() > PRUNED) else {
            continue;
        };
        if message["role"] != "tool" {
            continue;
        }
        let mut cut = PRUNED;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        message["content"] = json!(format!(
            "{}\n[older output cut to save context]",
            &text[..cut]
        ));
    }
}

/// `messages` as text for the model to summarise.
fn transcript(messages: &[Value]) -> String {
    let mut out = String::new();
    for message in messages {
        let role = message["role"].as_str().unwrap_or_default();
        if let Some(text) = content(message).filter(|t| !t.is_empty()) {
            let text: String = text.chars().take(4000).collect();
            out.push_str(&format!("[{role}] {text}\n"));
        }
        for call in message["tool_calls"].as_array().into_iter().flatten() {
            let args: String = call["function"]["arguments"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .take(500)
                .collect();
            out.push_str(&format!(
                "[tool call] {} {args}\n",
                call["function"]["name"].as_str().unwrap_or_default()
            ));
        }
    }
    out
}

fn name_of(call: &Value) -> String {
    call["function"]["name"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// The tools that only look: they can run side by side.
fn is_reading(name: &str) -> bool {
    matches!(
        name,
        "web_search" | "fetch_url" | "src" | "read_file" | "grep" | "glob"
    )
}

/// Runs one of the looking tools; failures go back to the model as its output.
async fn read_only(search: &Engine, call: &Value) -> String {
    let name = name_of(call);
    let args: Value = call["function"]["arguments"]
        .as_str()
        .and_then(|a| serde_json::from_str(a).ok())
        .unwrap_or(Value::Null);
    let arg = |k: &str| args[k].as_str().unwrap_or_default().to_string();
    let number = |k: &str| args[k].as_u64();
    let result = match name.as_str() {
        "web_search" => {
            let count = number("count").unwrap_or(10).clamp(1, 10);
            web_search(search, &arg("query"), count).await
        }
        "fetch_url" => fetch_url(&arg("url")).await,
        "src" => src(&arg("prefix")),
        "read_file" => {
            eprintln!("· read {}", arg("path"));
            read_file(&arg("path"), number("offset"), number("limit"))
        }
        "grep" => {
            eprintln!("· search for {}", arg("pattern"));
            grep(&arg("pattern"), &arg("path"), &arg("glob"), number("limit"))
        }
        "glob" => {
            eprintln!("· find {}", arg("pattern"));
            glob(&arg("pattern"), &arg("path"))
        }
        other => Err(anyhow!("there is no tool called '{other}'")),
    };
    result.unwrap_or_else(|err| format!("error: {err:#}"))
}

fn direct_command(line: &str) -> Option<&str> {
    if AI.load(Ordering::Relaxed) {
        typed_command(line).filter(|_| SHELL.load(Ordering::Relaxed))
    } else {
        Some(line.strip_prefix('!').unwrap_or(line).trim()).filter(|c| !c.is_empty())
    }
}

/// A typed `uc agent model ...` line, which the chat takes itself instead of
/// running in bash: the rest of the line after `uc agent model`, or `Some("")`
/// for `uc agent model-update`. `!uc agent model` still runs in bash, where
/// the same words save the model for the sessions to come.
fn agent_model_line(line: &str) -> Option<&str> {
    if line == "uc agent model-update" {
        return Some("");
    }
    line.strip_prefix("uc agent model")
        .filter(|rest| rest.is_empty() || rest.starts_with(' '))
}

/// `uc agent compact`, `context`, `exit` or `quit` typed at the prompt: the
/// session's own commands, which it takes instead of running them in bash;
/// the word after `uc agent`. `!uc agent context` still runs in bash.
fn agent_command(line: &str) -> Option<&'static str> {
    let rest = line.strip_prefix("uc agent ")?.trim();
    AGENT_COMMANDS.iter().find(|c| **c == rest).copied()
}

/// `command` with its first word, the program, in bright yellow: what a line
/// that runs straight away looks like at the prompt.
fn color_program(command: &str) -> String {
    let end = command.find(char::is_whitespace).unwrap_or(command.len());
    format!("\x1b[1;93m{}\x1b[0m{}", &command[..end], &command[end..])
}

/// The command line to run when what was typed is one rather than something for
/// the model: `!` and the rest, or a line starting with a program or builtin
/// that doesn't read like a sentence. Unsure means it goes to the model.
fn typed_command(line: &str) -> Option<&str> {
    if let Some(command) = line.strip_prefix('!') {
        return Some(command.trim()).filter(|c| !c.is_empty());
    }
    if line.ends_with('?') {
        return None;
    }
    let first = line.split_whitespace().next()?;
    let known = BUILTINS.contains(&first) || runnable(first) || user_shell().knows(first);
    if WORDS.contains(&first) || !known {
        return None;
    }
    let prose = unquoted(line)
        .split_whitespace()
        .skip(1)
        .any(|word| PROSE.contains(&word.to_lowercase().as_str()));
    (!prose).then_some(line)
}

/// The line without what is inside quotes: `git commit -m 'fix the bug'` is
/// still a command.
fn unquoted(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut quote = None;
    for c in line.chars() {
        match (quote, c) {
            (None, '\'' | '"') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, c) => out.push(c),
            _ => {}
        }
    }
    out
}

/// Whether a program by this name is on PATH, or at this path when it has a /.
fn runnable(name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let executable = |path: &std::path::Path| {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if name.contains('/') {
        return executable(std::path::Path::new(name));
    }
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| executable(&dir.join(name))))
}

/// The user's bash aliases and functions, read once from an interactive bash,
/// so a typed `ll` or `g status` runs as it would in their own shell.
#[derive(Default)]
struct UserShell {
    /// Their shell options, aliases and functions, run before a typed command.
    definitions: String,
    /// What their bash completes each command with, `complete -p`.
    completions: String,
    /// Each alias and what it stands for.
    aliases: BTreeMap<String, String>,
    functions: BTreeSet<String>,
    /// Where bash keeps their history, `$HISTFILE`.
    history: Option<PathBuf>,
}

impl UserShell {
    fn knows(&self, name: &str) -> bool {
        self.aliases.contains_key(name) || self.functions.contains(name)
    }

    /// The command with its first word replaced by what it is an alias for:
    /// `..` is `cd ..`.
    fn expand(&self, command: &str) -> String {
        let command = command.trim_start();
        let first = command.split_whitespace().next().unwrap_or_default();
        match self.aliases.get(first) {
            Some(value) => format!("{value}{}", &command[first.len()..]),
            None => command.to_string(),
        }
    }

    /// A bash script that runs the command with the user's definitions.
    fn script(&self, command: &str) -> String {
        format!("{}\n{command}", self.definitions)
    }

    /// What Tab offers for the last word of a command line in their bash, or
    /// only file names for a word in a line for the model; nothing when bash
    /// takes too long.
    fn complete(&self, line: &str, files: bool) -> Vec<String> {
        use std::os::unix::process::CommandExt;
        let script = format!("{}\n{}\n{COMPLETE}", self.definitions, self.completions);
        let (line, mode) = match files {
            true => (line.to_string(), "files"),
            false => (self.expand(line), ""),
        };
        let mut bash = std::process::Command::new("bash");
        bash.args(["-c", &script, "uc", &line, mode])
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: setsid is async-signal-safe and touches no memory.
        unsafe {
            bash.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(bash.output());
        });
        let Ok(Ok(output)) = rx.recv_timeout(Duration::from_secs(2)) else {
            return Vec::new();
        };
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|word| word.trim_end().to_string())
            .filter(|word| !word.is_empty())
            .collect()
    }
}

/// Completes the last word of the command line in `$1` the way the user's
/// bash does on Tab: the command's own completion, loaded by bash-completion
/// when it has one, file names when it hasn't, command names for the first
/// word.
const COMPLETE: &str = r#"
_uc_line=$1
if [[ $2 == files ]]; then compgen -f -- "$_uc_line"; exit; fi
read -ra COMP_WORDS <<< "$_uc_line"
[[ -z $_uc_line || $_uc_line == *[[:space:]] ]] && COMP_WORDS+=("")
COMP_CWORD=$(( ${#COMP_WORDS[@]} - 1 ))
COMP_LINE=$_uc_line COMP_POINT=${#_uc_line} COMP_TYPE=9 COMP_KEY=9
_uc_cur=${COMP_WORDS[COMP_CWORD]}
if (( COMP_CWORD == 0 )); then compgen -c -- "$_uc_cur"; exit; fi
_uc_cmd=${COMP_WORDS[0]}
# bash-completion's loader answers 124 once it has loaded the real one.
for _uc_try in 1 2; do
  _uc_spec=$(complete -p -- "$_uc_cmd" 2>/dev/null || complete -p -D 2>/dev/null)
  [[ $_uc_spec =~ -F\ ([^ ]+) ]] || break
  COMPREPLY=()
  "${BASH_REMATCH[1]}" "$_uc_cmd" "$_uc_cur" "${COMP_WORDS[COMP_CWORD-1]}"
  (( $? == 124 )) || break
done
if (( ${#COMPREPLY[@]} )); then printf '%s\n' "${COMPREPLY[@]}"; else compgen -f -- "$_uc_cur"; fi
"#;

/// The `uc agent` commands the prompt itself takes, besides `model`.
const AGENT_COMMANDS: &[&str] = &["compact", "context", "exit", "quit"];

/// Tab at the prompt: a command line the way the user's bash
/// completes it, with or without `!`, and file names in a line for the model.
struct Completion;

impl Completion {
    /// Where the word Tab replaces starts in `before`, and what it offers.
    fn candidates(before: &str) -> (usize, Vec<String>) {
        let word_start = before.rfind(char::is_whitespace).map_or(0, |i| {
            i + before[i..].chars().next().map_or(1, char::len_utf8)
        });
        let (offset, command) = match before.strip_prefix('!') {
            Some(command) => (1, Some(command)),
            None if word_start == 0 => (0, Some(before)),
            None => (0, typed_command(before)),
        };
        let word_start = word_start.max(offset);
        let word = &before[word_start..];
        let words = match command {
            Some(command) => user_shell().complete(command, false),
            None => user_shell().complete(word, true),
        };
        // `--opt=` and `host:` complete after the = or :, like bash does.
        let start = match words.iter().all(|w| w.starts_with(word)) {
            true => word_start,
            false => word
                .rfind(['=', ':'])
                .map_or(word_start, |i| word_start + i + 1),
        };
        let mut words: Vec<String> = words
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        // bash's `cd` completion already ends a `$CDPATH` directory with `/`.
        if let [only] = words.as_mut_slice()
            && !only.ends_with('/')
        {
            let dir = std::path::Path::new(only.as_str()).is_dir();
            only.push(if dir { '/' } else { ' ' });
        }
        (start, words)
    }
}

impl rustyline::completion::Completer for Completion {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        Ok(Self::candidates(&line[..pos]))
    }
}

impl rustyline::hint::Hinter for Completion {
    type Hint = String;
}

/// The program of a line that will run straight away turns green as it is
/// typed, so it shows before Enter whether the line goes to bash or the model.
impl rustyline::highlight::Highlighter for Completion {
    fn highlight<'l>(&self, line: &'l str, _: usize) -> std::borrow::Cow<'l, str> {
        // The chat takes `uc agent model` and the like itself; not bash lines.
        if agent_model_line(line).is_some() || agent_command(line).is_some() {
            return std::borrow::Cow::Borrowed(line);
        }
        let Some(command) = direct_command(line) else {
            return std::borrow::Cow::Borrowed(line);
        };
        // The command is a slice of the line: after a `!` and spaces, if any.
        let start = command.as_ptr() as usize - line.as_ptr() as usize;
        let end = start + command.len();
        std::borrow::Cow::Owned(format!(
            "{}{}{}",
            &line[..start],
            color_program(command),
            &line[end..]
        ))
    }

    fn highlight_char(&self, _: &str, _: usize, _: rustyline::highlight::CmdKind) -> bool {
        true
    }
}

impl rustyline::validate::Validator for Completion {}

impl rustyline::Helper for Completion {}

/// The user's shell, read on first use; empty when bash can't tell in time.
fn user_shell() -> &'static UserShell {
    static SHELL: OnceLock<UserShell> = OnceLock::new();
    SHELL.get_or_init(|| read_user_shell().unwrap_or_default())
}

/// Asks an interactive bash, with no terminal to touch, for its definitions.
/// What the bashrc prints itself comes before the first NUL and is dropped.
fn read_user_shell() -> Option<UserShell> {
    use std::os::unix::process::CommandExt;
    const DUMP: &str = "printf '\\0'; shopt -p; printf '\\0'; alias -p; printf '\\0'; \
                        complete -p; printf '\\0'; declare -F; printf '\\0'; declare -f; \
                        printf '\\0%s' \"$HISTFILE\"";
    let mut bash = std::process::Command::new("bash");
    bash.args(["-ic", DUMP])
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: setsid is async-signal-safe and touches no memory.
    unsafe {
        bash.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    // A bashrc that never finishes leaves the thread behind, not uc waiting.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(bash.output());
    });
    let output = rx.recv_timeout(Duration::from_secs(5)).ok()?.ok()?;
    parse_user_shell(&String::from_utf8_lossy(&output.stdout))
}

fn parse_user_shell(dump: &str) -> Option<UserShell> {
    let mut parts = dump.rsplitn(7, '\0');
    let (history, functions, names, completions, aliases, options) = (
        parts.next()?,
        parts.next()?,
        parts.next()?,
        parts.next()?,
        parts.next()?,
        parts.next()?,
    );
    parts.next()?;
    Some(UserShell {
        // The options first: bash-completion's functions need extglob to parse.
        definitions: format!("{options}shopt -s expand_aliases\n{aliases}{functions}"),
        completions: completions.to_string(),
        aliases: aliases
            .lines()
            .filter_map(|line| line.strip_prefix("alias ")?.split_once('='))
            .map(|(name, quoted)| {
                let value = quoted.strip_prefix('\'').and_then(|v| v.strip_suffix('\''));
                (
                    name.to_string(),
                    value.unwrap_or(quoted).replace("'\\''", "'"),
                )
            })
            .collect(),
        functions: names
            .lines()
            .filter_map(|line| line.strip_prefix("declare -f "))
            .map(str::to_string)
            .collect(),
        history: Some(PathBuf::from(history)).filter(|h| !h.as_os_str().is_empty()),
    })
}

/// Where `cd` goes, for a plain `cd`, `cd ~/x` or `cd dir`; None for anything
/// more, which bash runs.
fn cd_target(command: &str) -> Option<PathBuf> {
    let mut words = command.split_whitespace();
    if words.next() != Some("cd") {
        return None;
    }
    let (arg, rest) = (words.next(), words.next());
    if rest.is_some() || arg.is_some_and(|a| a.contains(|c: char| "$`;|&<>*?".contains(c))) {
        return None;
    }
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    match arg {
        None | Some("~") => home(),
        Some(arg) => match arg.strip_prefix("~/") {
            Some(rest) => home().map(|h| h.join(rest)),
            None => Some(in_cdpath(arg).unwrap_or_else(|| PathBuf::from(arg))),
        },
    }
}

/// The directory a relative `cd dir` finds through `$CDPATH`, like bash: not
/// for `/x`, `./x` or `../x`, and an empty entry is the working directory.
fn in_cdpath(dir: &str) -> Option<PathBuf> {
    if dir.starts_with('/')
        || dir == "."
        || dir == ".."
        || dir.starts_with("./")
        || dir.starts_with("../")
    {
        return None;
    }
    let cdpath = std::env::var_os("CDPATH")?;
    std::env::split_paths(&cdpath)
        .map(|base| match base.as_os_str().is_empty() {
            true => PathBuf::from(dir),
            false => base.join(dir),
        })
        .find(|path| path.is_dir())
}

/// Asks on the terminal. Options are numbered, with one more number last for
/// typing an answer of their own; typing one straight away works too.
fn ask_user(question: &str, options: &[String]) -> Result<String> {
    let mut prompt = format!("\x1b[1m?\x1b[0m {}\n", question.trim());
    for (i, option) in options.iter().enumerate() {
        prompt.push_str(&format!("  {}. {option}\n", i + 1));
    }
    if !options.is_empty() {
        prompt.push_str(&format!(
            "  {}. something else (type it)\n",
            options.len() + 1
        ));
    }
    prompt.push_str("> ");
    let answer = read_line(&prompt)?.unwrap_or_default();
    let answer = match pick(&answer, options) {
        Pick::Option(option) => return Ok(option),
        Pick::Own => read_line("> ")?.unwrap_or_default(),
        Pick::Typed => answer,
    };
    let answer = answer.trim();
    Ok(if answer.is_empty() {
        "(no answer)".into()
    } else {
        answer.into()
    })
}

#[derive(Debug, PartialEq)]
enum Pick {
    Option(String),
    /// The last number: they'll type their own answer next.
    Own,
    Typed,
}

fn pick(answer: &str, options: &[String]) -> Pick {
    match answer.trim().parse::<usize>() {
        Ok(n) if (1..=options.len()).contains(&n) => Pick::Option(options[n - 1].clone()),
        Ok(n) if !options.is_empty() && n == options.len() + 1 => Pick::Own,
        _ => Pick::Typed,
    }
}

/// `ls -y` is `ls`, approved up front.
fn split_approval(line: &str) -> (&str, bool) {
    match line.strip_suffix("-y") {
        Some(rest) if rest.is_empty() || rest.ends_with(char::is_whitespace) => {
            (rest.trim_end(), true)
        }
        _ => (line, false),
    }
}

fn system_prompt(model: &str, ask: bool, output: bool) -> String {
    let cwd = std::env::current_dir()
        .map(|d| d.display().to_string())
        .unwrap_or_default();
    let confirms = if ask {
        "the user confirms commands and changes"
    } else {
        "they run without asking, so be careful with anything destructive"
    };
    let sees = if output {
        "The user sees a command's output on their terminal while it runs, and a job's output \
         when you read it, so never repeat or reformat it; when a command was all they asked \
         for, say nothing after it."
    } else {
        "The user sees what you run but not its output, only your replies, so tell \
         them what they need from it: the answer, not the raw output."
    };
    format!(
        "You are uc, the usecode coding agent, running in a terminal on the user's machine, \
         on the model {model}. The working directory is {cwd}. Use the tools to \
         look around, run commands and change files; {confirms}. {sees}\n\n{MAIN_SKILL}"
    )
}

/// The tools the model has.
fn tools() -> Value {
    let tool = |name: &str, description: &str, properties: Value, required: &[&str]| {
        json!({"type": "function", "function": {
            "name": name,
            "description": description,
            "parameters": {"type": "object", "properties": properties, "required": required},
        }})
    };
    let string = |description: &str| json!({"type": "string", "description": description});
    let integer = |description: &str| json!({"type": "integer", "description": description});
    let boolean = |description: &str| json!({"type": "boolean", "description": description});
    json!([
        tool(
            "bash",
            "Run a command with bash in the working directory, without a terminal and with stdin \
             closed; returns its exit status and output (stdout and stderr together). A plain \
             `cd <dir>` on its own changes the working directory for the commands and tools \
             after it. A command still running after the timeout carries on as a job. Deleting files always asks the \
             user first. The exception is ssh-add: it runs on the user's terminal so they can \
             type the key's passphrase.",
            json!({
                "command": string("the command line"),
                "title": string("what it does in a few lowercase words, shown to the user instead of the command, like 'list the rust files' or 'build the release binary'"),
                "timeout": integer("seconds to wait before it carries on as a job; 120 if left out"),
                "background": boolean("start it as a job and return straight away, for servers and watchers"),
                "ask": boolean("ask the user first, for anything hard to undo or reaching outside this machine"),
            }),
            &["command", "title"],
        ),
        tool(
            "job_output",
            "A job's output since you last looked, and whether it is still running.",
            json!({
                "job": integer("the job's id"),
                "wait": integer("seconds to wait for it to finish first; 0 if left out"),
            }),
            &["job"],
        ),
        tool(
            "job_kill",
            "Stop a job and everything it started; returns its last output.",
            json!({"job": integer("the job's id")}),
            &["job"],
        ),
        tool(
            "ask_user",
            "Ask the user a question and get their answer: the way to get input from them in the \
             middle of a task.",
            json!({
                "question": string("the question, one line"),
                "options": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "the answers to pick from, when there are a few; they are \
                                    numbered and the user can still type their own",
                },
            }),
            &["question"],
        ),
        tool(
            "web_search",
            "Search the web; returns each result's title, URL and snippet.",
            json!({
                "query": string("what to search for"),
                "count": integer("how many results, up to 10; 10 if left out"),
            }),
            &["query"],
        ),
        tool(
            "fetch_url",
            "Load a web page and return it as text, rendered in a browser so pages built by \
             JavaScript work too; links are listed at the end.",
            json!({"url": string("the page's http or https URL")}),
            &["url"],
        ),
        tool(
            "src",
            "List the git repositories under a directory, ~/src if left out, one path relative \
             to it per line; a repository's own nested ones are not listed. Use it to find the \
             repository a task is about.",
            json!({"prefix": string("the directory to look under; ~/src if left out")}),
            &[],
        ),
        tool(
            "read_file",
            "Read a text file; returns its lines numbered. Long files come in pieces: read the \
             next with offset, or find what you need with grep first.",
            json!({
                "path": string("the file's path"),
                "offset": integer("the line to start at, 1 for the first; 1 if left out"),
                "limit": integer("how many lines, up to 2000; 2000 if left out"),
            }),
            &["path"],
        ),
        tool(
            "grep",
            "Search file contents with a regular expression, skipping what .gitignore ignores; \
             returns path:line:text for each match.",
            json!({
                "pattern": string("the regular expression"),
                "path": string("the directory or file to search; the working directory if left out"),
                "glob": string("only files whose path matches, like '*.rs' or 'src/**/*.md'"),
                "limit": integer("how many matches, up to 500; 100 if left out"),
            }),
            &["pattern"],
        ),
        tool(
            "glob",
            "Find files by name with a glob pattern, skipping what .gitignore ignores; returns \
             one path per line.",
            json!({
                "pattern": string("the pattern, like '**/*.rs' or 'src/*/mod.rs'"),
                "path": string("the directory to look under; the working directory if left out"),
            }),
            &["pattern"],
        ),
        tool(
            "write_file",
            "Create or replace a file with the given content.",
            json!({"path": string("the file's path"), "content": string("the whole new content")}),
            &["path", "content"],
        ),
        tool(
            "edit_file",
            "Replace one exact occurrence of old_text in a file with new_text.",
            json!({
                "path": string("the file's path"),
                "old_text": string("text that occurs exactly once in the file"),
                "new_text": string("its replacement"),
            }),
            &["path", "old_text", "new_text"],
        ),
        tool(
            "chat",
            "Drive the chat itself, not the machine, when the user asks for it: action model \
             switches the model for the lines after it (model: a number from the list, part \
             of a listed id or any id of the provider; without it the models of every \
             provider are listed), \
             action clear clears the screen and conversation context, action exit leaves uc.",
            json!({
                "action": string("what to do: model, clear or exit"),
                "model": string("which model to switch to, for action model: a number from \
                                 the list, part of a listed id or any id of the provider"),
                "provider": string("for action model, which provider runs it when more than \
                                    one serves it, by the name the list shows beside it"),
            }),
            &["action"],
        ),
    ])
}

impl Session {
    /// The model, as the status line and `uc agent model` say it.
    fn models(&self) -> String {
        format!(
            "model {}",
            label(&self.pool, &Choice::new(&self.provider, &self.model))
        )
    }

    /// The provider the model runs on.
    fn current(&self) -> Result<&Provider> {
        named(&self.providers, &self.provider).ok_or_else(|| anyhow!("{}", no_provider()))
    }

    /// The kind of provider the model runs on.
    fn kind(&self) -> Option<Kind> {
        self.current().ok().map(|p| p.kind)
    }

    /// Makes `choice` the model.
    fn set_model(&mut self, choice: Choice) {
        if choice.model != self.model || choice.provider != self.provider {
            self.messages[0] = json!({"role": "system", "content": system_prompt(&choice.model, self.ask, self.show_output)});
            self.model = choice.model;
            self.provider = choice.provider;
        }
    }

    /// The whole request posted to the provider; its reply is OpenAI's.
    async fn send(&self, body: &Value) -> Result<Value> {
        let provider = self.current()?;
        match provider.kind {
            Kind::OpenRouter => send(provider, body).await,
            Kind::OpenAi => send(provider, &plain_request(body)).await,
            Kind::Claude => claude::send(&provider.key, body).await,
            Kind::Copilot => copilot::send(&provider.key, body).await,
        }
    }

    /// The request streamed: the reply comes as OpenAI's SSE chunks.
    async fn stream(&self, body: &Value) -> Result<claude::Bytes> {
        let provider = self.current()?;
        let bytes = |response: reqwest::Response| -> claude::Bytes {
            Box::pin(
                response
                    .bytes_stream()
                    .map(|bytes| bytes.map(|b| b.to_vec()).map_err(std::io::Error::other)),
            )
        };
        Ok(match provider.kind {
            Kind::OpenRouter => bytes(open(provider, body).await?),
            Kind::OpenAi => bytes(open(provider, &plain_request(body)).await?),
            Kind::Claude => claude::stream(&provider.key, body).await?,
            Kind::Copilot => copilot::stream(&provider.key, body).await?,
        })
    }

    /// A line for the model: one turn, until it answers.
    async fn ask(&mut self, line: &str) -> Result<()> {
        self.messages.push(json!({"role": "user", "content": line}));
        self.turn().await.map(drop)
    }

    /// `uc agent model` without a name it knows: picks the model with the picker, starting
    /// on the current one and filtered by `query`; one match is taken
    /// straight away and Esc keeps the model. Without a terminal it is `switch`.
    fn pick_model(&mut self, query: &str) {
        let current = Choice::new(&self.provider, &self.model);
        let mut order: Vec<usize> = (0..self.pool.len()).collect();
        order.sort_by_key(|i| self.pool[*i] != current);
        let items: Vec<picker::Item> = order
            .iter()
            .map(|i| picker::Item::new(label(&self.pool, &self.pool[*i]), (i + 1).to_string()))
            .collect();
        if !picker::available() {
            self.switch(query);
            return;
        }
        let picked = picker::Picker::new("model> ")
            .reverse()
            .select_one()
            .query(query)
            .pick(items);
        if let Some(number) = picked.and_then(|p| p.values.into_iter().next()) {
            self.switch(&number);
        }
    }

    /// `uc agent model-update`: fetches every provider's model list again, keeps it
    /// in the cache and makes it the list `uc agent model` offers.
    async fn update_models(&mut self) {
        eprintln!("· fetching the model lists");
        self.pool = pool(&self.providers, true).await;
        eprintln!("· {} models", self.pool.len());
    }

    /// `uc agent model` and chat `model`: list with no name; otherwise switch to the
    /// named model, on the provider named after it if more than one serves
    /// it. The same lines go to the terminal and back to whoever asked.
    fn switch(&mut self, name: &str) -> String {
        let say = |line: String| {
            eprint!("{line}");
            line
        };
        if name.is_empty() {
            let current = Choice::new(&self.provider, &self.model);
            let mut lines = String::new();
            for (i, choice) in self.pool.iter().enumerate() {
                let mark = if *choice == current { "*" } else { " " };
                lines.push_str(&format!("{mark} {} {}\n", i + 1, label(&self.pool, choice)));
            }
            return say(lines);
        }
        match choice_named(&self.pool, name, &self.provider) {
            Some(choice) if named(&self.providers, &choice.provider).is_none() => say(format!(
                "· {} has no key for {} in {}, so it is off\n",
                choice.provider,
                providers::label(&choice.provider),
                providers::path().display()
            )),
            Some(choice) => {
                self.set_model(choice);
                say(format!("· {}\n", self.models()))
            }
            None => say(format!("· no model {name}; `uc agent model` lists them\n")),
        }
    }

    /// `uc agent model <name>` typed at the prompt: switch to the named
    /// model, and save it as the one new `uc` sessions start on, so the
    /// sessions on the machine follow the one choice. A model on a provider
    /// with no key switches nowhere and saves nothing.
    fn switch_and_save(&mut self, name: &str) {
        let saved = choice_named(&self.pool, name, &self.provider)
            .filter(|c| named(&self.providers, &c.provider).is_some());
        drop(self.switch(name));
        let Some(choice) = saved else {
            return;
        };
        match save_model_choice(&choice) {
            Ok(path) => {
                eprintln!("· saved in {} for new `uc` sessions", path.display());
            }
            Err(err) => eprintln!("· not saved: {err:#}"),
        }
    }

    /// Clears the screen and the conversation's context, like Ctrl-L at the
    /// prompt: what was said is gone, and the next line starts fresh. Called
    /// in the middle of a turn, the messages wait until the turn has ended —
    /// its tool calls have to be answered in the same context — and then the
    /// prompt loop drops them, as CLEARED says.
    fn clear(&mut self) -> String {
        print!("\x1b[H\x1b[2J\x1b[3J");
        let _ = std::io::stdout().flush();
        CLEARED.store(true, Ordering::Relaxed);
        eprintln!("· context cleared");
        "context cleared".into()
    }

    /// Leaves uc: stops the jobs, puts the terminal back as it was and exits.
    fn exit(&mut self) -> ! {
        stop_jobs();
        restore_terminal(&mut saved());
        eprintln!("· leaving uc");
        std::process::exit(0);
    }

    /// The doer's last request filled `COMPACT_AT` of its window.
    async fn near_window(&self) -> bool {
        let Some(prompt) = self
            .last_usage
            .as_ref()
            .and_then(|u| u["prompt_tokens"].as_u64())
        else {
            return false;
        };
        let window = match model_info(&self.model).await {
            Ok(Some((Some(window), _))) => window,
            _ => return false,
        };
        prompt as f64 >= COMPACT_AT * window as f64
    }

    /// `uc agent compact`: cuts old tool output, then, if the conversation has earlier
    /// turns, replaces them with a summary. The latest turn stays as it is.
    async fn compact(&mut self) -> Result<()> {
        let before = self.messages.len();
        prune_tool_output(&mut self.messages);
        // The turns before the last line the user typed; a tool call and its
        // answers are never split.
        let last_user = self.messages.iter().rposition(|m| m["role"] == "user");
        if let Some(last_user) = last_user.filter(|i| *i > 1) {
            let older: Vec<Value> = self.messages[1..last_user].to_vec();
            let summary = self.summarize(&older).await?;
            let tail = self.messages.split_off(last_user);
            self.messages.truncate(1);
            self.messages.push(json!({
                "role": "user",
                "content": format!("Summary of our conversation so far, written to carry on from:\n\n{summary}"),
            }));
            self.messages.push(json!({"role": "assistant", "content": "Understood, carrying on from the summary."}));
            self.messages.extend(tail);
        }
        // The count was of the longer conversation.
        self.last_usage = None;
        eprintln!(
            "· compacted the context: {} messages, was {before}",
            self.messages.len()
        );
        Ok(())
    }

    /// What the model makes of `messages`, in a few paragraphs.
    async fn summarize(&mut self, messages: &[Value]) -> Result<String> {
        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": SUMMARY_PROMPT},
                {"role": "user", "content": transcript(messages)},
            ],
            "reasoning": {"effort": "low"},
        });
        let _clock = std::io::stderr()
            .is_terminal()
            .then(|| Clock(tokio::spawn(spin("summarising the conversation".into()))));
        let value = self.send(&body).await?;
        if let Some(cost) = value["usage"]["cost"].as_f64() {
            self.spent += cost;
        }
        value["choices"][0]["message"]["content"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .ok_or_else(|| anyhow!("the model wrote no summary"))
    }

    /// `uc agent context`: how much of the model's context the conversation has
    /// taken. The real count is from the last reply's `usage`, an estimate
    /// from the bytes before that, and the window is the model's
    /// `context_length` on OpenRouter.
    async fn context(&self) {
        // About 4 characters a token: a rough count until the model has
        // said what the last turn took.
        let bytes: usize = self
            .messages
            .iter()
            .map(|m| content(m).map(str::len).unwrap_or_default())
            .sum();
        eprintln!(
            "· {} messages, about {} tokens",
            self.messages.len(),
            bytes / 4
        );
        let tokens = self
            .last_usage
            .as_ref()
            .and_then(|u| u["total_tokens"].as_u64());
        if let Some(total) = tokens {
            eprint!("· last turn {total} tokens");
            if let Some(prompt) = self
                .last_usage
                .as_ref()
                .and_then(|u| u["prompt_tokens"].as_u64())
            {
                eprint!(" ({prompt} in");
                if let Some(cached) = self
                    .last_usage
                    .as_ref()
                    .and_then(|u| u["prompt_tokens_details"]["cached_tokens"].as_u64())
                    .filter(|c| *c > 0)
                {
                    eprint!(", {cached} cached");
                }
                eprint!(")");
            }
            match self
                .last_usage
                .as_ref()
                .and_then(|u| u["completion_tokens"].as_u64())
            {
                Some(completion) => eprintln!(", {completion} out"),
                None => eprintln!(),
            }
        } else {
            eprintln!("· no turn yet, so no token count; that comes with the first reply");
        }
        match model_info(&self.model).await {
            Ok(Some((window, per_million))) => {
                if let (Some(window), Some(total)) = (window, tokens) {
                    let pct = 100.0 * total as f64 / window as f64;
                    eprintln!(
                        "· model {}'s window is {} tokens, {:.1}% of it used",
                        self.model, window, pct
                    );
                }
                if self.spent > 0.0 {
                    let per_million = per_million.unwrap_or_default();
                    eprintln!(
                        "· spent about ${:.4} this session, input ${:.2} and output ${:.2} per million tokens",
                        self.spent, per_million.0, per_million.1
                    );
                }
            }
            Ok(None) => eprintln!(
                "· model {}'s window and price aren't on OpenRouter's list",
                self.model
            ),
            Err(err) => eprintln!("· OpenRouter's model list: {err:#}"),
        }
    }

    /// Asks the model until it answers without calling a tool; returns that
    /// answer.
    async fn turn(&mut self) -> Result<String> {
        loop {
            if self.near_window().await
                && let Err(err) = self.compact().await
            {
                eprintln!("· could not compact the context: {err:#}");
            }
            let message = self.complete().await?;
            let text = message["content"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_string();
            let calls = message["tool_calls"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            self.messages.push(message);
            if calls.is_empty() {
                return Ok(text);
            }
            // Looking around changes nothing, so a batch of it runs at once.
            if calls.len() > 1 && calls.iter().all(|c| is_reading(&name_of(c))) {
                let outputs = futures_util::future::join_all(
                    calls.iter().map(|call| read_only(&self.search, call)),
                )
                .await;
                for (call, output) in calls.iter().zip(outputs) {
                    self.messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call["id"],
                        "content": truncate(output),
                    }));
                }
                continue;
            }
            for call in calls {
                let output = self.call(&call).await;
                self.messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call["id"],
                    "content": truncate(output),
                }));
            }
        }
    }

    /// One request to the model; its text is printed as it arrives.
    async fn complete(&mut self) -> Result<Value> {
        let messages = &self.messages;
        // OpenRouter and Claude take cache breakpoints; the others, plain
        // messages.
        let messages = if !matches!(self.kind(), Some(Kind::OpenRouter | Kind::Claude)) {
            messages.clone()
        } else {
            cached(&self.model, messages)
        };
        let body = json!({
            "model": self.model,
            "messages": messages,
            "tools": tools(),
            "reasoning": {"effort": self.effort},
            "stream": true,
            "usage": {"include": true},
        });
        let what = format!("\x1b[94m{}\x1b[0m", any(&DOING));
        let mut clock = std::io::stderr()
            .is_terminal()
            .then(|| Clock(tokio::spawn(spin(what))));
        let mut stream = self.stream(&body).await?;
        let mut pending = Vec::new();
        let mut reply = Reply::default();
        let mut shown = false;
        'stream: while let Some(bytes) = stream.next().await {
            pending.extend_from_slice(&bytes.context("reading the model's reply")?);
            while let Some(end) = pending.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = pending.drain(..=end).collect();
                let line = String::from_utf8_lossy(&line);
                // Comments (": OPENROUTER PROCESSING") keep the connection alive.
                let Some(data) = line.trim().strip_prefix("data:").map(str::trim) else {
                    continue;
                };
                if data == "[DONE]" {
                    break 'stream;
                }
                let Ok(chunk) = serde_json::from_str::<Value>(data) else {
                    continue;
                };
                let mut text = reply.add(&chunk)?;
                if !shown {
                    text = text.trim_start().to_string();
                }
                if text.is_empty() {
                    continue;
                }
                clock = None;
                shown = true;
                print!("{text}");
                let _ = std::io::stdout().flush();
            }
        }
        drop(clock);
        if shown {
            println!("\n");
        }
        // What the last turn took: `cost` is what OpenRouter charged, the
        // token counts what it says the messages came to.
        if let Some(usage) = reply.usage.clone() {
            if let Some(cost) = usage["cost"].as_f64() {
                self.spent += cost;
            }
            self.last_usage = Some(usage);
        }
        let message = reply.message();
        if message["content"].is_null() && message.get("tool_calls").is_none() {
            bail!("{} sent no message", providers::label(&self.provider));
        }
        Ok(message)
    }

    /// Runs one tool call; failures go back to the model as its output.
    async fn call(&mut self, call: &Value) -> String {
        let name = call["function"]["name"].as_str().unwrap_or_default();
        let args: Value = call["function"]["arguments"]
            .as_str()
            .and_then(|a| serde_json::from_str(a).ok())
            .unwrap_or(Value::Null);
        let arg = |k: &str| args[k].as_str().unwrap_or_default().to_string();
        let flag = |k: &str| args[k].as_bool().unwrap_or(false);
        let number = |k: &str| args[k].as_u64();
        let job = number("job").unwrap_or_default() as usize;
        let result = match name {
            "bash" => {
                let timeout = number("timeout").unwrap_or(DEFAULT_TIMEOUT);
                let (command, title) = (arg("command"), arg("title"));
                self.bash(&command, &title, timeout, flag("background"), flag("ask"))
                    .await
            }
            "job_output" => self.job_output(job, number("wait").unwrap_or(0)).await,
            "job_kill" => self.job_kill(job).await,
            "ask_user" => {
                let options: Vec<String> = args["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|o| o.as_str().map(|o| o.trim().to_string()))
                    .filter(|o| !o.is_empty())
                    .collect();
                ask_user(&arg("question"), &options)
            }
            "web_search" | "fetch_url" | "src" | "read_file" | "grep" | "glob" => {
                return read_only(&self.search, call).await;
            }
            "write_file" => self.write_file(&arg("path"), &arg("content")),
            "edit_file" => self.edit_file(&arg("path"), &arg("old_text"), &arg("new_text")),
            "chat" => match arg("action").trim() {
                "model" => Ok(self.switch(format!("{} {}", arg("model"), arg("provider")).trim())),
                "clear" => Ok(self.clear()),
                "exit" => self.exit(),
                other => Err(anyhow!(
                    "there is no chat action '{other}'; model, clear or exit"
                )),
            },
            other => Err(anyhow!("there is no tool called '{other}'")),
        };
        result.unwrap_or_else(|err| format!("error: {err:#}"))
    }

    async fn bash(
        &mut self,
        command: &str,
        title: &str,
        timeout: u64,
        background: bool,
        ask: bool,
    ) -> Result<String> {
        let asks = ask || deletes(command) || !(self.allow_all || self.allow_turn);
        let title = title.trim();
        if self.show_commands || title.is_empty() || asks {
            eprintln!("{}{}", prompt(), color_program(command));
        } else {
            eprintln!("· {title}");
        }
        self.confirm("run it?", ask || deletes(command))?;
        if let Some(dir) = cd_target(command) {
            std::env::set_current_dir(&dir).with_context(|| format!("cd {}", dir.display()))?;
            let cwd = std::env::current_dir().unwrap_or(dir);
            return Ok(format!("the working directory is now {}", cwd.display()));
        }
        if on_terminal(command) {
            return self.run_on_terminal(command).await;
        }
        let id = self.start(command, !background && self.show_output)?;
        if background {
            eprintln!("· job {id} started");
            return Ok(format!("started job {id}"));
        }
        self.waiting = Some(id);
        self.wait(id, timeout).await;
        self.waiting = None;
        let job = &self.jobs[&id];
        job.echo.store(false, Ordering::Relaxed);
        if job.status.is_none() {
            eprintln!("· still running after {timeout}s, as job {id}");
        }
        let (status, text) = self.take(id);
        Ok(format!("{status}\noutput:\n{text}"))
    }

    /// Runs a command in the foreground with the terminal as its stdin and
    /// output, so the user can type a passphrase into it.
    async fn run_on_terminal(&mut self, command: &str) -> Result<String> {
        let status = on_terminal_status(command).await?;
        Ok(format!(
            "{status}\noutput: (it ran on the user's terminal, not captured)"
        ))
    }

    /// A command the user typed: it runs on their terminal like in a shell,
    /// after the same confirmations as the model's. The model hears nothing
    /// about it, except for `cd`, which changes the working directory
    /// everything the model does afterwards happens in.
    async fn run_typed(&mut self, command: &str) -> Result<()> {
        let shell = user_shell();
        self.confirm("run it?", deletes(&shell.expand(command)))?;
        match cd_target(&shell.expand(command)) {
            Some(dir) => {
                FAILED.store(true, Ordering::Relaxed);
                std::env::set_current_dir(&dir).with_context(|| format!("cd {}", dir.display()))?;
                FAILED.store(false, Ordering::Relaxed);
                let cwd = std::env::current_dir().unwrap_or(dir);
                self.messages.push(json!({"role": "user",
                           "content": format!(
                               "I changed the working directory to {}.", cwd.display())}));
            }
            None => {
                FAILED.store(true, Ordering::Relaxed);
                let status = on_terminal_status(&shell.script(command)).await?;
                FAILED.store(!status.success(), Ordering::Relaxed);
                if !status.success() {
                    eprintln!("· {status}");
                }
            }
        };
        Ok(())
    }

    fn start(&mut self, command: &str, echo: bool) -> Result<usize> {
        let mut bash = tokio::process::Command::new("bash");
        bash.args(["-c", command])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Its own session, so job_kill stops a whole pipeline, and ssh can't
        // open the terminal to ask for a passphrase: it fails instead of
        // stopping on SIGTTIN and hanging.
        // SAFETY: setsid is async-signal-safe and touches no memory.
        unsafe {
            bash.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = bash.spawn().context("starting bash")?;
        let group = child.id().context("bash has no pid")? as i32;
        GROUPS.lock().unwrap().insert(group);
        let output = Arc::new(Mutex::new(Output::default()));
        let echo = Arc::new(AtomicBool::new(echo));
        let readers = vec![
            pump(child.stdout.take(), output.clone(), echo.clone()),
            pump(child.stderr.take(), output.clone(), echo.clone()),
        ];
        let id = self.next_job;
        self.next_job += 1;
        self.jobs.insert(
            id,
            Job {
                child,
                group,
                output,
                seen: 0,
                echo,
                readers,
                status: None,
            },
        );
        Ok(id)
    }

    /// Waits up to `secs` for a job to finish.
    async fn wait(&mut self, id: usize, secs: u64) {
        let Some(job) = self.jobs.get_mut(&id) else {
            return;
        };
        if job.status.is_some() {
            return;
        }
        // Without its output on screen, a long wait would look like uc hung.
        let quiet = !job.echo.load(Ordering::Relaxed) && std::io::stderr().is_terminal();
        let what = format!("\x1b[94m{}\x1b[0m", any(&RUNNING));
        let clock = quiet.then(|| Clock(tokio::spawn(spin(what))));
        let waited = tokio::time::timeout(Duration::from_secs(secs), job.child.wait()).await;
        drop(clock);
        if let Ok(Ok(status)) = waited {
            job.status = Some(status);
            GROUPS.lock().unwrap().remove(&job.group);
            // Let the pipes drain, unless something it left running holds them.
            for reader in job.readers.drain(..) {
                let _ = tokio::time::timeout(Duration::from_secs(1), reader).await;
            }
        }
    }

    /// A job's status and its output since last time; a finished job is
    /// forgotten once this has been said.
    fn take(&mut self, id: usize) -> (String, String) {
        let Some(job) = self.jobs.get_mut(&id) else {
            return (format!("there is no job {id}"), String::new());
        };
        let (text, seen) = job.output.lock().unwrap().since(job.seen);
        job.seen = seen;
        let status = match job.status {
            Some(status) => status.to_string(),
            None => format!("still running as job {id}"),
        };
        if job.status.is_some() {
            self.jobs.remove(&id);
        }
        (status, text)
    }

    async fn job_output(&mut self, id: usize, wait: u64) -> Result<String> {
        if !self.jobs.contains_key(&id) {
            bail!("there is no job {id}");
        }
        self.wait(id, wait).await;
        let (status, text) = self.take(id);
        if self.show_output {
            eprint!("{text}");
        }
        eprintln!("· job {id}: {status}");
        Ok(format!("{status}\noutput:\n{text}"))
    }

    async fn job_kill(&mut self, id: usize) -> Result<String> {
        let job = self
            .jobs
            .get(&id)
            .ok_or_else(|| anyhow!("there is no job {id}"))?;
        let group = job.group;
        // bash exiting isn't enough: the rest of the pipeline may still run.
        for signal in [libc::SIGTERM, libc::SIGKILL] {
            signal_group(group, signal);
            self.wait(id, 3).await;
            if group_gone(group, 3).await {
                break;
            }
        }
        eprintln!("· job {id} stopped");
        let (status, text) = self.take(id);
        Ok(format!("{status}\noutput:\n{text}"))
    }

    fn write_file(&mut self, path: &str, content: &str) -> Result<String> {
        eprintln!("· write {path} ({} lines)", content.lines().count());
        // Over a file that is there, what changes is worth seeing.
        if let Ok(before) = std::fs::read_to_string(path) {
            eprint!("{}", diff::render(&before, content, diff::color()));
        }
        self.confirm("write it?", false)?;
        if let Some(dir) = std::path::Path::new(path).parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir).context("creating the directory")?;
        }
        std::fs::write(path, content).context("writing the file")?;
        Ok(format!("wrote {path}"))
    }

    fn edit_file(&mut self, path: &str, old: &str, new: &str) -> Result<String> {
        let text = std::fs::read_to_string(path).context("reading the file")?;
        match text.matches(old).count() {
            1 => {}
            0 => bail!("old_text is not in {path}"),
            n => bail!("old_text occurs {n} times in {path}; give more context"),
        }
        let edited = text.replacen(old, new, 1);
        eprintln!("· edit {path}");
        eprint!("{}", diff::render(&text, &edited, diff::color()));
        self.confirm("apply it?", false)?;
        std::fs::write(path, edited).context("writing the file")?;
        Ok(format!("edited {path}"))
    }

    /// After Esc: the command bash was waiting on is stopped, and the model
    /// is told the tool calls it was left with were stopped.
    fn stopped(&mut self) {
        if let Some(job) = self.waiting.take().and_then(|id| self.jobs.remove(&id)) {
            signal_group(job.group, libc::SIGKILL);
            GROUPS.lock().unwrap().remove(&job.group);
        }
        if let Some(at) = self.messages.iter().rposition(|m| m["role"] == "assistant") {
            let answered: Vec<Value> = self.messages[at + 1..]
                .iter()
                .map(|m| m["tool_call_id"].clone())
                .collect();
            let calls = self.messages[at]["tool_calls"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for call in calls.iter().filter(|c| !answered.contains(&c["id"])) {
                self.messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call["id"],
                    "content": "stopped: the user pressed Esc",
                }));
            }
        }
        self.messages
            .push(json!({"role": "user", "content": "I pressed Esc and stopped you there."}));
    }

    /// Ok to go ahead; an error tells the model the user said no. `always`
    /// asks even in auto mode or after `-y`.
    fn confirm(&mut self, question: &str, always: bool) -> Result<()> {
        if !always && (self.allow_all || self.allow_turn) {
            return Ok(());
        }
        let answer = read_line(&format!("{question} [y/N/a] "))?.unwrap_or_default();
        match answer.trim().to_lowercase().as_str() {
            "y" | "yes" => Ok(()),
            "a" | "all" => {
                self.allow_all = true;
                Ok(())
            }
            _ => bail!("the user declined"),
        }
    }
}

/// Runs a command with the terminal as its stdin and output; Ctrl-C goes to it.
async fn on_terminal_status(command: &str) -> Result<ExitStatus> {
    let _lent = lend_terminal();
    let _foreground = Foreground::take();
    // Stopped with Esc, the command goes too rather than staying on the
    // terminal next to the prompt.
    tokio::process::Command::new("bash")
        .args(["-c", command])
        .kill_on_drop(true)
        .status()
        .await
        .context("starting bash")
}

/// Ctrl-C goes to the command on the terminal until dropped, even when the
/// wait for it is.
struct Foreground;

impl Foreground {
    fn take() -> Self {
        FOREGROUND.store(true, Ordering::Relaxed);
        Foreground
    }
}

impl Drop for Foreground {
    fn drop(&mut self) {
        FOREGROUND.store(false, Ordering::Relaxed);
    }
}

/// The running clock of a wait, cleared away when dropped.
struct Clock(JoinHandle<()>);

impl Drop for Clock {
    fn drop(&mut self) {
        self.0.abort();
        eprint!("\r\x1b[2K");
    }
}

/// Resolves when Esc is pressed on the terminal; never without one. Until it
/// is dropped, keys typed meanwhile aren't echoed and are dropped.
async fn escape() {
    if !std::io::stdin().is_terminal() {
        return std::future::pending().await;
    }
    let (pressed, rx) = tokio::sync::oneshot::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let thread = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            if watch_escape(&stop) {
                let _ = pressed.send(());
            }
        })
    };
    let _watching = Watching(stop, Some(thread));
    if rx.await.is_err() {
        std::future::pending::<()>().await;
    }
}

/// Stops the Esc watcher and waits for it to put the terminal back, so the
/// next prompt finds it as it was.
struct Watching(Arc<AtomicBool>, Option<std::thread::JoinHandle<()>>);

impl Drop for Watching {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
        if let Some(thread) = self.1.take() {
            let _ = thread.join();
        }
    }
}

/// Reads the terminal a key at a time, without echo, until Esc or `stop`;
/// whether it was Esc. It lets go while a prompt or command has the terminal.
fn watch_escape(stop: &AtomicBool) -> bool {
    let pressed = loop {
        if stop.load(Ordering::Relaxed) {
            break false;
        }
        if LENT.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        // Put the terminal in cbreak with `SAVED` locked only that long:
        // after this the lock is left free for whoever hands the terminal
        // back (Ctrl-C, `chat exit`), while `LENT` says whether it was.
        // `LENT` is looked at again under the lock `lend_terminal` sets it
        // under, so the terminal isn't put in cbreak just after it was lent.
        {
            let mut saved = saved();
            if LENT.load(Ordering::Relaxed) {
                continue;
            }
            if saved.is_none() {
                let old = cbreak();
                if old.is_none() {
                    break false;
                }
                *saved = old;
            }
        }
        let mut fd = libc::pollfd {
            fd: 0,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one pollfd, and it is valid for the call.
        let ready = unsafe { libc::poll(&mut fd, 1, 50) };
        if ready < 0 {
            continue;
        }
        if ready == 0 {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        // Lent while it polled: the keys are the prompt's or the command's.
        if LENT.load(Ordering::Relaxed) {
            continue;
        }
        let mut buf = [0u8; 64];
        // SAFETY: reads at most buf.len() bytes into buf.
        let n = unsafe { libc::read(0, buf.as_mut_ptr().cast(), buf.len()) };
        // Arrow keys start with Esc too, but come with the rest in one read.
        if n == 1 && buf[0] == 0x1b {
            break true;
        }
        if n <= 0 {
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    restore_terminal(&mut saved());
    pressed
}

/// Turns off line buffering and echo on the terminal, keeping Ctrl-C and the
/// output as they are; the settings from before, None if it isn't one.
fn cbreak() -> Option<libc::termios> {
    // SAFETY: termios is plain data, filled in by tcgetattr before it is used.
    let mut old: libc::termios = unsafe { std::mem::zeroed() };
    // SAFETY: both calls only read or write the termios passed to them.
    unsafe {
        if libc::tcgetattr(0, &mut old) != 0 {
            return None;
        }
        let mut new = old;
        new.c_lflag &= !(libc::ICANON | libc::ECHO);
        new.c_cc[libc::VMIN] = 1;
        new.c_cc[libc::VTIME] = 0;
        if libc::tcsetattr(0, libc::TCSANOW, &new) != 0 {
            return None;
        }
    }
    Some(old)
}

fn restore_terminal(saved: &mut Option<libc::termios>) {
    if let Some(old) = saved.take() {
        // SAFETY: only reads the termios passed to it.
        unsafe { libc::tcsetattr(0, libc::TCSANOW, &old) };
    }
}

/// Gives the terminal to a prompt or a command that reads it, as it was before
/// the Esc watcher, until dropped.
struct Lent;

fn lend_terminal() -> Lent {
    let mut saved = saved();
    LENT.store(true, Ordering::Relaxed);
    restore_terminal(&mut saved);
    Lent
}

impl Drop for Lent {
    fn drop(&mut self) {
        LENT.store(false, Ordering::Relaxed);
    }
}

/// The braille dots the ₿ turns through while uc waits, a step every 100ms.
const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// What the spinner says while the model works, one picked per request.
const DOING: [&str; 7] = [
    "crafting",
    "forging",
    "orchestrating",
    "wrangling",
    "tinkering",
    "conjuring",
    "assembling",
];

/// What the spinner says while a command runs quietly, one picked per run.
const RUNNING: [&str; 7] = [
    "churning",
    "grinding",
    "humming",
    "chugging",
    "whirring",
    "crunching",
    "toiling",
];

/// One of `words`, picked by the clock.
fn any(words: &[&'static str]) -> &'static str {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos() as usize);
    words[nanos % words.len()]
}

/// The braille spinner turning in the default text color next to what uc
/// waits on, until aborted.
async fn spin(what: String) {
    for dot in SPINNER.into_iter().cycle() {
        eprint!("\r\x1b[2K{dot} {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// `user@host dir ₿ ` in the colors of the dotfiles' .bashrc prompt: the
/// terminal theme's own, so it looks the same in Gruvbox dark and light.
fn prompt() -> String {
    let user = ["USER", "LOGNAME"]
        .into_iter()
        .find_map(|v| std::env::var(v).ok().filter(|u| !u.is_empty()))
        .unwrap_or_else(|| "?".to_string());
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default();
    let host = host.trim().split('.').next().unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let dir = if home.as_deref() == Some(cwd.as_path()) {
        "~".to_string()
    } else {
        cwd.file_name()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_else(|| cwd.display().to_string())
    };
    // SAFETY: geteuid can't fail and touches no memory.
    let who = if unsafe { libc::geteuid() } == 0 {
        "1;91"
    } else {
        "1;92"
    };
    let mark = if FAILED.load(Ordering::Relaxed) {
        "1;91"
    } else {
        "0"
    };
    format!("\x1b[{who}m{user}\x1b[0m@{host} \x1b[1;94m{dir} \x1b[{mark}m₿\x1b[0m ")
}

/// The line editor every prompt reads through, so arrow keys edit the line.
static EDITOR: Mutex<Option<Editor<Completion, DefaultHistory>>> = Mutex::new(None);

/// How many earlier lines Up and Down go through: all of them, in practice.
const HISTORY_SIZE: usize = 1_000_000;

/// Where the lines typed at the main prompt are kept between sessions.
fn history_path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(state.join("usecode/chat_history"))
}

/// Where bash keeps what the user typed in it; `~/.bash_history` if bash
/// can't tell.
fn bash_history_path() -> Option<PathBuf> {
    user_shell().history.clone().or_else(|| {
        std::env::var_os("HISTFILE")
            .filter(|f| !f.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".bash_history")))
    })
}

/// The entries of a history file kept the way bash keeps it, oldest first,
/// each with the time on the `#1700000000` line before it. One without a
/// time has the time of the one before.
fn timed_lines(text: &str) -> Vec<(u64, String)> {
    let mut time = 0;
    let mut lines = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match line.strip_prefix('#').and_then(|t| t.parse().ok()) {
            Some(t) => time = t,
            None => lines.push((time, line.to_string())),
        }
    }
    lines
}

/// Bash's lines and uc's in the order they were typed, oldest first, so Up
/// in uc goes back through what was typed in bash as well.
fn merged_lines(bash: &str, uc: &str) -> Vec<String> {
    let mut lines = timed_lines(bash);
    lines.extend(timed_lines(uc));
    // Stable: lines typed in the same second keep their order.
    lines.sort_by_key(|(time, _)| *time);
    lines.into_iter().map(|(_, line)| line).collect()
}

/// Every earlier line, bash's and uc's, oldest first.
fn past_lines() -> Vec<String> {
    let read = |path: Option<PathBuf>| {
        let text = path.and_then(|p| std::fs::read(p).ok()).unwrap_or_default();
        String::from_utf8_lossy(&text).into_owned()
    };
    merged_lines(&read(bash_history_path()), &read(history_path()))
}

/// Keeps a line typed at the main prompt the way bash would, after a `#` line
/// with the time, so it reads back in order with bash's.
fn keep_line(line: &str) -> std::io::Result<()> {
    let path = history_path().ok_or(std::io::ErrorKind::NotFound)?;
    std::fs::create_dir_all(path.parent().unwrap_or(&path))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "#{now}\n{}", line.replace('\n', " "))
}

/// Rewrites a history file from before uc kept the time, rustyline's `#V2`
/// one, the way bash keeps it.
fn upgrade_history() -> std::io::Result<()> {
    let path = history_path().ok_or(std::io::ErrorKind::NotFound)?;
    let text = std::fs::read_to_string(&path)?;
    if !text.starts_with("#V2\n") {
        return Ok(());
    }
    let lines: String = history_lines(&text)
        .iter()
        .map(|l| format!("{}\n", l.replace('\n', " ")))
        .collect();
    std::fs::write(path, lines)
}

/// A line typed at the main prompt; Up and Down go through the earlier ones,
/// this session's and the ones before. None at end of input.
fn read_prompt(prompt: &str) -> Result<Option<String>> {
    let line = read_line(prompt)?;
    if let Some(line) = line.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
        let mut editor = EDITOR.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(editor) = editor.as_mut() {
            let _ = editor.add_history_entry(line);
        }
        TYPED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(line.to_string());
        let _ = keep_line(line);
    }
    Ok(line)
}

/// Ctrl-R picks an earlier line with the picker, newest first, like fzf does
/// in bash.
/// Without a terminal it is the usual reverse search.
struct PickHistory;

impl ConditionalEventHandler for PickHistory {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, ctx: &EventContext) -> Option<Cmd> {
        let mut seen = BTreeSet::new();
        let mut lines = Vec::new();
        for line in past_lines().into_iter().rev() {
            if seen.insert(line.clone()) {
                lines.push(line);
            }
        }
        if !picker::available() {
            return None;
        }
        let picked = picker::Picker::new("> ")
            .query(ctx.line())
            .pick(lines.into_iter().map(picker::Item::line).collect());
        match picked.and_then(|p| p.values.into_iter().next()) {
            None => Some(Cmd::Repaint),
            Some(line) => Some(Cmd::Replace(Movement::WholeBuffer, Some(line))),
        }
    }
}

/// Every earlier line, bash's and uc's, to tell one Up put there from one
/// typed.
static TYPED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Up or Down went to the previous or next line without a prefix, and the
/// next one should too.
static BROWSING: AtomicBool = AtomicBool::new(false);

/// Up and Down find the earlier lines that start with what is typed before
/// the cursor, like bash's `history-search-backward`. On an empty line they
/// go through every earlier line, as rustyline's search finds nothing for an
/// empty prefix. Inside a line of several, they move between its lines
/// first.
struct PrefixHistory(SearchDirection);

impl ConditionalEventHandler for PrefixHistory {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, ctx: &EventContext) -> Option<Cmd> {
        let (before, after) = ctx.line().split_at(ctx.pos());
        // A line an earlier Up put there, not edited since: the cursor is
        // at its end, so `before` is all of it, not a prefix.
        let browsing = BROWSING.load(Ordering::Relaxed)
            && after.is_empty()
            && TYPED
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains(before);
        let plain = before.is_empty() || browsing;
        BROWSING.store(plain, Ordering::Relaxed);
        match self.0 {
            SearchDirection::Reverse if plain => Some(Cmd::PreviousHistory),
            SearchDirection::Forward if plain => Some(Cmd::NextHistory),
            SearchDirection::Reverse if !before.contains('\n') => Some(Cmd::HistorySearchBackward),
            SearchDirection::Forward if !after.contains('\n') => Some(Cmd::HistorySearchForward),
            _ => None,
        }
    }
}

/// The entries of a rustyline history file, the one uc kept before it kept
/// the time, oldest first; `#V2` files escape
/// newlines and backslashes.
fn history_lines(text: &str) -> Vec<String> {
    let mut lines = text.lines();
    let v2 = text.starts_with("#V2\n");
    if v2 {
        lines.next();
    }
    lines
        .filter(|l| !l.is_empty())
        .map(|l| {
            if !v2 {
                return l.to_string();
            }
            let mut out = String::with_capacity(l.len());
            let mut chars = l.chars();
            while let Some(c) = chars.next() {
                match (c, chars.clone().next()) {
                    ('\\', Some('n')) => {
                        chars.next();
                        out.push('\n');
                    }
                    ('\\', Some(_)) => out.extend(chars.next()),
                    (c, _) => out.push(c),
                }
            }
            out
        })
        .collect()
}

/// Ctrl-C at a question the model asked: it stops the model, like Esc.
#[derive(Debug)]
struct Stopped;

/// Ctrl-L at the prompt: clears the screen, and the model's context with it,
/// the way a fresh uc would start. The line being typed is kept.
struct ClearScreen;

impl ConditionalEventHandler for ClearScreen {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, _: &EventContext) -> Option<Cmd> {
        CLEARED.store(true, Ordering::Relaxed);
        print!("\x1b[H\x1b[2J\x1b[3J");
        let _ = std::io::stdout().flush();
        Some(Cmd::Repaint)
    }
}

impl std::fmt::Display for Stopped {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("stopped")
    }
}

impl std::error::Error for Stopped {}

/// A line from stdin after `prompt`; None at end of input. Ctrl-C quits uc,
/// or stops the model when it is working on a line.
fn read_line(prompt: &str) -> Result<Option<String>> {
    let _lent = lend_terminal();
    let mut editor = EDITOR.lock().unwrap_or_else(|e| e.into_inner());
    if editor.is_none() {
        let config = Config::builder()
            .max_history_size(HISTORY_SIZE)?
            .completion_type(CompletionType::List)
            .build();
        let mut new = Editor::with_config(config).context("opening the terminal")?;
        new.set_helper(Some(Completion));
        let _ = upgrade_history();
        let mut typed = TYPED.lock().unwrap_or_else(|e| e.into_inner());
        for line in past_lines() {
            let _ = new.add_history_entry(line.as_str());
            typed.insert(line);
        }
        drop(typed);
        new.bind_sequence(
            KeyEvent::ctrl('R'),
            EventHandler::Conditional(Box::new(PickHistory)),
        );
        new.bind_sequence(
            KeyEvent(KeyCode::Up, Modifiers::NONE),
            EventHandler::Conditional(Box::new(PrefixHistory(SearchDirection::Reverse))),
        );
        new.bind_sequence(
            KeyEvent(KeyCode::Down, Modifiers::NONE),
            EventHandler::Conditional(Box::new(PrefixHistory(SearchDirection::Forward))),
        );
        new.bind_sequence(
            KeyEvent::ctrl('L'),
            EventHandler::Conditional(Box::new(ClearScreen)),
        );
        new.bind_sequence(
            KeyEvent::new(' ', Modifiers::NONE),
            EventHandler::Conditional(Box::new(voice::HoldToTalk)),
        );
        *editor = Some(new);
    }
    BROWSING.store(false, Ordering::Relaxed);
    match editor.as_mut().map(|e| e.readline(prompt)) {
        Some(Ok(line)) => match voice::SAID.lock().unwrap_or_else(|e| e.into_inner()).take() {
            // Shown where it would have been typed, the line above.
            Some(said) => {
                let prompt = prompt.rsplit('\n').next().unwrap_or(prompt);
                println!("\x1b[1A\r\x1b[2K{prompt}{said}");
                Ok(Some(said))
            }
            None => Ok(Some(line)),
        },
        Some(Err(ReadlineError::Eof)) => Ok(None),
        Some(Err(ReadlineError::Interrupted)) if BUSY.load(Ordering::Relaxed) => {
            Err(Stopped.into())
        }
        Some(Err(ReadlineError::Interrupted)) => {
            stop_jobs();
            std::process::exit(130);
        }
        Some(Err(err)) => Err(err).context("reading stdin"),
        None => Ok(None),
    }
}

/// How `uc agent model` shows `choice`: its id, with the provider beside it.
pub(crate) fn label(_pool: &[Choice], choice: &Choice) -> String {
    format!("{} ({})", choice.model, providers::label(&choice.provider))
}

/// The model `uc agent model <name> [provider]` means: its number in `pool`, the one
/// listed there whose id has `name` in it, or `name` itself when it is an id
/// (it has a `/`, or a provider is named). A model more than one provider
/// serves is taken on the provider named, else on `prefer`, else on the
/// first listed.
pub(crate) fn choice_named(pool: &[Choice], name: &str, prefer: &str) -> Option<Choice> {
    // `opus copilot` and `opus (Copilot)`, as the list shows it, both name one.
    let mut words: Vec<&str> = name.split_whitespace().collect();
    let only: Option<String> = match words[..] {
        [_, .., last] => {
            let last = last.trim_matches(|c| c == '(' || c == ')');
            pool.iter()
                .map(|c| c.provider.as_str())
                .find(|p| p.eq_ignore_ascii_case(last) || providers::label(p) == last)
                .map(String::from)
        }
        _ => None,
    };
    if only.is_some() {
        words.pop();
    }
    let name = words.join(" ");
    if let Ok(n) = name.parse::<usize>() {
        return pool.get(n.checked_sub(1)?).cloned();
    }
    let lower = name.to_lowercase();
    let listed: Vec<&Choice> = pool
        .iter()
        .filter(|c| only.as_ref().is_none_or(|p| c.provider == *p))
        .collect();
    let found: Vec<&Choice> = listed
        .iter()
        .copied()
        .filter(|c| c.model.to_lowercase().contains(&lower))
        .collect();
    // One model on several providers is still one model.
    let best = |of: Vec<&Choice>| {
        of.iter()
            .find(|c| c.provider.eq_ignore_ascii_case(prefer))
            .or(of.first())
            .map(|c| (*c).clone())
    };
    let names: BTreeSet<String> = found.iter().map(|c| common_name(&c.model)).collect();
    if names.len() == 1 {
        return best(found);
    }
    // `opus` is the picked Opus, not every Opus OpenRouter has ever served.
    let picked: Vec<&Choice> = found.iter().copied().filter(|c| !c.extra).collect();
    if picked.len() < found.len()
        && picked
            .iter()
            .map(|c| common_name(&c.model))
            .collect::<BTreeSet<_>>()
            .len()
            == 1
    {
        return best(picked);
    }
    if name.contains('/') || (only.is_some() && found.is_empty() && !name.is_empty()) {
        return Some(Choice::new(only.as_deref().unwrap_or(prefer), &name));
    }
    // `glm` is GLM Flash and GLM 5.3: the exact name after the vendor wins.
    best(
        found
            .into_iter()
            .filter(|c| common_name(&c.model) == common_name(&lower))
            .collect(),
    )
}

/// The text of a message, be it a plain string or what OpenRouter sends as
/// parts.
fn content(message: &Value) -> Option<&str> {
    match &message["content"] {
        Value::String(text) => Some(text),
        Value::Array(parts) => parts
            .iter()
            .find_map(|p| p["text"].as_str())
            .or_else(|| parts.first().and_then(|p| p["content"].as_str())),
        _ => None,
    }
}

/// Where the model lists are kept between runs, so starting `uc` does not
/// wait on the providers: `$XDG_CACHE_HOME/usecode/models.json`, else
/// `~/.cache/usecode/models.json`. `uc agent model-update` fetches them again.
fn cache_path() -> Option<PathBuf> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(cache.join("usecode/models.json"))
}

/// What the cache keeps under `key`, if it is there.
fn cache_get(key: &str) -> Option<Value> {
    let text = std::fs::read_to_string(cache_path()?).ok()?;
    let mut cache: Value = serde_json::from_str(&text).ok()?;
    match cache[key].take() {
        Value::Null => None,
        value => Some(value),
    }
}

/// Keeps `value` in the cache under `key`, next to the rest. A cache that
/// can't be written is no loss: the next run fetches again.
fn cache_set(key: &str, value: Value) {
    let Some(path) = cache_path() else { return };
    let mut cache = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    cache[key] = value;
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, cache.to_string())?;
        std::fs::rename(&tmp, &path)
    };
    if let Err(err) = write() {
        eprintln!("· can't write {}: {err}", path.display());
    }
}

/// OpenRouter's model list as the session holds it.
static OPENROUTER_LIST: Mutex<Option<Arc<Vec<Value>>>> = Mutex::new(None);

/// The models' context window and prices, read off OpenRouter's list: from
/// the cache if it has one, else fetched and cached. `model_info` takes the
/// field of the `model` whose `id` or `canonical_slug` matches, if it is in
/// the list.
async fn models() -> Result<Arc<Vec<Value>>> {
    if let Some(list) = OPENROUTER_LIST.lock().unwrap().clone() {
        return Ok(list);
    }
    let cached = cache_get("openrouter").and_then(|v| match v {
        Value::Array(list) if !list.is_empty() => Some(list),
        _ => None,
    });
    match cached {
        Some(list) => {
            let list = Arc::new(list);
            *OPENROUTER_LIST.lock().unwrap() = Some(list.clone());
            Ok(list)
        }
        None => refresh_models().await,
    }
}

/// Fetches OpenRouter's list again, and keeps it in the session and the
/// cache. When the fetch fails, what was there stays.
async fn refresh_models() -> Result<Arc<Vec<Value>>> {
    let list: Value = client()
        .get("https://openrouter.ai/api/v1/models")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("reading OpenRouter's model list")?;
    // Only what the session reads of each, to keep the cache small.
    let models: Vec<Value> = list["data"]
        .as_array()
        .ok_or_else(|| anyhow!("OpenRouter's model list came without models"))?
        .iter()
        .map(|m| {
            json!({
                "id": m["id"],
                "canonical_slug": m["canonical_slug"],
                "context_length": m["context_length"],
                "pricing": {
                    "prompt": m["pricing"]["prompt"],
                    "completion": m["pricing"]["completion"],
                },
                "architecture": {"output_modalities": m["architecture"]["output_modalities"]},
            })
        })
        .collect();
    cache_set("openrouter", Value::Array(models.clone()));
    let models = Arc::new(models);
    *OPENROUTER_LIST.lock().unwrap() = Some(models.clone());
    Ok(models)
}

/// The id of an entry of OpenRouter's list when the model answers in text
/// (image and embedding models can't run a chat).
fn chat_model_id(entry: &Value) -> Option<&str> {
    let id = entry["id"].as_str()?;
    let text = match entry["architecture"]["output_modalities"].as_array() {
        Some(out) => out.iter().any(|m| m == "text"),
        None => true,
    };
    text.then_some(id)
}

/// The `model`'s context window and price per million tokens, off the list
/// OpenRouter serves; None when the model isn't there.
async fn model_info(model: &str) -> Result<Option<(Option<u64>, Option<(f64, f64)>)>> {
    let models = models().await?;
    let Some(entry) = models
        .iter()
        .find(|m| m["id"] == model || m["canonical_slug"] == model)
        // Copilot's ids have no vendor: `claude-opus-5.5` is Anthropic's.
        .or_else(|| {
            (!model.contains('/')).then(|| {
                models
                    .iter()
                    .find(|m| m["id"].as_str().map(common_name) == Some(common_name(model)))
            })?
        })
    else {
        return Ok(None);
    };
    let window = entry["context_length"].as_u64();
    let price = entry["pricing"]["prompt"]
        .as_str()
        .and_then(|p| p.parse::<f64>().ok())
        .zip(
            entry["pricing"]["completion"]
                .as_str()
                .and_then(|p| p.parse::<f64>().ok()),
        )
        .map(|(in_per, out_per)| (in_per * 1e6, out_per * 1e6));
    Ok(Some((window, price)))
}

/// An environment variable, if it is set to something.
fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The engine's results for `query`; DuckDuckGo's when it is the engine or
/// the other fails, read off its HTML page: no key needed, up to 10 results,
/// the first page.
async fn web_search(engine: &Engine, query: &str, count: u64) -> Result<String> {
    eprintln!("· search {query}");
    let searched = match engine {
        Engine::Parallel(key) => Some(("Parallel", parallel_search(query, count, key).await)),
        Engine::Exa(key) => Some(("Exa", exa_search(query, count, key).await)),
        Engine::DuckDuckGo => None,
    };
    let note = match searched {
        Some((_, Ok(results))) => return Ok(results),
        Some((name, Err(err))) => format!("[{name} failed, {err:#}; searched DuckDuckGo]\n\n"),
        None => String::new(),
    };
    let response = client()
        .get(DUCKDUCKGO)
        .header("User-Agent", "Mozilla/5.0")
        .query(&[("q", query)])
        .timeout(WEB_TIMEOUT)
        .send()
        .await
        .context("calling DuckDuckGo")?;
    let status = response.status();
    let page = response.text().await.context("reading DuckDuckGo's page")?;
    // A busy IP gets a challenge page instead of results, with a 202.
    if page.contains("anomaly-modal") {
        bail!("DuckDuckGo is limiting this IP for now; try again in a few minutes");
    }
    if status != reqwest::StatusCode::OK {
        bail!("DuckDuckGo: {status}");
    }
    Ok(note + &search_results(&page, count as usize))
}

/// Exa's results for `query`, each with its most relevant sentences.
async fn exa_search(query: &str, count: u64, key: &str) -> Result<String> {
    let response = client()
        .post(EXA)
        .header("x-api-key", key)
        .json(&json!({
            "query": query,
            "numResults": count,
            "contents": {"highlights": {"numSentences": 3, "highlightsPerUrl": 1}},
        }))
        .timeout(WEB_TIMEOUT)
        .send()
        .await
        .context("calling Exa")?;
    let status = response.status();
    let value: Value = response.json().await.context("reading Exa's reply")?;
    if !status.is_success() {
        let msg = value["error"].as_str().unwrap_or("no message");
        bail!("Exa: {status}: {msg}");
    }
    Ok(api_results(&value, "highlights"))
}

/// Parallel's results for `query`, each with excerpts picked for it.
async fn parallel_search(query: &str, count: u64, key: &str) -> Result<String> {
    let response = client()
        .post(PARALLEL)
        .header("x-api-key", key)
        .header("parallel-beta", "search-extract-2025-10-10")
        .json(&json!({
            "objective": query,
            "search_queries": [query],
            "max_results": count,
            "excerpts": {"max_chars_per_result": 1500},
        }))
        .timeout(WEB_TIMEOUT)
        .send()
        .await
        .context("calling Parallel")?;
    let status = response.status();
    let value: Value = response.json().await.context("reading Parallel's reply")?;
    if !status.is_success() {
        let msg = value["error"]["message"]
            .as_str()
            .or_else(|| value["error"].as_str())
            .unwrap_or("no message");
        bail!("Parallel: {status}: {msg}");
    }
    Ok(api_results(&value, "excerpts"))
}

/// Exa's or Parallel's results as text, the same shape as DuckDuckGo's: the
/// snippet is the strings in each result's `snippets` list.
fn api_results(value: &Value, snippets: &str) -> String {
    let results: Vec<String> = value["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|result| {
            let url = result["url"].as_str()?;
            let title = result["title"].as_str().unwrap_or(url);
            let snippet = result[snippets]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            let snippet = snippet.split_whitespace().collect::<Vec<_>>().join(" ");
            Some(format!("{title}\n{url}\n{snippet}"))
        })
        .collect();
    if results.is_empty() {
        return "no results".to_string();
    }
    results.join("\n\n")
}

/// The results on DuckDuckGo's HTML page as text, ads left out.
fn search_results(page: &str, count: usize) -> String {
    let results: Vec<String> = page
        .split(r#"class="result__a""#)
        .skip(1)
        .filter_map(|result| {
            let href = between(result, r#"href=""#, "\"")?;
            let url = result_url(href)?;
            let title = between(result, ">", "</a>")?;
            let snippet = between(result, r#"class="result__snippet""#, "</a>")
                .and_then(|s| s.split_once('>'))
                .map_or("", |(_, s)| s);
            Some(format!(
                "{}\n{url}\n{}",
                html_text(title),
                html_text(snippet)
            ))
        })
        .take(count)
        .collect();
    if results.is_empty() {
        return "no results".to_string();
    }
    results.join("\n\n")
}

/// The text between the first `start` and the `end` after it.
fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let (_, rest) = s.split_once(start)?;
    rest.split_once(end).map(|(inner, _)| inner)
}

/// Where a result's link goes: DuckDuckGo sends it through `/l/?uddg=`; ads
/// go through `y.js` and are left out.
fn result_url(href: &str) -> Option<String> {
    let href = href.replace("&amp;", "&");
    let url = reqwest::Url::parse(&href)
        .or_else(|_| reqwest::Url::parse(&format!("https:{href}")))
        .ok()?;
    if url.path().ends_with("y.js") {
        return None;
    }
    match url.query_pairs().find(|(k, _)| k == "uddg") {
        Some((_, target)) => Some(target.into_owned()),
        None => Some(url.to_string()),
    }
}

/// An HTML fragment as plain text: tags gone, entities decoded.
fn html_text(html: &str) -> String {
    html2text::config::plain_no_decorate()
        .string_from_read(html.as_bytes(), usize::MAX)
        .unwrap_or_else(|_| html.to_string())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The page as markdown from Cloudflare's headless browser when `fetch:
/// cloudflare:` in `agent.yaml` has an account and a token, else fetched and turned into
/// text here, without running its JavaScript.
async fn fetch_url(url: &str) -> Result<String> {
    eprintln!("· fetch {url}");
    if !url.starts_with("http://") && !url.starts_with("https://") {
        bail!("only http and https URLs can be fetched");
    }
    let mut note = String::new();
    let settings = providers::settings();
    if let (Some(account), Some(token)) = (
        settings.cloudflare_account.as_deref(),
        settings.cloudflare_token.as_deref(),
    ) {
        match render(url, account, token).await {
            Ok(markdown) => return Ok(markdown),
            Err(err) => note = format!("[not rendered, {err:#}; fetched without JavaScript]\n\n"),
        }
    }
    let response = client()
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (compatible; uc)")
        .timeout(WEB_TIMEOUT)
        .send()
        .await
        .context("fetching the page")?;
    let status = response.status();
    let html = response
        .headers()
        .get("content-type")
        .and_then(|t| t.to_str().ok())
        .is_none_or(|t| t.contains("html"));
    let body = response.bytes().await.context("reading the page")?;
    if !status.is_success() {
        note.push_str(&format!("[HTTP {status}]\n\n"));
    }
    let text = if html {
        html2text::from_read(&body[..], 100).context("reading the page's HTML")?
    } else {
        String::from_utf8_lossy(&body).into_owned()
    };
    Ok(note + &text)
}

/// Cloudflare Browser Rendering's markdown of the page.
async fn render(url: &str, account: &str, token: &str) -> Result<String> {
    let response = client()
        .post(format!(
            "https://api.cloudflare.com/client/v4/accounts/{account}/browser-rendering/markdown"
        ))
        .bearer_auth(token)
        .json(&json!({"url": url}))
        .timeout(WEB_TIMEOUT)
        .send()
        .await
        .context("calling Cloudflare")?;
    let status = response.status();
    let value: Value = response
        .json()
        .await
        .context("reading Cloudflare's reply")?;
    match value["result"].as_str() {
        Some(markdown) if value["success"] == true => Ok(markdown.to_string()),
        _ => {
            let msg = value["errors"][0]["message"]
                .as_str()
                .unwrap_or("no message");
            bail!("Cloudflare: {status}: {msg}")
        }
    }
}

/// The git repositories under `prefix` (`~/src` when empty), each relative to
/// it, sorted. A repository is not looked into, so its submodules and vendored
/// checkouts are not listed apart.
fn src(prefix: &str) -> Result<String> {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    let prefix = match prefix.trim() {
        "" => home().context("HOME is not set")?.join("src"),
        "~" => home().context("HOME is not set")?,
        p => match p.strip_prefix("~/") {
            Some(rest) => home().context("HOME is not set")?.join(rest),
            None => PathBuf::from(p),
        },
    };
    eprintln!("· src {}", prefix.display());
    let prefix = prefix
        .canonicalize()
        .with_context(|| format!("reading {}", prefix.display()))?;
    let mut repos = Vec::new();
    let mut dirs = vec![prefix.clone()];
    while let Some(dir) = dirs.pop() {
        if dir.join(".git").exists() {
            repos.push(match dir.strip_prefix(&prefix) {
                Ok(p) if p.as_os_str().is_empty() => ".".to_string(),
                Ok(p) => p.display().to_string(),
                Err(_) => dir.display().to_string(),
            });
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        // Symlinks are not followed, so a link back up can't loop.
        dirs.extend(
            entries
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.path()),
        );
    }
    repos.sort();
    if repos.is_empty() {
        return Ok(format!("no git repositories under {}", prefix.display()));
    }
    Ok(format!("under {}:\n{}", prefix.display(), repos.join("\n")))
}

fn truncate(mut output: String) -> String {
    if output.len() > MAX_OUTPUT {
        let mut end = MAX_OUTPUT;
        while !output.is_char_boundary(end) {
            end -= 1;
        }
        output.truncate(end);
        output.push_str("\n[output cut]");
    }
    output
}

// ------------------------------------------------------- uc agent model ----

/// Saves `choice` as `provider:` and `model:` in `agent.yaml`, the model new
/// `uc` sessions start on, and returns the file's path. `UC_PROVIDER` and
/// `UC_MODEL` still win over it where they are set.
fn save_model_choice(choice: &Choice) -> Result<PathBuf> {
    let path = providers::save_choice(&choice.provider, &choice.model)?;
    if env("UC_MODEL").is_some() || env("UC_PROVIDER").is_some() {
        eprintln!(
            "· UC_MODEL or UC_PROVIDER is set in this environment and wins over it; unset them"
        );
    }
    Ok(path)
}

/// `uc agent model` in a shell, when a session is not there to take it:
/// with no model named it lists the models of every provider with a key,
/// marking the one a new `uc` starts on; with one named (`uc agent model
/// opus claude`) it makes that the one new sessions start on, saved as
/// `provider:` and `model:` in `agent.yaml`. Typed at the
/// prompt of a running session the same words switch that session too; the
/// other sessions running keep theirs.
pub(crate) async fn agent_model(args: &[OsString]) -> Result<()> {
    let args: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("usage: uc agent model [<number|name|id> [provider]]\n");
        println!("With no model named: the models of every provider with a key, the");
        println!("current one marked, as typing `uc agent model` in the chat lists them.");
        println!("With one named: makes it the model new `uc` sessions start on, saved");
        println!(
            "as provider: and model: in {}. The name is a number from the list, part of",
            providers::path().display()
        );
        println!("a listed id or any id of the provider. A session already running keeps");
        println!("its model: type `uc agent model` at that session's prompt to switch it.");
        return Ok(());
    }
    let providers = providers::usable();
    let Some(start) = start_provider(&providers) else {
        bail!("{}", no_provider());
    };
    let pool = pool(&providers, false).await;
    let current = Choice::new(&start.name, &start_model(start, &pool));
    let name = args.join(" ");
    if name.is_empty() {
        for (i, choice) in pool.iter().enumerate() {
            let mark = if *choice == current { "*" } else { " " };
            println!("{mark} {} {}", i + 1, label(&pool, choice));
        }
        return Ok(());
    }
    let choice = choice_named(&pool, &name, &start.name)
        .ok_or_else(|| anyhow!("no model {name}; `uc agent model` lists them"))?;
    if named(&providers, &choice.provider).is_none() {
        bail!(
            "{} has no key for {} in {}, so it is off",
            choice.provider,
            providers::label(&choice.provider),
            providers::path().display()
        );
    }
    let path = save_model_choice(&choice)?;
    println!(
        "· {} on {}, saved in {} for new `uc` sessions",
        choice.model,
        providers::label(&choice.provider),
        path.display()
    );
    Ok(())
}

/// `uc agent model-update` in a shell: fetches every provider's model list
/// again and keeps it in the cache, what `uc agent model-update` typed at a
/// session's prompt does for that session.
pub(crate) async fn agent_model_update() -> Result<()> {
    let providers = providers::usable();
    if providers.is_empty() {
        bail!("{}", no_provider());
    }
    eprintln!("· fetching the model lists");
    let pool = pool(&providers, true).await;
    eprintln!("· {} models", pool.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn src_lists_repositories_but_not_inside_them() {
        let root = std::env::temp_dir().join(format!("uc-src-{}", std::process::id()));
        for dir in [
            "a/one/.git",
            "a/one/vendor/two/.git",
            "b/three/.git",
            "c/empty",
        ] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let listed = src(root.to_str().unwrap()).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        let lines: Vec<&str> = listed.lines().skip(1).collect();
        assert_eq!(lines, ["a/one", "b/three"]);
    }

    #[test]
    fn reads_a_file_in_numbered_pieces() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f.txt");
        std::fs::write(&file, "a\nb\nc\nd\n").unwrap();
        let path = file.to_str().unwrap();
        assert_eq!(
            read_file(path, None, None).unwrap(),
            "     1\ta\n     2\tb\n     3\tc\n     4\td\n"
        );
        let piece = read_file(path, Some(2), Some(2)).unwrap();
        assert!(piece.starts_with("     2\tb\n     3\tc\n"));
        assert!(piece.contains("[1 more lines; read from offset 4]"));
    }

    #[test]
    fn greps_and_globs_skip_what_gitignore_ignores() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::write(root.join(".gitignore"), "skip.rs\n").unwrap();
        std::fs::write(root.join("a.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.join("skip.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.join("b.md"), "fn main\n").unwrap();
        let path = root.to_str().unwrap();
        let hits = grep("fn main", path, "*.rs", None).unwrap();
        assert!(hits.contains("a.rs:1:") && !hits.contains("skip.rs") && !hits.contains("b.md"));
        assert_eq!(grep("zzz", path, "", None).unwrap(), "no matches\n");
        let files = glob("*.md", path).unwrap();
        assert!(files.contains("b.md") && !files.contains("a.rs"));
    }

    #[test]
    fn prune_cuts_old_tool_output_only() {
        let long = "x".repeat(PRUNED * 3);
        let mut messages: Vec<Value> = vec![json!({"role": "tool", "content": long.clone()})];
        messages.extend((0..KEEP_RECENT).map(|_| json!({"role": "tool", "content": long.clone()})));
        prune_tool_output(&mut messages);
        assert!(messages[0]["content"].as_str().unwrap().len() < PRUNED + 60);
        assert_eq!(messages[1]["content"].as_str().unwrap().len(), long.len());
    }

    #[test]
    fn transcript_shows_text_and_tool_calls() {
        let messages = vec![
            json!({"role": "user", "content": "build it"}),
            json!({"role": "assistant", "content": null, "tool_calls": [
                {"function": {"name": "bash", "arguments": "{\"command\":\"make\"}"}}]}),
        ];
        let text = transcript(&messages);
        assert!(text.contains("[user] build it") && text.contains("[tool call] bash"));
    }

    #[test]
    fn a_streamed_reply_becomes_a_message() {
        let mut reply = Reply::default();
        let chunks = [
            json!({"choices": [{"delta": {"content": "Hel"}}]}),
            json!({"choices": [{"delta": {"content": "lo", "reasoning_details": [
                {"index": 0, "type": "reasoning.text", "text": "th"}]}}]}),
            json!({"choices": [{"delta": {"reasoning_details": [
                {"index": 0, "type": "reasoning.text", "text": "ink", "signature": "s"}],
                "tool_calls": [{"index": 0, "id": "c1", "function": {"name": "bash", "arguments": "{\"co"}}]}}]}),
            json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "function": {"arguments": "mmand\":\"ls\"}"}}]}}]}),
            json!({"choices": [], "usage": {"prompt_tokens": 7}}),
        ];
        let text: String = chunks.iter().map(|c| reply.add(c).unwrap()).collect();
        assert_eq!(text, "Hello");
        assert_eq!(reply.usage.as_ref().unwrap()["prompt_tokens"], 7);
        let message = reply.message();
        assert_eq!(message["content"], "Hello");
        assert_eq!(message["tool_calls"][0]["id"], "c1");
        assert_eq!(
            message["tool_calls"][0]["function"]["arguments"],
            "{\"command\":\"ls\"}"
        );
        assert_eq!(message["reasoning_details"][0]["text"], "think");
        assert_eq!(message["reasoning_details"][0]["signature"], "s");
        assert!(
            Reply::default()
                .add(&json!({"error": {"message": "no"}}))
                .is_err()
        );
    }

    #[test]
    fn cache_breakpoints_go_on_the_system_prompt_and_last_message() {
        let messages = vec![
            json!({"role": "system", "content": "s"}),
            json!({"role": "user", "content": "u"}),
            json!({"role": "user", "content": "last"}),
        ];
        let marked = cached("anthropic/claude-haiku-5.5", &messages);
        assert_eq!(
            marked[0]["content"][0]["cache_control"]["type"],
            "ephemeral"
        );
        assert_eq!(marked[1]["content"], "u");
        assert_eq!(marked[2]["content"][0]["text"], "last");
        assert_eq!(cached("z-ai/glm-5.3", &messages), messages);
    }

    #[test]
    fn truncates_on_a_char_boundary() {
        let long = "é".repeat(MAX_OUTPUT);
        let cut = truncate(long);
        assert!(cut.ends_with("[output cut]"));
        assert!(cut.len() <= MAX_OUTPUT + "\n[output cut]".len());
        assert_eq!(truncate("short".into()), "short");
    }

    #[test]
    fn lists_search_results_as_text() {
        let page = r#"<div class="result">
            <a rel="nofollow" class="result__a" href="//duckduckgo.com/y.js?ad_domain=x.com&amp;u3=1">Ad</a>
            <a class="result__snippet" href="//duckduckgo.com/y.js">Buy</a>
          </div><div class="result">
            <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust%2Dlang.org%2F&amp;rut=e4">Rust</a>
            <a class="result__url" href="//duckduckgo.com/l/?uddg=x">rust-lang.org</a>
            <a class="result__snippet" href="//duckduckgo.com/l/?uddg=x">A language empowering <b>everyone</b> &amp; more</a>
          </div><div class="result">
            <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fdoc.rust-lang.org%2Fbook%2F">The Book</a>
          </div>"#;
        assert_eq!(
            search_results(page, 10),
            "Rust\nhttps://www.rust-lang.org/\nA language empowering everyone & more\n\n\
             The Book\nhttps://doc.rust-lang.org/book/\n"
        );
        assert_eq!(
            search_results(page, 1),
            "Rust\nhttps://www.rust-lang.org/\nA language empowering everyone & more"
        );
        assert_eq!(search_results("<html></html>", 10), "no results");
    }

    #[test]
    fn lists_api_results_as_text() {
        let reply = json!({"results": [
            {"title": "Rust", "url": "https://rust-lang.org/", "highlights": ["A language\n empowering everyone."]},
            {"url": "https://example.com/"},
        ]});
        assert_eq!(
            api_results(&reply, "highlights"),
            "Rust\nhttps://rust-lang.org/\nA language empowering everyone.\n\n\
             https://example.com/\nhttps://example.com/\n"
        );
        assert_eq!(
            api_results(&json!({"results": []}), "excerpts"),
            "no results"
        );
    }

    #[test]
    fn reads_the_bash_history() {
        assert_eq!(
            timed_lines("ls\n#1700000000\ngit status\n\n# a note\npwd\n"),
            [
                (0, "ls".into()),
                (1700000000, "git status".into()),
                (1700000000, "# a note".into()),
                (1700000000, "pwd".into()),
            ]
        );
    }

    #[test]
    fn merges_bash_and_uc_by_time() {
        let bash = "old\n#100\nls\n#300\ngit push\n";
        let uc = "#200\nfix the tests\n#300\nmake it faster\n";
        assert_eq!(
            merged_lines(bash, uc),
            ["old", "ls", "fix the tests", "git push", "make it faster"]
        );
    }

    #[test]
    fn reads_the_history_file() {
        assert_eq!(history_lines("#V2\nls\n\na\\nb\\\\c\n"), ["ls", "a\nb\\c"]);
        assert_eq!(history_lines("ls\npwd\n"), ["ls", "pwd"]);
    }

    #[test]
    fn a_trailing_y_approves() {
        assert_eq!(split_approval("ls -y"), ("ls", true));
        assert_eq!(split_approval("-y"), ("", true));
        assert_eq!(split_approval("ls"), ("ls", false));
        assert_eq!(split_approval("apt-y"), ("apt-y", false));
    }

    #[test]
    fn a_command_line_runs_and_a_sentence_goes_to_the_model() {
        for line in [
            "ls -al",
            "git status",
            "cd ..",
            "git commit -m 'fix the bug'",
            "make all",
            "!yes | head -1",
        ] {
            assert!(typed_command(line).is_some(), "{line}");
        }
        assert_eq!(typed_command("! ls"), Some("ls"));
        for line in [
            "yes",
            "go ahead",
            "find the bug in main.rs",
            "make it faster",
            "ls?",
            "fix the tests",
            "why is it slow",
            "!",
        ] {
            assert!(typed_command(line).is_none(), "{line}");
        }
    }

    #[test]
    fn aliases_and_functions_come_from_the_users_bash() {
        let dump = "bashrc says hi\0shopt -s extglob\n\0alias ..='cd ..'\n\
                    alias say='echo '\\''hi'\\'''\n\0complete -F _git git\n\0declare -f mkcd\n\0mkcd () { :; }\n\0/home/me/.history";
        let shell = parse_user_shell(dump).unwrap();
        assert!(shell.knows("..") && shell.knows("mkcd") && !shell.knows("hi"));
        assert_eq!(shell.aliases["say"], "echo 'hi'");
        assert_eq!(shell.expand(".."), "cd ..");
        assert_eq!(shell.expand("say there"), "echo 'hi' there");
        assert_eq!(shell.expand("ls -l"), "ls -l");
        assert!(!shell.script("..").contains("bashrc says hi"));
        assert!(shell.script("..").ends_with("mkcd () { :; }\n\n.."));
        assert_eq!(shell.history, Some(PathBuf::from("/home/me/.history")));
        assert!(parse_user_shell("no definitions").is_none());
    }

    #[test]
    fn tab_completes_like_bash() {
        let (start, words) = Completion::candidates("!ech");
        assert_eq!(start, 1);
        assert!(words.iter().any(|w| w.trim_end() == "echo"), "{words:?}");
        let (start, words) = Completion::candidates("ls Carg");
        assert_eq!((start, words), (3, vec!["Cargo.toml ".to_string()]));
        let (start, words) = Completion::candidates("look at sr");
        assert_eq!((start, words), (8, vec!["src/".to_string()]));
    }

    #[test]
    fn cd_goes_where_bash_would() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(cd_target("cd"), Some(home.clone()));
        assert_eq!(cd_target("cd ~/src"), Some(home.join("src")));
        assert_eq!(cd_target("cd lib"), Some(PathBuf::from("lib")));
        assert_eq!(cd_target("cd lib && make"), None);
        assert_eq!(cd_target("cd $HOME"), None);
        assert_eq!(cd_target("ls"), None);
    }

    fn openrouter_pool() -> Vec<Choice> {
        MODELS
            .iter()
            .map(|m| Choice::new("openrouter", m))
            .collect()
    }

    /// OpenRouter's pool, the Claude subscription's and two of Copilot's.
    fn every_pool() -> Vec<Choice> {
        let mut pool = openrouter_pool();
        pool.extend(CLAUDE_MODELS.iter().map(|m| Choice::new("claude", m)));
        pool.push(Choice::new("copilot", "gpt-4.1"));
        pool.push(Choice::new("copilot", "claude-opus-5-5"));
        pool
    }

    #[test]
    fn model_named_takes_a_number_part_of_an_id_or_an_id() {
        let pool = openrouter_pool();
        let named = |name: &str| choice_named(&pool, name, "openrouter").map(|c| c.model);
        assert_eq!(named("2").as_deref(), Some("anthropic/claude-haiku-5.5"));
        assert_eq!(named("0"), None);
        assert_eq!(named("9"), None);
        assert_eq!(
            named("haiku").as_deref(),
            Some("anthropic/claude-haiku-5.5")
        );
        assert_eq!(named("glm-5.3").as_deref(), Some("z-ai/glm-5.3"));
        assert_eq!(named("glm"), None);
        assert_eq!(named("opus").as_deref(), Some("anthropic/claude-opus-5.5"));
        assert_eq!(named("luna").as_deref(), Some("openai/gpt-6-luna"));
        assert_eq!(named("openai/gpt-5").as_deref(), Some("openai/gpt-5"));
        assert_eq!(named("nope"), None);
    }

    #[test]
    fn a_model_on_several_providers_is_taken_on_the_one_named_or_preferred() {
        let pool = every_pool();
        let named = |name: &str, prefer| choice_named(&pool, name, prefer);
        let opus = |provider, model: &str| Some(Choice::new(provider, model));
        assert_eq!(
            named("opus", "openrouter"),
            opus("openrouter", "anthropic/claude-opus-5.5")
        );
        assert_eq!(
            named("opus", "claude"),
            opus("claude", "anthropic/claude-opus-5.5")
        );
        assert_eq!(
            named("opus copilot", "claude"),
            opus("copilot", "claude-opus-5-5")
        );
        assert_eq!(
            named("anthropic/claude-opus-5.5 (Claude)", "openrouter"),
            opus("claude", "anthropic/claude-opus-5.5")
        );
        assert_eq!(named("gpt-4.1", "openrouter"), opus("copilot", "gpt-4.1"));
        assert_eq!(named("o3 copilot", "openrouter"), opus("copilot", "o3"));
        assert_eq!(
            named("8", "openrouter"),
            opus("claude", DEFAULT_CLAUDE_MODEL)
        );
    }

    #[test]
    fn the_provider_is_shown_beside_every_model() {
        let pool = every_pool();
        assert_eq!(label(&pool, &pool[0]), "z-ai/glm-5.3-flash (OpenRouter)");
        assert_eq!(
            label(&pool, &pool[5]),
            "anthropic/claude-opus-5.5 (OpenRouter)"
        );
        assert_eq!(label(&pool, &pool[8]), "anthropic/claude-opus-5.5 (Claude)");
        assert_eq!(label(&pool, &pool[9]), "gpt-4.1 (Copilot)");
        assert_eq!(label(&pool, &pool[10]), "claude-opus-5-5 (Copilot)");
        assert_eq!(
            label(&openrouter_pool(), &pool[5]),
            "anthropic/claude-opus-5.5 (OpenRouter)"
        );
        assert_eq!(common_name("claude-opus-5-5"), "claude-opus-5.5");
        assert_eq!(common_name("openai/gpt-6-luna"), "gpt-6-luna");
    }

    #[test]
    fn picks_an_option_by_number() {
        let options = ["push it".to_string(), "commit first".to_string()];
        assert_eq!(pick(" 2 ", &options), Pick::Option("commit first".into()));
        assert_eq!(pick("3", &options), Pick::Own);
        assert_eq!(pick("4", &options), Pick::Typed);
        assert_eq!(pick("just push", &options), Pick::Typed);
        assert_eq!(pick("1", &[]), Pick::Typed);
    }

    #[test]
    fn deleting_always_counts() {
        for command in [
            "rm -rf target",
            "ls && sudo rm x",
            "find . -name '*.o' -delete",
            "bash -c 'rm x'",
            "/bin/rm x",
            "\\rm x",
            "echo $(rmdir d)",
            "xargs unlink",
        ] {
            assert!(deletes(command), "{command}");
        }
        for command in [
            "ls -l",
            "cargo fmt",
            "echo firmware",
            "npm run rmx",
            "rsync a b",
        ] {
            assert!(!deletes(command), "{command}");
        }
    }

    #[test]
    fn output_keeps_the_end() {
        let mut output = Output::default();
        output.push(b"hello ");
        let (text, seen) = output.since(0);
        assert_eq!((text.as_str(), seen), ("hello ", 6));
        output.push(&vec![b'x'; MAX_KEPT + 1]);
        let (text, seen) = output.since(seen);
        assert!(text.starts_with("[earlier output dropped]"));
        assert_eq!(seen, 7 + MAX_KEPT);
        assert_eq!(output.since(seen).0, "");
    }

    fn provider(name: &str) -> Provider {
        let kind = match name {
            "openrouter" => Kind::OpenRouter,
            "claude" => Kind::Claude,
            "copilot" => Kind::Copilot,
            _ => Kind::OpenAi,
        };
        Provider::new(name, kind)
    }

    fn session() -> Session {
        Session {
            providers: vec![provider("openrouter")],
            provider: "openrouter".into(),
            model: String::new(),
            pool: openrouter_pool(),
            ask: false,
            effort: String::new(),
            messages: Vec::new(),
            allow_all: true,
            allow_turn: false,
            show_output: false,
            show_commands: false,
            jobs: BTreeMap::new(),
            next_job: 1,
            search: Engine::DuckDuckGo,
            waiting: None,
            last_usage: None,
            spent: 0.0,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn esc_stops_the_command_and_answers_the_calls_left() {
        let mut session = session();
        let id = session.start("sleep 30", false).unwrap();
        let group = session.jobs[&id].group;
        session.waiting = Some(id);
        session
            .messages
            .push(json!({"role": "assistant", "tool_calls": [
                {"id": "a", "function": {"name": "bash"}},
                {"id": "b", "function": {"name": "bash"}},
            ]}));
        session
            .messages
            .push(json!({"role": "tool", "tool_call_id": "a", "content": "ok"}));
        session.stopped();
        assert!(session.jobs.is_empty());
        let answers: Vec<_> = session
            .messages
            .iter()
            .filter(|m| m["role"] == "tool")
            .collect();
        assert_eq!(answers.len(), 2);
        assert_eq!(answers[1]["tool_call_id"], "b");
        tokio::time::sleep(Duration::from_millis(200)).await;
        // SAFETY: signal 0 only checks whether the group still exists.
        assert_ne!(unsafe { libc::kill(-group, 0) }, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_slow_command_carries_on_as_a_job() {
        let mut s = session();
        let first = s
            .bash("echo a; sleep 2; echo b", "", 1, false, false)
            .await
            .unwrap();
        assert!(first.starts_with("still running as job 1"), "{first}");
        assert!(first.ends_with("a\n"), "{first}");
        let last = s.job_output(1, 5).await.unwrap();
        assert!(last.starts_with("exit status: 0"), "{last}");
        assert!(last.ends_with("output:\nb\n"), "{last}");
        assert!(s.jobs.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn job_kill_stops_the_whole_pipeline() {
        let mut s = session();
        s.bash("sleep 100 | cat", "", 0, true, false).await.unwrap();
        let group = s.jobs[&1].group;
        let killed = s.job_kill(1).await.unwrap();
        assert!(killed.contains("signal"), "{killed}");
        // SAFETY: signal 0 only checks whether the group still exists.
        assert_eq!(unsafe { libc::kill(-group, 0) }, -1);
        assert!(!GROUPS.lock().unwrap().contains(&group));
    }

    #[test]
    fn only_ssh_add_gets_the_terminal() {
        assert!(on_terminal("ssh-add ~/.ssh/id_evgnomon"));
        assert!(on_terminal("/usr/bin/ssh-add"));
        assert!(!on_terminal("git push"));
        assert!(!on_terminal("echo ssh-add"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn commands_have_no_terminal_to_ask_on() {
        let mut s = session();
        let out = s
            .bash("exec 3</dev/tty", "", 5, false, false)
            .await
            .unwrap();
        assert!(!out.starts_with("exit status: 0"), "{out}");
    }

    #[test]
    fn every_tool_has_a_name_and_parameters() {
        for tool in tools().as_array().unwrap() {
            assert!(tool["function"]["name"].is_string());
            assert_eq!(tool["function"]["parameters"]["type"], "object");
        }
    }

    #[test]
    fn chat_is_a_tool() {
        let tools = tools();
        let tool = tools
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["function"]["name"] == "chat")
            .unwrap();
        assert_eq!(tool["function"]["parameters"]["required"][0], "action");
        assert!(tool["function"]["parameters"]["properties"]["model"].is_object());
    }

    #[test]
    fn chat_model_lists_switches_and_fails() {
        let mut s = session();
        s.model = "z-ai/glm-5.3-flash".into();
        s.messages = vec![json!({"role": "system", "content": "you are uc"})];
        let listed = s.switch("");
        assert!(listed.contains("* 1 z-ai/glm-5.3-flash"), "{listed}");
        let switched = s.switch("haiku");
        assert_eq!(
            switched,
            "· model anthropic/claude-haiku-5.5 (OpenRouter)\n"
        );
        assert_eq!(s.model, "anthropic/claude-haiku-5.5");
        let named = s.switch("2");
        assert_eq!(named, "· model anthropic/claude-haiku-5.5 (OpenRouter)\n");
        let missing = s.switch("nope");
        assert!(missing.starts_with("· no model nope"), "{missing}");
    }

    #[test]
    fn uc_agent_commands_are_taken_by_the_chat() {
        assert_eq!(agent_command("uc agent compact"), Some("compact"));
        assert_eq!(agent_command("uc agent context"), Some("context"));
        assert_eq!(agent_command("uc agent exit"), Some("exit"));
        assert_eq!(agent_command("uc agent quit"), Some("quit"));
        assert_eq!(agent_command("uc agent usage"), None);
        assert_eq!(agent_command("uc agent contexts"), None);
        assert_eq!(agent_command("!uc agent exit"), None);
        assert_eq!(agent_command("/context"), None);
    }

    #[test]
    fn uc_agent_model_lines_are_taken_by_the_chat() {
        assert_eq!(agent_model_line("uc agent model"), Some(""));
        assert_eq!(
            agent_model_line("uc agent model opus claude"),
            Some(" opus claude")
        );
        assert_eq!(agent_model_line("uc agent model-update"), Some(""));
        assert_eq!(agent_model_line("uc agent modelxyz"), None);
        assert_eq!(agent_model_line("uc agent models"), None);
        assert_eq!(agent_model_line("uc agent login claude"), None);
        assert_eq!(agent_model_line("!uc agent model opus"), None);
        assert_eq!(agent_model_line("please uc agent model"), None);
    }

    #[test]
    fn chat_model_switches_between_providers() {
        let mut s = session();
        s.providers.push(provider("claude"));
        s.providers.push(provider("copilot"));
        s.pool = every_pool();
        s.model = "z-ai/glm-5.3-flash".into();
        s.messages = vec![json!({"role": "system", "content": "you are uc"})];
        let listed = s.switch("");
        assert!(
            listed.contains("anthropic/claude-opus-5.5 (Claude)"),
            "{listed}"
        );
        assert!(listed.contains(" gpt-4.1 (Copilot)\n"), "{listed}");
        let switched = s.switch("opus claude");
        assert_eq!(switched, "· model anthropic/claude-opus-5.5 (Claude)\n");
        assert_eq!(s.provider, "claude");
        assert!(
            s.messages[0]["content"]
                .as_str()
                .unwrap()
                .contains("claude-opus-5.5")
        );
        // Opus again stays on the provider the session is on.
        s.switch("opus");
        assert_eq!(s.provider, "claude");
        s.switch("gpt-4.1");
        assert_eq!(
            (s.provider.as_str(), s.model.as_str()),
            ("copilot", "gpt-4.1")
        );
        s.providers.retain(|p| p.name != "claude");
        let off = s.switch("opus claude");
        assert!(off.contains("claude has no key for Claude"), "{off}");
        assert_eq!(s.provider, "copilot");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn chat_clears_the_context_and_keeps_a_turn() {
        let mut s = session();
        s.messages = vec![
            json!({"role": "system", "content": "you are uc"}),
            json!({"role": "user", "content": "hello"}),
        ];
        let cleared = s.clear();
        assert_eq!(cleared, "context cleared");
        assert!(CLEARED.swap(false, Ordering::Relaxed), "the clear is said");
        // The prompt loop drops what was said, once the turn has ended.
        s.messages.retain(|m| m["role"] == "system");
        assert_eq!(s.messages.len(), 1);
        assert_eq!(s.messages[0]["role"], "system");

        // A clear inside a turn waits for the turn to end: the tool messages
        // keep their assistant with its calls, so the next request is well
        // formed, and CLEARED says to drop the rest.
        let mut s = session();
        s.messages = vec![
            json!({"role": "system", "content": "you are uc"}),
            json!({"role": "user", "content": "clear the chat"}),
            json!({"role": "assistant", "content": "", "tool_calls": [
                {"id": "a", "function": {"name": "chat"}},
            ]}),
        ];
        let cleared = s.clear();
        assert_eq!(cleared, "context cleared");
        assert!(CLEARED.swap(false, Ordering::Relaxed), "the clear waits");
        assert_eq!(s.messages.len(), 3, "the turn's messages stay");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn chat_is_dispatched_like_a_tool_call() {
        let mut s = session();
        s.providers.push(provider("claude"));
        s.pool = every_pool();
        s.model = "z-ai/glm-5.3-flash".into();
        s.messages = vec![json!({"role": "system", "content": "you are uc"})];
        let model = json!({"id": "a", "function": {"name": "chat", "arguments":
            r#"{"action": "model", "model": "haiku"}"#}});
        let out = s.call(&model).await;
        assert_eq!(out, "· model anthropic/claude-haiku-5.5 (OpenRouter)\n");
        assert_eq!(s.model, "anthropic/claude-haiku-5.5");
        let on = json!({"id": "p", "function": {"name": "chat", "arguments":
            r#"{"action": "model", "model": "haiku", "provider": "Claude"}"#}});
        let out = s.call(&on).await;
        assert_eq!(out, "· model anthropic/claude-haiku-5.5 (Claude)\n");
        assert_eq!(s.provider, "claude");
        let listed = json!({"id": "b", "function": {"name": "chat", "arguments":
            r#"{"action": "model"}"#}});
        let out = s.call(&listed).await;
        assert!(
            out.contains("* 7 anthropic/claude-haiku-5.5 (Claude)"),
            "{out}"
        );
        let unknown = json!({"id": "c", "function": {"name": "chat", "arguments":
            r#"{"action": "nope"}"#}});
        let out = s.call(&unknown).await;
        assert!(out.starts_with("error: "), "{out}");
        assert!(out.contains("no chat action"), "{out}");
    }

    #[test]
    fn watch_escape_leaves_the_saved_lock_free_while_it_waits() {
        // Not a terminal, like under `cargo test` from a pipe: no keys to
        // watch for, and the watcher gives up at once.
        if !std::io::stdin().is_terminal() {
            eprintln!("· no terminal: watch_escape never runs");
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let ready = Arc::new(AtomicBool::new(false));
        let watcher = {
            let stop = stop.clone();
            let ready = ready.clone();
            std::thread::spawn(move || {
                ready.store(true, Ordering::Relaxed);
                assert!(!watch_escape(&stop));
            })
        };
        while !ready.load(Ordering::Relaxed) {}
        // A hold of `SAVED` (Ctrl-C, `chat exit`) must get the lock while the
        // watcher waits, not after its `poll` or `read`; a watchdog fails the
        // test rather than let it hang on the lock held across them.
        let (disarm, hung) = std::sync::mpsc::channel::<()>();
        let watchdog = std::thread::spawn(move || {
            use std::sync::mpsc::RecvTimeoutError;
            // A timeout is the watcher holding the lock; a disconnect is the
            // test done before the 30s.
            if matches!(
                hung.recv_timeout(Duration::from_secs(30)),
                Err(RecvTimeoutError::Timeout)
            ) {
                eprintln!("· watch_escape held the lock while it waited");
                std::process::abort();
            }
        });
        for _ in 0..40 {
            restore_terminal(&mut saved());
            std::thread::sleep(Duration::from_millis(50));
        }
        stop.store(true, Ordering::Relaxed);
        watcher.join().expect("the watcher stopped");
        drop(disarm);
        let _ = watchdog.join();
    }
}
