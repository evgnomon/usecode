// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc` with no command: a coding agent on the terminal.
//!
//! Each line typed goes to a model on OpenRouter, which answers or asks to
//! run a shell command, read, write or edit a file in the current directory,
//! search the web (DuckDuckGo's HTML page, no key) or read a page (rendered by
//! Cloudflare when `CLOUDFLARE_ACCOUNT_ID` is set, else fetched as is).
//! Commands, writes and edits are shown and run straight away; with `--ask`
//! they are confirmed first (`a` allows the rest of the session, a line ending
//! in `-y` allows what its answer does). Deleting files is always confirmed.
//! A command still running after its timeout carries on as a job the model
//! follows and stops itself. Only the model's replies are printed, not what
//! the commands print, unless `--output` is given. A line that is a command
//! (`ls -al`, `git status`, or anything after `!`) runs on the terminal
//! straight away, with the same confirmations, and the model is told it ran;
//! the user's bash aliases and functions work there too;
//! `--no-shell` sends every line to the model. Esc or Ctrl-C stops the model,
//! and the command it is waiting on, and goes back to the prompt; Ctrl-C at
//! the prompt quits.
//! What the model is told about all this is the main skill, `chat/main.md`, always in its system prompt. The key comes from
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
const DUCKDUCKGO: &str = "https://html.duckduckgo.com/html/";
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
/// `output` shows what the commands print as well as the model's replies;
/// `shell` runs a line that is a command instead of sending it to the model.
pub fn main(ask: bool, output: bool, shell: bool) -> ExitCode {
    // Worker threads keep reading the jobs' pipes while the prompt waits.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");
    let result = runtime.block_on(run(ask, output, shell));
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
    /// The job bash is waiting on, stopped if Esc is pressed meanwhile.
    waiting: Option<usize>,
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

async fn run(ask: bool, output: bool, shell: bool) -> Result<()> {
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
        waiting: None,
    };
    if shell {
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
            restore_terminal(&mut SAVED.lock().unwrap_or_else(|e| e.into_inner()));
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
        if matches!(line, "/exit" | "/quit" | "exit") {
            return Ok(());
        }
        let (line, approved) = split_approval(line);
        if line.is_empty() {
            continue;
        }
        session.allow_turn = approved;
        let work = async {
            match typed_command(line).filter(|_| shell) {
                Some(command) => session.run_typed(command).await,
                None => {
                    session
                        .messages
                        .push(json!({"role": "user", "content": line}));
                    session.turn().await
                }
            }
        };
        let ctrl_c = STOP.notified();
        tokio::pin!(ctrl_c);
        ctrl_c.as_mut().enable();
        BUSY.store(true, Ordering::Relaxed);
        let (done, stopped) = tokio::select! {
            done = work => match done {
                Err(err) if err.is::<Stopped>() => (Ok(()), true),
                done => (done, false),
            },
            () = escape() => (Ok(()), true),
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
    }
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
    /// Each alias and what it stands for.
    aliases: BTreeMap<String, String>,
    functions: BTreeSet<String>,
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
}

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
                        declare -F; printf '\\0'; declare -f";
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
    let mut parts = dump.rsplitn(5, '\0');
    let (functions, names, aliases, options) =
        (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    parts.next()?;
    Some(UserShell {
        // The options first: bash-completion's functions need extglob to parse.
        definitions: format!("{options}shopt -s expand_aliases\n{aliases}{functions}"),
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
            None => Some(PathBuf::from(arg)),
        },
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
            "web_search" => {
                let count = number("count").unwrap_or(10).clamp(1, 10);
                web_search(&arg("query"), count).await
            }
            "fetch_url" => fetch_url(&arg("url")).await,
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
    /// after the same confirmations as the model's, and the model is told.
    async fn run_typed(&mut self, command: &str) -> Result<()> {
        let shell = user_shell();
        self.confirm("run it?", deletes(&shell.expand(command)))?;
        let note = match cd_target(&shell.expand(command)) {
            Some(dir) => {
                std::env::set_current_dir(&dir).with_context(|| format!("cd {}", dir.display()))?;
                let cwd = std::env::current_dir().unwrap_or(dir);
                format!("I changed the working directory to {}.", cwd.display())
            }
            None => {
                let status = on_terminal_status(&shell.script(command)).await?;
                if !status.success() {
                    eprintln!("· {status}");
                }
                format!(
                    "I ran `{command}` on my terminal myself: {status}. Its output went to my \
                     terminal, not to you; run it again if you need it."
                )
            }
        };
        self.messages.push(json!({"role": "user", "content": note}));
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
        let clock = quiet.then(|| Clock(tokio::spawn(elapsed())));
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
    FOREGROUND.store(true, Ordering::Relaxed);
    let status = tokio::process::Command::new("bash")
        .args(["-c", command])
        .status()
        .await;
    FOREGROUND.store(false, Ordering::Relaxed);
    status.context("starting bash")
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
        let mut saved = SAVED.lock().unwrap_or_else(|e| e.into_inner());
        if LENT.load(Ordering::Relaxed) {
            drop(saved);
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        if saved.is_none() {
            *saved = cbreak();
            if saved.is_none() {
                break false;
            }
        }
        let mut fd = libc::pollfd {
            fd: 0,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one pollfd, and it is valid for the call.
        if unsafe { libc::poll(&mut fd, 1, 50) } <= 0 {
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
            drop(saved);
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    restore_terminal(&mut SAVED.lock().unwrap_or_else(|e| e.into_inner()));
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
    LENT.store(true, Ordering::Relaxed);
    restore_terminal(&mut SAVED.lock().unwrap_or_else(|e| e.into_inner()));
    Lent
}

impl Drop for Lent {
    fn drop(&mut self) {
        LENT.store(false, Ordering::Relaxed);
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

/// Ctrl-C at a question the model asked: it stops the model, like Esc.
#[derive(Debug)]
struct Stopped;

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

/// An environment variable, if it is set to something.
fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// DuckDuckGo's results for `query`, read off its HTML page: no key needed,
/// up to 10 results, the first page.
async fn web_search(query: &str, count: u64) -> Result<String> {
    eprintln!("· search {query}");
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
    Ok(search_results(&page, count as usize))
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
            Some(format!("{}\n{url}\n{}", html_text(title), html_text(snippet)))
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

/// The page as markdown from Cloudflare's headless browser when
/// `CLOUDFLARE_ACCOUNT_ID` and a token are set, else fetched and turned into
/// text here, without running its JavaScript.
async fn fetch_url(url: &str) -> Result<String> {
    eprintln!("· fetch {url}");
    if !url.starts_with("http://") && !url.starts_with("https://") {
        bail!("only http and https URLs can be fetched");
    }
    let mut note = String::new();
    if let (Some(account), Some(token)) = (
        env("CLOUDFLARE_ACCOUNT_ID"),
        env("CLOUDFLARE_BROWSER_TOKEN").or_else(|| env("CLOUDFLARE_API_TOKEN")),
    ) {
        match render(url, &account, &token).await {
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
                    alias say='echo '\\''hi'\\'''\n\0declare -f mkcd\n\0mkcd () { :; }\n";
        let shell = parse_user_shell(dump).unwrap();
        assert!(shell.knows("..") && shell.knows("mkcd") && !shell.knows("hi"));
        assert_eq!(shell.aliases["say"], "echo 'hi'");
        assert_eq!(shell.expand(".."), "cd ..");
        assert_eq!(shell.expand("say there"), "echo 'hi' there");
        assert_eq!(shell.expand("ls -l"), "ls -l");
        assert!(!shell.script("..").contains("bashrc says hi"));
        assert!(shell.script("..").ends_with("mkcd () { :; }\n\n.."));
        assert!(parse_user_shell("no definitions").is_none());
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
            waiting: None,
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
