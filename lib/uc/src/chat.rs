// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc` with no command: a coding agent on the terminal.
//!
//! Each line typed goes to a model on OpenRouter, which answers or asks to
//! run a shell command, read, write or edit a file in the current directory.
//! Commands, writes and edits are shown and run straight away; with `--ask`
//! they are confirmed first (`a` allows the rest of the session, a line ending
//! in `-y` allows what its answer does). Deleting files is always confirmed.
//! A command still running after its timeout carries on as a job the model
//! follows and stops itself. Only the model's replies are printed, not what
//! the commands print, unless `--output` is given. What the model is told about all this is the main
//! skill, `chat/main.md`, always in its system prompt. The key comes from
//! `OPENROUTER_API_KEY`, the model from `UC_MODEL`, the reasoning effort from
//! `UC_EFFORT`.

use anyhow::{Context, Result, anyhow, bail};
use rustyline::error::ReadlineError;
use rustyline::{
    Cmd, ConditionalEventHandler, DefaultEditor, Event, EventContext, EventHandler, KeyEvent,
    Movement, RepeatCount,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::{ExitCode, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::task::JoinHandle;

const API: &str = "https://openrouter.ai/api/v1/chat/completions";
const DEFAULT_MODEL: &str = "deepseek/deepseek-v4.1-flash";
const DEFAULT_EFFORT: &str = "high";
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

/// The process groups of the jobs still running, stopped when uc exits.
static GROUPS: Mutex<BTreeSet<i32>> = Mutex::new(BTreeSet::new());

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
/// `output` shows what the commands print as well as the model's replies.
pub fn main(ask: bool, output: bool) -> ExitCode {
    // Worker threads keep reading the jobs' pipes while the prompt waits.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");
    let result = runtime.block_on(run(ask, output));
    stop_jobs();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc: {err:#}");
            ExitCode::FAILURE
        }
    }
}

struct Session {
    key: String,
    model: String,
    /// OpenRouter's reasoning effort: low, medium or high.
    effort: String,
    messages: Vec<Value>,
    /// Auto mode, or `a` at a confirmation: run everything without asking.
    allow_all: bool,
    /// The line ended in `-y`: run what this turn asks for without asking.
    allow_turn: bool,
    /// Print what commands and jobs print, not only the model's replies.
    show_output: bool,
    jobs: BTreeMap<usize, Job>,
    next_job: usize,
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

async fn run(ask: bool, output: bool) -> Result<()> {
    let key = std::env::var("OPENROUTER_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| anyhow!("set OPENROUTER_API_KEY (https://openrouter.ai/keys)"))?;
    let env_or = |name: &str, default: &str| {
        std::env::var(name)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| default.to_string())
    };
    let model = env_or("UC_MODEL", DEFAULT_MODEL);
    let effort = env_or("UC_EFFORT", DEFAULT_EFFORT);
    let mut session = Session {
        key: key.trim().to_string(),
        messages: vec![json!({"role": "system", "content": system_prompt(&model, ask, output)})],
        model,
        effort,
        allow_all: !ask,
        allow_turn: false,
        show_output: output,
        jobs: BTreeMap::new(),
        next_job: 1,
    };
    tokio::spawn(async {
        if tokio::signal::ctrl_c().await.is_ok() {
            stop_jobs();
            eprintln!();
            std::process::exit(130);
        }
    });
    loop {
        let Some(line) = read_prompt("\x1b[38;5;208m₿\x1b[0m ")? else {
            eprintln!();
            return Ok(());
        };
        let line = line.trim();
        if matches!(line, "/exit" | "/quit") {
            return Ok(());
        }
        let (line, approved) = split_approval(line);
        if line.is_empty() {
            continue;
        }
        session.allow_turn = approved;
        session
            .messages
            .push(json!({"role": "user", "content": line}));
        if let Err(err) = session.turn().await {
            eprintln!("uc: {err:#}");
        }
        session.allow_turn = false;
    }
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
        "The user sees the commands you run but not their output, only your replies, so tell \
         them what they need from it: the answer, not the raw output."
    };
    format!(
        "You are uc, the usecode coding agent, running in a terminal on the user's machine, \
         on the model {model} via OpenRouter. The working directory is {cwd}. Use the tools to \
         look around, run commands and change files; {confirms}. {sees}\n\n{MAIN_SKILL}"
    )
}

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
             closed; returns its exit status and output (stdout and stderr together). A command \
             still running after the timeout carries on as a job. Deleting files always asks the \
             user first. The exception is ssh-add: it runs on the user's terminal so they can \
             type the key's passphrase.",
            json!({
                "command": string("the command line"),
                "timeout": integer("seconds to wait before it carries on as a job; 120 if left out"),
                "background": boolean("start it as a job and return straight away, for servers and watchers"),
                "ask": boolean("ask the user first, for anything hard to undo or reaching outside this machine"),
            }),
            &["command"],
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
            "read_file",
            "Read a text file.",
            json!({"path": string("the file's path")}),
            &["path"],
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
    ])
}

impl Session {
    /// Asks the model until it answers without calling a tool.
    async fn turn(&mut self) -> Result<()> {
        loop {
            let message = self.complete().await?;
            if let Some(text) = message["content"].as_str().filter(|t| !t.trim().is_empty()) {
                println!("{}\n", text.trim());
            }
            let calls = message["tool_calls"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            self.messages.push(message);
            if calls.is_empty() {
                return Ok(());
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

    async fn complete(&self) -> Result<Value> {
        let body = json!({
            "model": self.model,
            "messages": self.messages,
            "tools": tools(),
            "reasoning": {"effort": self.effort},
        });
        let response = client()
            .post(API)
            .bearer_auth(&self.key)
            .header("X-Title", "usecode")
            .json(&body)
            .send()
            .await
            .context("calling OpenRouter")?;
        let status = response.status();
        let value: Value = response
            .json()
            .await
            .context("reading OpenRouter's reply")?;
        if !status.is_success() || value.get("error").is_some() {
            let msg = value["error"]["message"].as_str().unwrap_or("no message");
            bail!("OpenRouter: {status}: {msg}");
        }
        let mut message = value["choices"][0]["message"].clone();
        if !message.is_object() {
            bail!("OpenRouter sent no message");
        }
        // Only what the next request needs goes back into the history.
        let keep = ["role", "content", "tool_calls"];
        if let Some(map) = message.as_object_mut() {
            map.retain(|k, _| keep.contains(&k.as_str()));
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
                self.bash(&arg("command"), timeout, flag("background"), flag("ask"))
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
            "read_file" => {
                eprintln!("· read {}", arg("path"));
                std::fs::read_to_string(arg("path")).context("reading the file")
            }
            "write_file" => self.write_file(&arg("path"), &arg("content")),
            "edit_file" => self.edit_file(&arg("path"), &arg("old_text"), &arg("new_text")),
            other => Err(anyhow!("there is no tool called '{other}'")),
        };
        result.unwrap_or_else(|err| format!("error: {err:#}"))
    }

    async fn bash(
        &mut self,
        command: &str,
        timeout: u64,
        background: bool,
        ask: bool,
    ) -> Result<String> {
        // The ₿ in Gruvbox fg (#ebdbb2) is the shell prompt from the dotfiles' .bashrc.
        eprintln!("\x1b[38;2;235;219;178m₿\x1b[0m {command}");
        self.confirm("run it?", ask || deletes(command))?;
        if on_terminal(command) {
            return self.run_on_terminal(command).await;
        }
        let id = self.start(command, !background && self.show_output)?;
        if background {
            eprintln!("· job {id} started");
            return Ok(format!("started job {id}"));
        }
        self.wait(id, timeout).await;
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
        let status = tokio::process::Command::new("bash")
            .args(["-c", command])
            .status()
            .await
            .context("starting bash")?;
        Ok(format!(
            "{status}\noutput: (it ran on the user's terminal, not captured)"
        ))
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
        let clock = quiet.then(|| tokio::spawn(elapsed()));
        let waited = tokio::time::timeout(Duration::from_secs(secs), job.child.wait()).await;
        if let Some(clock) = clock {
            clock.abort();
            let _ = clock.await;
            eprint!("\r\x1b[2K");
        }
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
        for signal in [libc::SIGTERM, libc::SIGKILL] {
            if self.jobs[&id].status.is_some() {
                break;
            }
            signal_group(group, signal);
            self.wait(id, 3).await;
        }
        eprintln!("· job {id} stopped");
        let (status, text) = self.take(id);
        Ok(format!("{status}\noutput:\n{text}"))
    }

    fn write_file(&mut self, path: &str, content: &str) -> Result<String> {
        eprintln!("· write {path} ({} lines)", content.lines().count());
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
        eprintln!("· edit {path}");
        for line in old.lines() {
            eprintln!("  - {line}");
        }
        for line in new.lines() {
            eprintln!("  + {line}");
        }
        self.confirm("apply it?", false)?;
        std::fs::write(path, text.replacen(old, new, 1)).context("writing the file")?;
        Ok(format!("edited {path}"))
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

/// Shows how long a wait has taken, once it passes a few seconds, until aborted.
async fn elapsed() {
    let start = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let secs = start.elapsed().as_secs();
        if secs >= 3 {
            eprint!("\r\x1b[2K· running {secs}s");
        }
    }
}

/// The line editor every prompt reads through, so arrow keys edit the line.
static EDITOR: Mutex<Option<DefaultEditor>> = Mutex::new(None);

/// Where the lines typed at the main prompt are kept between sessions.
fn history_path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(state.join("usecode/chat_history"))
}

/// A line typed at the main prompt; Up and Down go through the earlier ones,
/// this session's and the ones before. None at end of input.
fn read_prompt(prompt: &str) -> Result<Option<String>> {
    let line = read_line(prompt)?;
    if let Some(line) = line.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
        let mut editor = EDITOR.lock().unwrap_or_else(|e| e.into_inner());
        if let (Some(editor), Some(path)) = (editor.as_mut(), history_path()) {
            let _ = editor.add_history_entry(line);
            let _ = std::fs::create_dir_all(path.parent().unwrap_or(&path));
            let _ = editor.append_history(&path);
        }
    }
    Ok(line)
}

/// Ctrl-R picks an earlier line with fzf, newest first, like it does in bash.
/// Without fzf it is the usual reverse search.
struct FzfHistory;

impl ConditionalEventHandler for FzfHistory {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, ctx: &EventContext) -> Option<Cmd> {
        let text = std::fs::read_to_string(history_path()?).unwrap_or_default();
        let mut seen = BTreeSet::new();
        let mut lines = Vec::new();
        for line in history_lines(&text).into_iter().rev() {
            if seen.insert(line.clone()) {
                lines.push(line);
            }
        }
        let mut fzf = std::process::Command::new("fzf")
            .args([
                "--height=40%",
                "--reverse",
                "--scheme=history",
                "--read0",
                "--print0",
            ])
            .args(["--bind=ctrl-r:toggle-sort", "--query", ctx.line()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .ok()?;
        if let Some(mut stdin) = fzf.stdin.take() {
            let _ = stdin.write_all(lines.join("\0").as_bytes());
        }
        let picked = fzf.wait_with_output().ok()?;
        let picked = String::from_utf8_lossy(&picked.stdout);
        match picked.trim_end_matches('\0') {
            "" => Some(Cmd::Repaint),
            line => Some(Cmd::Replace(Movement::WholeBuffer, Some(line.to_string()))),
        }
    }
}

/// The entries of a rustyline history file, oldest first; `#V2` files escape
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

/// A line from stdin after `prompt`; None at end of input. Ctrl-C quits uc.
fn read_line(prompt: &str) -> Result<Option<String>> {
    let mut editor = EDITOR.lock().unwrap_or_else(|e| e.into_inner());
    if editor.is_none() {
        let mut new = DefaultEditor::new().context("opening the terminal")?;
        if let Some(path) = history_path() {
            let _ = new.load_history(&path);
        }
        new.bind_sequence(
            KeyEvent::ctrl('R'),
            EventHandler::Conditional(Box::new(FzfHistory)),
        );
        *editor = Some(new);
    }
    match editor.as_mut().map(|e| e.readline(prompt)) {
        Some(Ok(line)) => Ok(Some(line)),
        Some(Err(ReadlineError::Eof)) => Ok(None),
        Some(Err(ReadlineError::Interrupted)) => {
            stop_jobs();
            std::process::exit(130);
        }
        Some(Err(err)) => Err(err).context("reading stdin"),
        None => Ok(None),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_on_a_char_boundary() {
        let long = "é".repeat(MAX_OUTPUT);
        let cut = truncate(long);
        assert!(cut.ends_with("[output cut]"));
        assert!(cut.len() <= MAX_OUTPUT + "\n[output cut]".len());
        assert_eq!(truncate("short".into()), "short");
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

    fn session() -> Session {
        Session {
            key: String::new(),
            model: String::new(),
            effort: String::new(),
            messages: Vec::new(),
            allow_all: true,
            allow_turn: false,
            show_output: false,
            jobs: BTreeMap::new(),
            next_job: 1,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_slow_command_carries_on_as_a_job() {
        let mut s = session();
        let first = s
            .bash("echo a; sleep 2; echo b", 1, false, false)
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
        s.bash("sleep 100 | cat", 0, true, false).await.unwrap();
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
        let out = s.bash("exec 3</dev/tty", 5, false, false).await.unwrap();
        assert!(!out.starts_with("exit status: 0"), "{out}");
    }

    #[test]
    fn every_tool_has_a_name_and_parameters() {
        for tool in tools().as_array().unwrap() {
            assert!(tool["function"]["name"].is_string());
            assert_eq!(tool["function"]["parameters"]["type"], "object");
        }
    }
}
