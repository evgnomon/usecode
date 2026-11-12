// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Subagents, the `task` tool: work handed to a model of its own, faster and
//! cheaper than the session's, in a conversation of its own. It gives back
//! only its answer, so what it read and ran never fills the session's
//! context or is sent again with every request after. There are four kinds:
//!
//! - `explore` reads the files on this machine; `research` the web. They
//!   only look, so several run side by side.
//! - `run` runs commands in the session's own shell, as the session's own
//!   tool calls do: shown, confirmed in ask mode, stopped with Esc. One at
//!   a time.
//! - `edit` changes files in the session's own tree, in place, so the build
//!   caches and the uncommitted work are there, with commands in a shell of
//!   its own (in auto mode only). Nothing is committed: the session looks at
//!   the change with `git diff`. Several run side by side, on other files.
//!
//! The model is picked the way `uc agent model` picks one: a tier (`fast`,
//! `balanced`, `strong`), `same` as the session, or a name. With no model in
//! the call: `--subagent-model-<kind>` (`UC_SUBAGENT_MODEL_EXPLORE`), else the
//! session's (`--subagent-model`, `subagent_model:` in `agent.yaml`, or `uc
//! agent model --sub`), else the kind's own tier. A tier is taken on the
//! session's provider first, so a Claude subscription runs its subagents on
//! Haiku rather than on another bill, then on the other providers with a
//! key, else the session's own model. `--subagent-effort`
//! (`UC_SUBAGENT_EFFORT`) is how hard it thinks, `low` by default. A child
//! that only looks and fails or runs out of requests on a tier is tried once
//! more on the next one up.
//!
//! A child talks to its provider in that provider's API, as the session does
//! ([`stream_on`]): its messages, with their thinking blocks and tool-call
//! ids, never leave it, and the session gets plain text back. It keeps one
//! model to the end, so its cache stays warm: each request reads all but
//! its newest messages from it. Every child of a kind on a model starts
//! with the same system prompt and tools, which the first of them writes
//! to the cache before the others start; its first request marks only that
//! part, since its question is read once.

use super::*;
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::LazyLock;
use tokio::sync::Semaphore;

/// The longest answer a child gives back, in bytes.
const ANSWER: usize = 6_000;
/// How many children talk to one provider at once, so a batch of them stays
/// under its rate limit.
const AT_ONCE: usize = 3;
/// What a child is told when it has one request left.
const LAST: &str = "[That was your last tool call: answer now with what you found and did, \
                    without calling more tools.]";
/// How long Claude on OpenRouter keeps a prompt cached without `--long-cache`.
const SHORT_CACHE: Duration = Duration::from_secs(300);
/// The longest a command of an `edit` child runs, in seconds.
const MAX_TIMEOUT: u64 = 600;

/// Where a kind's tools act.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Works {
    /// Only looking, the session's own tools.
    Looks,
    /// The session's shell and jobs.
    Shell,
    /// The session's tree, in place, with a shell of its own.
    Tree,
}

/// A kind of subagent: its name, what it is told, its tools, the tier it
/// runs on unless told otherwise, and how many requests it may make.
struct Spec {
    name: &'static str,
    prompt: &'static str,
    tools: &'static [&'static str],
    tier: &'static str,
    rounds: usize,
    works: Works,
}

/// What every child is told about its answer.
macro_rules! answer {
    () => {
        "Your last reply goes back to the agent as it is, and nothing else you saw does. So \
         answer directly: exact paths, line numbers, the names of functions and types, and \
         short snippets only where they help. Say what you could not do or are not sure of. \
         No preamble, under about 600 words."
    };
}

const KINDS: &[Spec] = &[
    Spec {
        name: "explore",
        prompt: concat!(
            "You are a subagent of uc, the usecode coding agent. It hands you one question \
             about the code and files on this machine, and you find the answer with the \
             tools, which only read.\n\n\
             - Ask for several things in one reply: grep, glob and read_file calls together.\n\
             - Find before you read: grep or glob for the names, then read_file only the \
             parts that matter, with offset and limit.\n\
             - Stop as soon as you can answer.\n\n",
            answer!()
        ),
        tools: &["src", "read_file", "grep", "glob"],
        tier: "fast",
        rounds: 15,
        works: Works::Looks,
    },
    Spec {
        name: "research",
        prompt: concat!(
            "You are a subagent of uc, the usecode coding agent. It hands you one question to \
             answer from the web, and you find the answer with web_search and fetch_url.\n\n\
             - Search with a few different wordings in one reply, then fetch the pages that \
             look best: the project's own docs, release notes and code before blogs.\n\
             - Prefer what is recent, and say how recent it is when it matters.\n\
             - Stop as soon as you can answer.\n\n",
            answer!(),
            " Give the URL of each thing you say beside it."
        ),
        tools: &["web_search", "fetch_url"],
        tier: "balanced",
        rounds: 12,
        works: Works::Looks,
    },
    Spec {
        name: "run",
        prompt: concat!(
            "You are a subagent of uc, the usecode coding agent. It hands you commands to run \
             (a build, the tests, a lint) and you run them with bash in its shell, then give \
             back how they went.\n\n\
             - Run what you were told. Don't change files: say what would fix a failure.\n\
             - A command that takes long carries on as a job; follow it with job_output.\n\
             - Where a failure comes from, read_file and grep can tell.\n\
             - The shell is the agent's own: a cd carries over, so cd back where you \
             started when you are done.\n\n",
            answer!(),
            " For a failure, give the first errors that matter with their file:line, not the \
             whole log."
        ),
        tools: &[
            "bash",
            "job_output",
            "job_kill",
            "read_file",
            "grep",
            "glob",
        ],
        tier: "fast",
        rounds: 25,
        works: Works::Shell,
    },
    Spec {
        name: "edit",
        prompt: concat!(
            "You are a subagent of uc, the usecode coding agent. It hands you one change to \
             make, and you make it in its own tree, in place, for it to look at when you \
             are done.\n\n\
             - Read the code you change first, and follow its style.\n\
             - Change files with edit_file and write_file. Don't delete files: say which \
             should go.\n\
             - Make only the change you were asked for; other agents may change other \
             files beside you.\n\
             - Where bash is there, build and test what you changed before you answer.\n\n",
            answer!(),
            " Say what you changed, file by file, and how you checked it."
        ),
        tools: &[
            "read_file",
            "grep",
            "glob",
            "write_file",
            "edit_file",
            "bash",
        ],
        tier: "balanced",
        rounds: 40,
        works: Works::Tree,
    },
];

/// A `task` call's arguments.
pub(super) struct Task {
    kind: String,
    prompt: String,
    model: String,
}

impl Task {
    pub(super) fn of(call: &Value) -> Task {
        let args: Value = call["function"]["arguments"]
            .as_str()
            .and_then(|a| serde_json::from_str(a).ok())
            .unwrap_or(Value::Null);
        let arg = |k: &str| args[k].as_str().unwrap_or_default().trim().to_string();
        let kind = Some(arg("kind")).filter(|k| !k.is_empty());
        Task {
            kind: kind.unwrap_or_else(|| "explore".into()),
            prompt: arg("prompt"),
            model: arg("model"),
        }
    }
}

/// The kind `task` asks for, when there is one with a prompt.
fn spec(task: &Task) -> Result<&'static Spec> {
    let Some(spec) = KINDS.iter().find(|k| k.name == task.kind) else {
        let kinds: Vec<&str> = KINDS.iter().map(|k| k.name).collect();
        bail!(
            "there is no kind of subagent '{}'; one of {}",
            task.kind,
            kinds.join(", ")
        );
    };
    if task.prompt.is_empty() {
        bail!("a task needs a prompt");
    }
    Ok(spec)
}

/// Whether `call` is a subagent that runs in the session's shell, one at a
/// time, rather than beside the others.
pub(super) fn in_session(call: &Value) -> bool {
    name_of(call) == "task" && spec(&Task::of(call)).is_ok_and(|s| s.works == Works::Shell)
}

/// Beside the spinner while subagents work.
pub(super) fn at_work(n: usize) -> String {
    let s = if n == 1 { " is" } else { "s are" };
    format!(" \x1b[2m· {n} subagent{s} at work\x1b[0m")
}

/// What a child needs of the session.
pub(super) struct Parent<'a> {
    pub(super) providers: &'a [Provider],
    pub(super) pool: &'a [Choice],
    /// The model the session runs on.
    pub(super) choice: Choice,
    pub(super) cache_key: &'a str,
    pub(super) search: &'a Engine,
    /// The session's model for its subagents, empty for each kind's own.
    pub(super) sub_model: &'a str,
    /// Commands run without asking: an `edit` child gets a shell.
    pub(super) allow_all: bool,
}

/// What a child's requests took: `explore on claude-haiku-5.5`, the tokens
/// and the conversation, to save beside the session's.
pub(super) struct Used {
    pub(super) who: String,
    pub(super) tokens: Tokens,
    pub(super) transcript: Value,
}

/// The model a call asks for: its own, else the user's for the kind, else
/// the session's for all, else the kind's tier.
fn asked(spec: &Spec, task: &Task, sub_model: &str) -> String {
    let flag = crate::flag::value(None, &format!("--subagent-model-{}", spec.name));
    asked_from(spec, &task.model, flag, sub_model)
}

fn asked_from(spec: &Spec, model: &str, flag: Option<String>, sub_model: &str) -> String {
    [Some(model.to_string()), flag, Some(sub_model.to_string())]
        .into_iter()
        .flatten()
        .find(|m| !m.trim().is_empty())
        .unwrap_or_else(|| spec.tier.into())
}

/// The models a tier means, by the name they go by on any provider, the one
/// wanted first: a provider takes the first of them it serves.
fn tier(name: &str) -> Option<&'static [&'static str]> {
    match name.to_ascii_lowercase().as_str() {
        "fast" => Some(&["glm-5.3-flash", "claude-haiku-5.5"]),
        "balanced" => Some(&["glm-5.3", "claude-sonnet-5.5"]),
        "strong" => Some(&["claude-opus-5.5"]),
        _ => None,
    }
}

/// The tier above `name`, to try once more on.
fn next_tier(name: &str) -> Option<&'static str> {
    match name.to_ascii_lowercase().as_str() {
        "fast" => Some("balanced"),
        "balanced" => Some("strong"),
        _ => None,
    }
}

/// The model `asked` means for a child, and a line to say when it isn't the
/// one asked for. A tier is taken on the session's provider first, then on
/// the others in `agent.yaml`'s order; a name as `uc agent model` takes it,
/// on a provider with a key; else the session's own model.
fn pick(parent: &Parent<'_>, asked: &str) -> (Choice, Option<String>) {
    let asked = asked.trim();
    if asked.eq_ignore_ascii_case("same") {
        return (parent.choice.clone(), None);
    }
    let keyed = |c: &Choice| named(parent.providers, &c.provider).is_some();
    let found = match tier(asked) {
        Some(names) => {
            let first = std::iter::once(parent.choice.provider.as_str());
            let others = parent
                .providers
                .iter()
                .map(|p| p.name.as_str())
                .filter(|p| *p != parent.choice.provider);
            first.chain(others).find_map(|provider| {
                names.iter().find_map(|name| {
                    parent
                        .pool
                        .iter()
                        .find(|c| c.provider == provider && common_name(&c.model) == *name)
                        .cloned()
                })
            })
        }
        None => choice_named(parent.pool, asked, &parent.choice.provider),
    };
    match found.filter(keyed) {
        Some(choice) => (choice, None),
        None => (
            parent.choice.clone(),
            Some(format!(
                "no {asked} model on a provider with a key; the subagent runs on {}",
                parent.choice.model
            )),
        ),
    }
}

/// `name` as the subagents' model, checked: a tier or `same` as it is, a
/// model as `<id> <provider>`, so it means the same one again.
pub(super) fn named_model(parent: &Parent<'_>, name: &str) -> Result<String> {
    let name = name.trim();
    if tier(name).is_some() || name.eq_ignore_ascii_case("same") {
        return Ok(name.to_ascii_lowercase());
    }
    let choice = choice_named(parent.pool, name, &parent.choice.provider)
        .filter(|c| named(parent.providers, &c.provider).is_some())
        .ok_or_else(|| {
            anyhow!("no model {name} on a provider with a key; `uc agent model` lists them")
        })?;
    Ok(format!("{} {}", choice.model, choice.provider))
}

/// What each kind runs on now: `explore on glm-5.3-flash, research on ...`.
pub(super) fn plan(parent: &Parent<'_>) -> String {
    KINDS
        .iter()
        .map(|spec| {
            let task = Task {
                kind: spec.name.into(),
                prompt: String::new(),
                model: String::new(),
            };
            let (choice, _) = pick(parent, &asked(spec, &task, parent.sub_model));
            format!("{} on {}", spec.name, short_model(&choice.model))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The provider and model a compaction's summary is written on: the
/// session's model for its subagents, else the fast tier.
pub(super) fn summarizer(parent: &Parent<'_>) -> Result<(Provider, String)> {
    let asked = Some(parent.sub_model)
        .filter(|m| !m.is_empty())
        .unwrap_or("fast");
    let (choice, _) = pick(parent, asked);
    let provider = named(parent.providers, &choice.provider)
        .cloned()
        .ok_or_else(|| anyhow!("{}", no_provider()))?;
    Ok((provider, choice.model))
}

/// The few tools a kind has, as [`all_tools`] describes them.
fn tools_for(names: &[&str]) -> Value {
    let all = all_tools();
    Value::Array(
        names
            .iter()
            .filter_map(|name| {
                all.as_array()?
                    .iter()
                    .find(|t| t["function"]["name"] == *name)
                    .cloned()
            })
            .collect(),
    )
}

/// What a kind's child is told: the same for every child of the kind, then,
/// after [`claude::SESSION`], what is this child's own (the directory, and
/// `extra`), so the first part is cached once for all of them.
fn system(spec: &Spec, extra: &str) -> String {
    let cwd = std::env::current_dir()
        .map(|d| d.display().to_string())
        .unwrap_or_default();
    let extra = if extra.is_empty() {
        String::new()
    } else {
        format!(" {extra}")
    };
    format!(
        "{}{}The working directory is {cwd}.{extra}",
        spec.prompt,
        claude::SESSION
    )
}

/// The children allowed at once on `provider`.
fn slots(provider: &str) -> Arc<Semaphore> {
    static SLOTS: LazyLock<Mutex<HashMap<String, Arc<Semaphore>>>> =
        LazyLock::new(Default::default);
    SLOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(provider.to_string())
        .or_insert_with(|| Arc::new(Semaphore::new(AT_ONCE)))
        .clone()
}

/// The gate of the children with `cache_key`: the first holds it until its
/// first reply starts, so its system prompt is in the cache when the others
/// ask; then it says `true`, and the others pass straight through.
fn gate(cache_key: &str) -> Arc<tokio::sync::Mutex<bool>> {
    static GATES: LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<bool>>>>> =
        LazyLock::new(Default::default);
    GATES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(cache_key.to_string())
        .or_default()
        .clone()
}

/// `messages` with the cache marks taken off all but the system prompt.
fn only_system_marked(messages: &mut [Value]) {
    for message in messages.iter_mut().filter(|m| m["role"] != "system") {
        for part in message["content"].as_array_mut().into_iter().flatten() {
            if let Some(part) = part.as_object_mut() {
                part.remove("cache_control");
            }
        }
    }
}

/// The model without its maker's prefix: `claude-haiku-5.5`.
pub(super) fn short_model(model: &str) -> &str {
    model.rsplit('/').next().unwrap_or(model)
}

/// The prompt's first line, cut to fit a status line.
fn headline(prompt: &str) -> String {
    let line = prompt.lines().next().unwrap_or_default().trim();
    let cut: String = line.chars().take(80).collect();
    if cut.len() < line.len() {
        format!("{cut}…")
    } else {
        cut
    }
}

/// `text` cut to [`ANSWER`].
fn cap(mut text: String) -> String {
    if text.len() > ANSWER {
        let mut end = ANSWER;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n[the subagent's answer was cut]");
    }
    text
}

/// How a child ended.
enum Outcome {
    Answer(String),
    /// It made all its requests without a final answer; what it said last.
    Unfinished(String),
}

impl Outcome {
    fn text(self) -> String {
        match self {
            Outcome::Answer(text) | Outcome::Unfinished(text) => text,
        }
    }
}

/// What runs a child's tool calls.
pub(super) trait Hands {
    /// The outputs of `calls`, in their order; failures as text.
    async fn run(&mut self, calls: &[Value]) -> Vec<String>;
}

/// The looking tools, side by side.
struct Looks<'a>(&'a Engine);

impl Hands for Looks<'_> {
    async fn run(&mut self, calls: &[Value]) -> Vec<String> {
        futures_util::future::join_all(calls.iter().map(|call| read_only(self.0, call))).await
    }
}

/// The session's own tools, one after another, as its own calls run.
impl Hands for Session {
    async fn run(&mut self, calls: &[Value]) -> Vec<String> {
        let mut outputs = Vec::new();
        for call in calls {
            // Boxed: `call` runs the subagent that runs this.
            let output: Pin<Box<dyn Future<Output = String> + '_>> = Box::pin(self.call(call));
            outputs.push(output.await);
        }
        outputs
    }
}

/// One child at work.
struct Child {
    spec: &'static Spec,
    provider: Provider,
    model: String,
    effort: String,
    cache_key: String,
    who: String,
    system: String,
    names: Vec<&'static str>,
    tools: Value,
    messages: Vec<Value>,
    tokens: Tokens,
    started: Instant,
}

impl Child {
    /// A child of `spec` on `choice`, with the tools `names`, told `extra`
    /// beside its directory.
    fn new(
        parent: &Parent<'_>,
        spec: &'static Spec,
        choice: Choice,
        extra: &str,
        names: Vec<&'static str>,
    ) -> Result<Child> {
        let provider = named(parent.providers, &choice.provider)
            .cloned()
            .ok_or_else(|| anyhow!("{}", no_provider()))?;
        Ok(Child {
            cache_key: format!(
                "{}-{}-{}",
                parent.cache_key,
                spec.name,
                common_name(&choice.model)
            ),
            who: format!("{} on {}", spec.name, short_model(&choice.model)),
            effort: crate::flag::value(None, "--subagent-effort").unwrap_or_else(|| "low".into()),
            system: system(spec, extra),
            tools: tools_for(&names),
            names,
            model: choice.model,
            provider,
            spec,
            messages: Vec::new(),
            tokens: Tokens::default(),
            started: Instant::now(),
        })
    }

    /// Works on `prompt` with `hands`, once the provider has room for it,
    /// saying when it starts and ends.
    async fn go(&mut self, prompt: &str, hands: &mut impl Hands) -> Result<Outcome> {
        out::note(&format!("{}: {}", self.who, headline(prompt)));
        let slots = slots(&self.provider.name);
        let outcome = match slots.acquire().await {
            Ok(_slot) => self.work(prompt, hands).await,
            Err(err) => Err(anyhow!("waiting for room on the provider: {err}")),
        };
        let how = if outcome.is_ok() { "done" } else { "failed" };
        out::note(&format!("{} {how}: {}", self.who, self.tokens.sub_line()));
        outcome
    }

    /// Asks the model until it answers without calling a tool, or has made
    /// all the requests its kind may.
    async fn work(&mut self, prompt: &str, hands: &mut impl Hands) -> Result<Outcome> {
        self.started = Instant::now();
        self.messages = vec![
            json!({"role": "system", "content": self.system}),
            json!({"role": "user", "content": prompt}),
        ];
        let rounds = self.spec.rounds;
        let mut said = String::new();
        for round in 1..=rounds {
            let first = round == 1;
            let mut messages = for_provider(Some(self.provider.kind), &self.model, &self.messages);
            if first {
                only_system_marked(&mut messages);
            }
            let mut body = json!({
                "model": self.model,
                "messages": messages,
                "tools": self.tools,
                "reasoning": {"effort": self.effort},
                "stream": true,
                "usage": {"include": true},
            });
            if first && self.provider.kind == Kind::Claude {
                body["cache_history"] = json!(false);
            }
            if takes_cache_key(&self.provider) {
                body["prompt_cache_key"] = json!(self.cache_key);
            }
            let stream = if first {
                let gate = gate(&self.cache_key);
                let mut warm = gate.lock().await;
                if *warm {
                    drop(warm);
                    stream_on(&self.provider, &body).await
                } else {
                    let stream = stream_on(&self.provider, &body).await;
                    *warm = stream.is_ok();
                    stream
                }
            } else {
                stream_on(&self.provider, &body).await
            }?;
            let reply = read_reply(stream, |_| {}).await?;
            if let Some(usage) = &reply.usage {
                self.tokens.add(usage);
            }
            let message = reply.message();
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
                if text.is_empty() {
                    bail!("the subagent gave no answer");
                }
                return Ok(Outcome::Answer(cap(text)));
            }
            if !text.is_empty() {
                said = text;
            }
            for (call, output) in calls.iter().zip(self.outputs(&calls, hands).await) {
                self.messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call["id"],
                    "content": truncate(output),
                }));
            }
            if round + 1 == rounds
                && let Some(last) = self.messages.last_mut()
            {
                let text = last["content"].as_str().unwrap_or_default();
                last["content"] = json!(format!("{text}\n\n{LAST}"));
            }
        }
        if said.is_empty() {
            bail!("the subagent made {rounds} requests without an answer");
        }
        Ok(Outcome::Unfinished(cap(format!(
            "[the subagent made {rounds} requests without a final answer; the last it said:]\n{said}"
        ))))
    }

    /// The outputs of a reply's tool calls: the kind's run by `hands`, any
    /// other an error for the model to read.
    async fn outputs(&self, calls: &[Value], hands: &mut impl Hands) -> Vec<String> {
        let ours: Vec<Value> = calls
            .iter()
            .filter(|c| self.names.contains(&name_of(c).as_str()))
            .cloned()
            .collect();
        let mut done = hands.run(&ours).await.into_iter();
        calls
            .iter()
            .map(|call| match self.names.contains(&name_of(call).as_str()) {
                true => done.next().unwrap_or_default(),
                false => format!("error: there is no tool called '{}' here", name_of(call)),
            })
            .collect()
    }

    /// What it took, and its conversation, to save.
    fn used(self, prompt: &str) -> Used {
        let t = &self.tokens;
        let transcript = json!({
            "kind": self.spec.name,
            "provider": self.provider.name,
            "model": self.model,
            "prompt": prompt,
            "seconds": self.started.elapsed().as_secs(),
            "tokens": {
                "requests": t.turn.requests,
                "input": t.input,
                "cached": t.cached,
                "written": t.written,
                "output": t.output,
            },
            "messages": self.messages,
        });
        Used {
            who: self.who,
            tokens: self.tokens,
            transcript,
        }
    }
}

/// Runs a `task` that leaves the session as it is (`explore`, `research`
/// or `edit`): its answer, and what its requests took, also when it failed.
pub(super) async fn run(parent: &Parent<'_>, task: &Task) -> (Result<String>, Vec<Used>) {
    let spec = match spec(task) {
        Ok(spec) => spec,
        Err(err) => return (Err(err), Vec::new()),
    };
    match spec.works {
        Works::Looks => looks(parent, spec, task).await,
        Works::Tree => edit(parent, spec, task).await,
        Works::Shell => (
            Err(anyhow!(
                "a run task has the session's shell, so it goes on its own"
            )),
            Vec::new(),
        ),
    }
}

/// A child that only looks, tried once more a tier up when it fails or
/// runs out of requests.
async fn looks(
    parent: &Parent<'_>,
    spec: &'static Spec,
    task: &Task,
) -> (Result<String>, Vec<Used>) {
    let mut asked = asked(spec, task, parent.sub_model);
    let mut used = Vec::new();
    let mut tried: Vec<Choice> = Vec::new();
    loop {
        let (choice, why) = pick(parent, &asked);
        if let Some(why) = why {
            out::note(&why);
        }
        let names = spec.tools.to_vec();
        let mut child = match Child::new(parent, spec, choice.clone(), "", names) {
            Ok(child) => child,
            Err(err) => return (Err(err), used),
        };
        let outcome = child.go(&task.prompt, &mut Looks(parent.search)).await;
        let who = child.who.clone();
        used.push(child.used(&task.prompt));
        tried.push(choice);
        let failed = match &outcome {
            Ok(Outcome::Answer(_)) => return (outcome.map(Outcome::text), used),
            Ok(Outcome::Unfinished(_)) => "ran out of requests",
            Err(_) => "failed",
        };
        let again = next_tier(&asked)
            .filter(|_| tried.len() == 1)
            .map(|next| (next, pick(parent, next).0))
            .filter(|(_, choice)| !tried.contains(choice));
        let Some((next, choice)) = again else {
            return (outcome.map(Outcome::text), used);
        };
        out::note(&format!(
            "{who} {failed}; once more on {}",
            short_model(&choice.model)
        ));
        asked = next.to_string();
    }
}

/// An `edit` child in the session's own tree, in place: the build caches
/// and the uncommitted work are there, and its change is left uncommitted
/// for the session to look at with `git diff`.
async fn edit(
    parent: &Parent<'_>,
    spec: &'static Spec,
    task: &Task,
) -> (Result<String>, Vec<Used>) {
    let (choice, why) = pick(parent, &asked(spec, task, parent.sub_model));
    if let Some(why) = why {
        out::note(&why);
    }
    let from = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut tree = Tree::open(&from, parent.search, parent.allow_all);
    let names: Vec<&'static str> = spec
        .tools
        .iter()
        .copied()
        .filter(|t| *t != "bash" || tree.commands)
        .collect();
    let commands = if tree.commands {
        "bash runs in the working directory, in a shell of its own."
    } else {
        "There is no bash: the user confirms each command, and you can't ask them."
    };
    let extra = format!(
        "You work in {dir}, in place, beside the agent and maybe other subagents: what is \
         there, uncommitted work and build caches too, is what you see. You only write \
         files under {root}. Don't commit, stash, check out or reset anything: the agent \
         looks at your change with git diff. {commands}",
        dir = tree.dir.display(),
        root = tree.root.display(),
    );
    let mut child = match Child::new(parent, spec, choice, &extra, names) {
        Ok(child) => child,
        Err(err) => return (Err(err), Vec::new()),
    };
    let outcome = child.go(&task.prompt, &mut tree).await;
    let used = vec![child.used(&task.prompt)];
    let change = tree.changed();
    match outcome {
        Ok(outcome) => (Ok(format!("{}\n\n{change}", outcome.text())), used),
        Err(err) => (Err(err.context(change)), used),
    }
}

/// Runs a `run` task in the session's shell: its commands show and ask as
/// the session's own do.
pub(super) async fn run_here(session: &mut Session, task: &Task) -> (Result<String>, Vec<Used>) {
    let spec = match spec(task) {
        Ok(spec) => spec,
        Err(err) => return (Err(err), Vec::new()),
    };
    let child = {
        let parent = session.parent();
        let (choice, why) = pick(&parent, &asked(spec, task, parent.sub_model));
        if let Some(why) = why {
            out::note(&why);
        }
        Child::new(&parent, spec, choice, "", spec.tools.to_vec())
    };
    let mut child = match child {
        Ok(child) => child,
        Err(err) => return (Err(err), Vec::new()),
    };
    let outcome = child.go(&task.prompt, session).await;
    let took = child.started.elapsed();
    if took > SHORT_CACHE
        && session.kind() == Some(Kind::OpenRouter)
        && caches_on_request(&session.model)
        && !claude::LONG_CACHE.load(Ordering::Relaxed)
    {
        out::note(&format!(
            "{} took {}: your conversation stays cached on OpenRouter for five minutes, so \
             the next request writes it again (`uc --long-cache` keeps it an hour)",
            child.who,
            elapsed(took.as_secs())
        ));
    }
    (outcome.map(Outcome::text), vec![child.used(&task.prompt)])
}

/// The tree an `edit` child works in: the session's own, in place, so the
/// build caches and the uncommitted work are there. Paths are taken from
/// the working directory, files are only written under the repository (or
/// the working directory, out of one), and commands run in a shell of their
/// own.
struct Tree<'a> {
    /// The repository's top directory, else the working directory: where
    /// the child may write.
    root: PathBuf,
    /// The working directory, where relative paths and commands start.
    dir: PathBuf,
    search: &'a Engine,
    /// It has bash: commands run without asking in the session.
    commands: bool,
    /// The files the child wrote, for the session to look at.
    written: Vec<PathBuf>,
}

impl<'a> Tree<'a> {
    /// The tree around `from`: the repository it is in, else `from` alone.
    fn open(from: &Path, search: &'a Engine, commands: bool) -> Tree<'a> {
        let dir = from.canonicalize().unwrap_or_else(|_| from.to_path_buf());
        let root = dir
            .ancestors()
            .find(|d| d.join(".git").exists())
            .unwrap_or(&dir)
            .to_path_buf();
        Tree {
            root,
            dir,
            search,
            commands,
            written: Vec::new(),
        }
    }

    /// `path` taken from the working directory.
    fn map(&self, path: &str) -> PathBuf {
        let path = Path::new(path.trim());
        if path.as_os_str().is_empty() {
            return self.dir.clone();
        }
        self.dir.join(path)
    }

    /// `path`, when it is under the root, for writing.
    fn writable(&self, path: &str) -> Result<PathBuf> {
        let mapped = self.map(path);
        let up = mapped
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir));
        if up || !mapped.starts_with(&self.root) {
            bail!(
                "{path} is outside {}; an edit task only changes files under it",
                self.root.display()
            );
        }
        Ok(mapped)
    }

    /// Notes that the child wrote `file`.
    fn wrote(&mut self, file: PathBuf) {
        if !self.written.contains(&file) {
            self.written.push(file);
        }
    }

    /// `call` with its path taken from the working directory.
    fn mapped(&self, call: &Value) -> Value {
        let mut call = call.clone();
        let mut args: Value = call["function"]["arguments"]
            .as_str()
            .and_then(|a| serde_json::from_str(a).ok())
            .unwrap_or_else(|| json!({}));
        let path = args["path"].as_str().unwrap_or_default().to_string();
        args["path"] = json!(self.map(&path).display().to_string());
        call["function"]["arguments"] = json!(args.to_string());
        call
    }

    fn write(&mut self, path: &str, content: &str) -> Result<String> {
        let file = self.writable(path)?;
        out::note(&format!(
            "edit: write {} ({} lines)",
            self.shown(&file),
            content.lines().count()
        ));
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).context("creating the directory")?;
        }
        std::fs::write(&file, content).context("writing the file")?;
        self.wrote(file.clone());
        Ok(format!("wrote {}", file.display()))
    }

    fn edit(&mut self, path: &str, old: &str, new: &str) -> Result<String> {
        let file = self.writable(path)?;
        let text = std::fs::read_to_string(&file).context("reading the file")?;
        match text.matches(old).count() {
            1 => {}
            0 => bail!("old_text is not in {path}"),
            n => bail!("old_text occurs {n} times in {path}; give more context"),
        }
        out::note(&format!("edit: edit {}", self.shown(&file)));
        std::fs::write(&file, text.replacen(old, new, 1)).context("writing the file")?;
        self.wrote(file.clone());
        Ok(format!("edited {}", file.display()))
    }

    /// `file` as the working directory names it.
    fn shown(&self, file: &Path) -> String {
        file.strip_prefix(&self.dir)
            .unwrap_or(file)
            .display()
            .to_string()
    }

    /// Runs `command` with bash in the working directory, in a shell of its own,
    /// stopped after `timeout` seconds.
    async fn bash(&self, command: &str, title: &str, timeout: u64) -> String {
        if deletes(command) {
            return "error: deleting files is for the agent to do, as the user is asked first; \
                    say what should go in your answer"
                .into();
        }
        out::note(&format!(
            "edit: {}",
            if title.is_empty() { command } else { title }
        ));
        let spawned = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .current_dir(&self.dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .kill_on_drop(true)
            .spawn();
        let child = match spawned {
            Ok(child) => child,
            Err(err) => return format!("error: starting bash: {err}"),
        };
        let group = child.id().map(|id| id as i32);
        let secs = timeout.clamp(1, MAX_TIMEOUT);
        match tokio::time::timeout(Duration::from_secs(secs), child.wait_with_output()).await {
            Ok(Ok(output)) => format!(
                "exit status: {}\noutput:\n{}{}",
                output.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
            Ok(Err(err)) => format!("error: running bash: {err}"),
            Err(_) => {
                if let Some(group) = group {
                    signal_group(group, libc::SIGKILL);
                }
                format!("error: still running after {secs}s, so it was stopped")
            }
        }
    }

    /// What the child changed, for the session to look at.
    fn changed(&self) -> String {
        let others = if self.commands {
            " Its commands may have changed more: `git status` shows it."
        } else {
            ""
        };
        if self.written.is_empty() {
            return format!("[It wrote no file.{others}]");
        }
        let files: Vec<String> = self.written.iter().map(|f| self.shown(f)).collect();
        format!(
            "[It changed, in place and uncommitted: {}. Look at it with `git diff -- {}` \
             before you go on.{others}]",
            files.join(", "),
            files.join(" ")
        )
    }
}

impl Hands for Tree<'_> {
    async fn run(&mut self, calls: &[Value]) -> Vec<String> {
        let mut outputs = Vec::new();
        for call in calls {
            let args: Value = call["function"]["arguments"]
                .as_str()
                .and_then(|a| serde_json::from_str(a).ok())
                .unwrap_or(Value::Null);
            let arg = |k: &str| args[k].as_str().unwrap_or_default().to_string();
            let output = match name_of(call).as_str() {
                "read_file" | "grep" | "glob" => read_only(self.search, &self.mapped(call)).await,
                "write_file" => self
                    .write(&arg("path"), &arg("content"))
                    .unwrap_or_else(|err| format!("error: {err:#}")),
                "edit_file" => self
                    .edit(&arg("path"), &arg("old_text"), &arg("new_text"))
                    .unwrap_or_else(|err| format!("error: {err:#}")),
                "bash" if self.commands => {
                    let timeout = args["timeout"].as_u64().unwrap_or(DEFAULT_TIMEOUT);
                    self.bash(&arg("command"), &arg("title"), timeout).await
                }
                other => format!("error: there is no tool called '{other}' here"),
            };
            outputs.push(output);
        }
        outputs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(name: &str, kind: Kind) -> Provider {
        Provider::new(name, kind)
    }

    fn pool() -> Vec<Choice> {
        let mut pool: Vec<Choice> = MODELS
            .iter()
            .map(|m| Choice::new("openrouter", m))
            .collect();
        pool.extend(CLAUDE_MODELS.iter().map(|m| Choice::new("claude", m)));
        pool.push(Choice::new("copilot", "claude-haiku-5-5"));
        pool.push(Choice::new("copilot", "claude-opus-5-5"));
        pool.push(Choice::new("groq", "llama-3.3-70b-versatile"));
        pool
    }

    fn parent<'a>(providers: &'a [Provider], pool: &'a [Choice], on: Choice) -> Parent<'a> {
        Parent {
            providers,
            pool,
            choice: on,
            cache_key: "uc-test",
            search: &Engine::DuckDuckGo,
            sub_model: "",
            allow_all: true,
        }
    }

    fn picked(providers: &[Provider], on: Choice, asked: &str) -> (String, String, bool) {
        let pool = pool();
        let (choice, why) = pick(&parent(providers, &pool, on), asked);
        (choice.provider, choice.model, why.is_some())
    }

    fn s(provider: &str, model: &str, fell_back: bool) -> (String, String, bool) {
        (provider.into(), model.into(), fell_back)
    }

    fn kind(name: &str) -> &'static Spec {
        KINDS.iter().find(|k| k.name == name).unwrap()
    }

    #[test]
    fn a_tier_is_taken_on_the_sessions_provider_first() {
        let all = [
            provider("openrouter", Kind::OpenRouter),
            provider("claude", Kind::Claude),
            provider("copilot", Kind::Copilot),
        ];
        let on = |p: &str, m: &str| Choice::new(p, m);
        let opus = "anthropic/claude-opus-5.5";
        assert_eq!(
            picked(&all, on("openrouter", opus), "fast"),
            s("openrouter", "z-ai/glm-5.3-flash", false)
        );
        assert_eq!(
            picked(&all, on("claude", opus), "fast"),
            s("claude", "anthropic/claude-haiku-5.5", false)
        );
        assert_eq!(
            picked(&all, on("copilot", "claude-opus-5-5"), "fast"),
            s("copilot", "claude-haiku-5-5", false)
        );
        assert_eq!(
            picked(&all, on("claude", opus), "balanced"),
            s("claude", "anthropic/claude-sonnet-5.5", false)
        );
    }

    #[test]
    fn a_tier_the_provider_lacks_is_taken_on_another() {
        let both = [
            provider("groq", Kind::OpenAi),
            provider("openrouter", Kind::OpenRouter),
        ];
        let on = Choice::new("groq", "llama-3.3-70b-versatile");
        assert_eq!(
            picked(&both, on, "strong"),
            s("openrouter", "anthropic/claude-opus-5.5", false)
        );
    }

    #[test]
    fn with_no_key_for_it_the_child_runs_on_the_sessions_model() {
        let groq = [provider("groq", Kind::OpenAi)];
        let on = Choice::new("groq", "llama-3.3-70b-versatile");
        let llama = s("groq", "llama-3.3-70b-versatile", true);
        assert_eq!(picked(&groq, on.clone(), "fast"), llama);
        assert_eq!(picked(&groq, on, "opus"), llama);
    }

    #[test]
    fn same_and_names_are_taken_as_they_are() {
        let all = [
            provider("openrouter", Kind::OpenRouter),
            provider("claude", Kind::Claude),
        ];
        let on = Choice::new("claude", "anthropic/claude-opus-5.5");
        assert_eq!(
            picked(&all, on.clone(), "same"),
            s("claude", "anthropic/claude-opus-5.5", false)
        );
        assert_eq!(
            picked(&all, on, "haiku"),
            s("claude", "anthropic/claude-haiku-5.5", false)
        );
    }

    #[test]
    fn a_named_model_is_saved_as_its_id_and_provider() {
        let all = [
            provider("openrouter", Kind::OpenRouter),
            provider("claude", Kind::Claude),
        ];
        let pool = pool();
        let parent = parent(
            &all,
            &pool,
            Choice::new("claude", "anthropic/claude-opus-5.5"),
        );
        assert_eq!(named_model(&parent, "Fast").unwrap(), "fast");
        assert_eq!(named_model(&parent, "same").unwrap(), "same");
        let haiku = named_model(&parent, "haiku").unwrap();
        assert_eq!(haiku, "anthropic/claude-haiku-5.5 claude");
        assert_eq!(
            pick(&parent, &haiku).0,
            Choice::new("claude", "anthropic/claude-haiku-5.5")
        );
        assert!(named_model(&parent, "no-such-model").is_err());
    }

    #[test]
    fn the_narrowest_model_wins() {
        let explore = kind("explore");
        assert_eq!(asked_from(explore, "", None, ""), "fast");
        assert_eq!(asked_from(kind("research"), "", None, ""), "balanced");
        assert_eq!(asked_from(explore, "", None, "strong"), "strong");
        assert_eq!(
            asked_from(explore, "", Some("haiku".into()), "strong"),
            "haiku"
        );
        assert_eq!(
            asked_from(explore, "same", Some("haiku".into()), "strong"),
            "same"
        );
    }

    #[test]
    fn a_child_that_fails_is_tried_a_tier_up() {
        assert_eq!(next_tier("fast"), Some("balanced"));
        assert_eq!(next_tier("Balanced"), Some("strong"));
        assert_eq!(next_tier("strong"), None);
        assert_eq!(next_tier("haiku"), None);
    }

    #[test]
    fn each_kind_has_only_its_tools() {
        let names = |spec: &Spec| -> Vec<String> {
            tools_for(spec.tools)
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["function"]["name"].as_str().unwrap().to_string())
                .collect()
        };
        for spec in KINDS {
            assert_eq!(names(spec), spec.tools, "{}", spec.name);
            assert!(!spec.tools.contains(&"task"), "a subagent has no subagents");
        }
    }

    #[test]
    fn only_run_has_the_sessions_shell() {
        let call = |kind: &str| json!({"function": {"name": "task", "arguments": json!({"kind": kind, "prompt": "x"}).to_string()}});
        assert!(in_session(&call("run")));
        for kind in ["explore", "research", "edit", ""] {
            assert!(!in_session(&call(kind)), "{kind}");
        }
        assert_eq!(Task::of(&call("")).kind, "explore");
        assert!(spec(&Task::of(&call("nope"))).is_err());
    }

    #[test]
    fn the_part_every_child_shares_comes_first() {
        for spec in KINDS {
            let text = system(spec, "In a worktree.");
            let parts = claude::split(&text);
            assert_eq!(parts.len(), 2);
            assert_eq!(parts[0], spec.prompt);
            assert!(parts[1].contains("The working directory is"));
            assert!(parts[1].ends_with(" In a worktree."));
        }
    }

    #[test]
    fn requests_are_built_for_the_childs_provider() {
        let messages = vec![
            json!({"role": "system", "content": "the prompt"}),
            json!({"role": "user", "content": "where is it?"}),
            json!({"role": "assistant", "content": "looking", claude::CONTENT: [{"type": "thinking"}]}),
            json!({"role": "user", "content": "go on"}),
        ];
        let haiku = "anthropic/claude-haiku-5.5";
        let has = |m: &[Value], what: &str| m.iter().any(|m| m.to_string().contains(what));
        let claude = for_provider(Some(Kind::Claude), haiku, &messages);
        assert!(has(&claude, claude::CONTENT));
        assert!(has(&claude, "cache_control"));
        for kind in [Kind::OpenRouter, Kind::OpenAi, Kind::Copilot] {
            assert!(!has(
                &for_provider(Some(kind), haiku, &messages),
                claude::CONTENT
            ));
        }
        let glm = "z-ai/glm-5.3-flash";
        assert!(has(
            &for_provider(Some(Kind::OpenRouter), haiku, &messages),
            "cache_control"
        ));
        assert!(!has(
            &for_provider(Some(Kind::OpenRouter), glm, &messages),
            "cache_control"
        ));
        assert!(!has(
            &for_provider(Some(Kind::Copilot), haiku, &messages),
            "cache_control"
        ));
    }

    #[test]
    fn a_first_request_marks_only_the_system_prompt() {
        let messages = vec![
            json!({"role": "system", "content": "the prompt"}),
            json!({"role": "user", "content": "where is it?"}),
        ];
        let mut marked = for_provider(
            Some(Kind::OpenRouter),
            "anthropic/claude-haiku-5.5",
            &messages,
        );
        only_system_marked(&mut marked);
        assert!(marked[0].to_string().contains("cache_control"));
        assert!(!marked[1].to_string().contains("cache_control"));
    }

    #[test]
    fn an_answer_is_cut_to_its_cap() {
        assert_eq!(cap("short".into()), "short");
        let long = cap("é".repeat(ANSWER));
        assert!(long.len() < ANSWER + 64);
        assert!(long.ends_with("[the subagent's answer was cut]"));
    }

    fn tree(root: &str) -> Tree<'static> {
        Tree {
            root: PathBuf::from(root),
            dir: PathBuf::from(root).join("lib"),
            search: &Engine::DuckDuckGo,
            commands: false,
            written: Vec::new(),
        }
    }

    #[test]
    fn paths_are_taken_from_the_working_directory() {
        let tree = tree("/r");
        let dir = PathBuf::from("/r/lib");
        assert_eq!(tree.map(""), dir);
        assert_eq!(tree.map("src/a.rs"), dir.join("src/a.rs"));
        assert_eq!(tree.map("/r/src/a.rs"), PathBuf::from("/r/src/a.rs"));
        assert_eq!(tree.map("/etc/hosts"), PathBuf::from("/etc/hosts"));
        assert!(tree.writable("src/a.rs").is_ok());
        assert!(tree.writable("/r/README").is_ok());
        assert!(tree.writable("/etc/hosts").is_err());
        assert!(tree.writable("../../x").is_err());
        let call = json!({"function": {"name": "grep", "arguments": "{\"pattern\":\"x\"}"}});
        let args: Value = serde_json::from_str(
            tree.mapped(&call)["function"]["arguments"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(args["path"], "/r/lib");
        assert_eq!(args["pattern"], "x");
    }

    #[tokio::test]
    async fn an_edit_is_made_in_place_and_left_uncommitted() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path().canonicalize().unwrap();
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::create_dir(root.join("lib")).unwrap();
        std::fs::write(root.join("lib/a.txt"), "one\n").unwrap();
        let search = Engine::DuckDuckGo;
        let mut tree = Tree::open(&root.join("lib"), &search, true);
        assert_eq!(tree.root, root);
        assert_eq!(tree.dir, root.join("lib"));
        let call = |name: &str, args: Value| json!({"function": {"name": name, "arguments": args.to_string()}});
        let outputs = tree
            .run(&[
                call(
                    "edit_file",
                    json!({"path": "a.txt", "old_text": "one", "new_text": "two"}),
                ),
                call("write_file", json!({"path": "b/c.txt", "content": "new\n"})),
                call(
                    "bash",
                    json!({"command": "cat a.txt b/c.txt; mkdir cache; touch cache/x.pyc", "title": "look"}),
                ),
                call("bash", json!({"command": "rm a.txt", "title": "delete"})),
                call(
                    "write_file",
                    json!({"path": "/etc/uc-test", "content": "x"}),
                ),
            ])
            .await;
        assert!(outputs[0].starts_with("edited"), "{}", outputs[0]);
        assert!(outputs[2].contains("two\nnew\n"), "{}", outputs[2]);
        assert!(outputs[3].starts_with("error: deleting"), "{}", outputs[3]);
        assert!(outputs[4].contains("is outside"), "{}", outputs[4]);
        // The change is in the tree itself, and the build caches stay.
        assert_eq!(
            std::fs::read_to_string(root.join("lib/a.txt")).unwrap(),
            "two\n"
        );
        assert!(root.join("lib/cache/x.pyc").exists());
        let said = tree.changed();
        assert!(said.contains("uncommitted: a.txt, b/c.txt."), "{said}");
        assert!(said.contains("`git diff -- a.txt b/c.txt`"), "{said}");
        assert!(said.contains("`git status`"), "{said}");
    }
}
