// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc` with no command: a coding agent on the terminal.
//!
//! Each line typed goes to a model on OpenRouter, which answers or asks to
//! run a shell command, read, write or edit a file in the current directory.
//! Commands, writes and edits are shown and run straight away; with `--ask`
//! they are confirmed first (`a` allows the rest of the session, a line ending
//! in `-y` allows what its answer does). The key comes from
//! `OPENROUTER_API_KEY`, the model from `UC_MODEL`, the reasoning effort from
//! `UC_EFFORT`.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::process::{ExitCode, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

const API: &str = "https://openrouter.ai/api/v1/chat/completions";
const DEFAULT_MODEL: &str = "deepseek/deepseek-v4.1-flash";
const DEFAULT_EFFORT: &str = "high";
/// Tool output beyond this is cut, so one noisy command can't fill the context.
const MAX_OUTPUT: usize = 30_000;

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

/// `ask` confirms every command and change instead of running it straight away.
pub fn main(ask: bool) -> ExitCode {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    match runtime.block_on(run(ask)) {
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
}

async fn run(ask: bool) -> Result<()> {
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
        messages: vec![json!({"role": "system", "content": system_prompt(&model, ask)})],
        model,
        effort,
        allow_all: !ask,
        allow_turn: false,
    };
    loop {
        let Some(line) = read_line("\x1b[38;5;208m₿\x1b[0m ")? else {
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

/// `ls -y` is `ls`, approved up front.
fn split_approval(line: &str) -> (&str, bool) {
    match line.strip_suffix("-y") {
        Some(rest) if rest.is_empty() || rest.ends_with(char::is_whitespace) => {
            (rest.trim_end(), true)
        }
        _ => (line, false),
    }
}

fn system_prompt(model: &str, ask: bool) -> String {
    let cwd = std::env::current_dir()
        .map(|d| d.display().to_string())
        .unwrap_or_default();
    let confirms = if ask {
        "the user confirms commands and changes"
    } else {
        "they run without asking, so be careful with anything destructive"
    };
    format!(
        "You are uc, the usecode coding agent, running in a terminal on the user's machine, \
         on the model {model} via OpenRouter. The working directory is {cwd}. Use the tools to \
         look around, run commands and change files; {confirms}. \
         The `uc` command line tool is installed (run `uc -h` to see its commands). The user \
         sees every command's output as it runs, so never repeat or reformat it; when a command \
         was all they asked for, say nothing after it. Otherwise be brief: your replies are \
         printed as plain text in a terminal, so no markdown."
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
    json!([
        tool(
            "bash",
            "Run a command with bash in the working directory; returns its exit status, stdout and stderr.",
            json!({"command": string("the command line")}),
            &["command"],
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
        let result = match name {
            "bash" => self.bash(&arg("command")).await,
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

    async fn bash(&mut self, command: &str) -> Result<String> {
        // The gray ₿ is the shell prompt from the dotfiles' .bashrc.
        self.confirm(&format!("\x1b[38;5;245m₿\x1b[0m {command}\nrun it?"))?;
        let output = tokio::process::Command::new("bash")
            .args(["-c", command])
            .stdin(Stdio::null())
            .output()
            .await
            .context("starting bash")?;
        let (stdout, stderr) = (
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        eprint!("{stdout}{stderr}");
        Ok(format!(
            "{}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            output.status
        ))
    }

    fn write_file(&mut self, path: &str, content: &str) -> Result<String> {
        eprintln!("· write {path} ({} lines)", content.lines().count());
        self.confirm("write it?")?;
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
        self.confirm("apply it?")?;
        std::fs::write(path, text.replacen(old, new, 1)).context("writing the file")?;
        Ok(format!("edited {path}"))
    }

    /// Ok to go ahead; an error tells the model the user said no.
    fn confirm(&mut self, question: &str) -> Result<()> {
        if self.allow_all || self.allow_turn {
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

/// A line from stdin after `prompt`; None at end of input.
fn read_line(prompt: &str) -> Result<Option<String>> {
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let mut line = String::new();
    let read = std::io::stdin()
        .lock()
        .read_line(&mut line)
        .context("reading stdin")?;
    Ok((read > 0).then_some(line))
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
    fn a_trailing_y_approves() {
        assert_eq!(split_approval("ls -y"), ("ls", true));
        assert_eq!(split_approval("-y"), ("", true));
        assert_eq!(split_approval("ls"), ("ls", false));
        assert_eq!(split_approval("apt-y"), ("apt-y", false));
    }

    #[test]
    fn every_tool_has_a_name_and_parameters() {
        for tool in tools().as_array().unwrap() {
            assert!(tool["function"]["name"].is_string());
            assert_eq!(tool["function"]["parameters"]["type"], "object");
        }
    }
}
