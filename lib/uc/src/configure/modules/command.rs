// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Ansible's `command` and `shell` modules: run a program, optionally through
//! sudo, with `creates`/`removes` guards and accepted exit codes.

use crate::configure::ctx::Ctx;
use crate::configure::engine::Outcome;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::Command;

/// What a finished command printed.
#[derive(Debug, Default, Clone)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == 0
    }

    /// stdout and stderr together, for tools that report on either.
    pub fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

pub struct Cmd {
    ctx: Ctx,
    program: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    cwd: Option<PathBuf>,
    sudo: bool,
    creates: Option<PathBuf>,
    removes: Option<PathBuf>,
    ok_codes: Option<Vec<i32>>,
    read_only: bool,
    stdin: Option<Vec<u8>>,
}

impl Cmd {
    pub fn new(ctx: Ctx, program: String) -> Cmd {
        Cmd {
            ctx,
            program,
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            sudo: false,
            creates: None,
            removes: None,
            ok_codes: Some(vec![0]),
            read_only: false,
            stdin: None,
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Cmd {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Cmd
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Cmd {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn cwd(mut self, dir: impl AsRef<Path>) -> Cmd {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Ansible's `become: true`.
    pub fn sudo(mut self) -> Cmd {
        self.sudo = true;
        self
    }

    pub fn sudo_if(self, sudo: bool) -> Cmd {
        if sudo { self.sudo() } else { self }
    }

    /// Skip the command when this path exists. A `*` in the last component
    /// matches like a shell glob.
    pub fn creates(mut self, path: impl AsRef<Path>) -> Cmd {
        self.creates = Some(path.as_ref().to_path_buf());
        self
    }

    /// Skip the command unless this path exists.
    pub fn removes(mut self, path: impl AsRef<Path>) -> Cmd {
        self.removes = Some(path.as_ref().to_path_buf());
        self
    }

    /// Exit codes that do not fail the task.
    pub fn ok_codes(mut self, codes: &[i32]) -> Cmd {
        self.ok_codes = Some(codes.to_vec());
        self
    }

    /// Any exit code is accepted; the caller inspects [`Output::code`].
    pub fn any_code(mut self) -> Cmd {
        self.ok_codes = None;
        self
    }

    /// The command only inspects the machine, so it also runs under `--check`.
    pub fn read_only(mut self) -> Cmd {
        self.read_only = true;
        self
    }

    pub fn stdin(mut self, input: impl Into<Vec<u8>>) -> Cmd {
        self.stdin = Some(input.into());
        self
    }

    /// Runs the command as a task step: `Ok` when a guard skips it,
    /// `Changed` when it ran.
    pub async fn run_step(self) -> Result<Outcome> {
        if let Some(path) = &self.creates
            && path_exists(path)
        {
            return Ok(Outcome::Ok);
        }
        if let Some(path) = &self.removes
            && !path_exists(path)
        {
            return Ok(Outcome::Ok);
        }
        if self.ctx.check() && !self.read_only {
            return Ok(Outcome::Changed);
        }
        self.output().await?;
        Ok(Outcome::Changed)
    }

    /// Runs the command and returns what it printed. Under `--check` a
    /// command that is not read-only is not run and reports success.
    pub async fn output(self) -> Result<Output> {
        if self.ctx.check() && !self.read_only {
            return Ok(Output::default());
        }
        let shown = self.display();
        let mut command = self.build();
        let mut child = command
            .spawn()
            .with_context(|| format!("starting `{shown}`"))?;

        if let Some(input) = self.stdin
            && let Some(mut pipe) = child.stdin.take()
        {
            pipe.write_all(&input).await?;
        }
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let (stdout, stderr, status) = tokio::join!(
            collect(&self.ctx, stdout),
            collect(&self.ctx, stderr),
            child.wait()
        );
        let status = status.with_context(|| format!("waiting for `{shown}`"))?;
        let out = Output {
            // A signal leaves no code; report it as the shell would.
            code: status.code().unwrap_or(128),
            stdout,
            stderr,
        };
        if let Some(codes) = &self.ok_codes
            && !codes.contains(&out.code)
        {
            let mut tail = out.stderr.trim_end().to_string();
            if tail.is_empty() {
                tail = out.stdout.trim_end().to_string();
            }
            bail!("`{shown}` exited with {}\n{tail}", out.code);
        }
        Ok(out)
    }

    fn build(&self) -> Command {
        let vars = self.ctx.vars();
        let mut env: Vec<(String, String)> = vec![("PATH".into(), vars.path.clone())];
        env.extend(self.env.iter().cloned());

        let mut command = if self.sudo && !self.ctx.is_root() {
            // sudo resets the environment, so it is passed through env(1).
            let mut c = Command::new("sudo");
            c.arg("-n").arg("env");
            for (k, v) in vars.proxy.iter().chain(env.iter()) {
                c.arg(format!("{k}={v}"));
            }
            c.arg(&self.program);
            c
        } else {
            let mut c = Command::new(&self.program);
            c.envs(env);
            c
        };
        command
            .args(&self.args)
            .stdin(if self.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(dir) = &self.cwd {
            command.current_dir(dir);
        }
        command
    }

    fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.sudo && !self.ctx.is_root() {
            parts.push("sudo".to_string());
        }
        parts.push(self.program.clone());
        parts.extend(self.args.iter().map(|a| {
            if a.contains(char::is_whitespace) {
                let short: String = a.chars().take(60).collect();
                let more = if a.chars().count() > 60 { "…" } else { "" };
                format!("'{short}{more}'")
            } else {
                a.clone()
            }
        }));
        parts.join(" ")
    }
}

/// Reads a pipe to the end, echoing each line to the task log.
async fn collect(ctx: &Ctx, pipe: impl AsyncRead + Unpin) -> String {
    let mut lines = BufReader::new(pipe).lines();
    let mut all = String::new();
    while let Ok(Some(line)) = lines.next_line().await {
        ctx.log(&line);
        all.push_str(&line);
        all.push('\n');
    }
    all
}

/// Whether `path` exists; a `*` or `?` in its last component is a glob.
pub fn path_exists(path: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    match name {
        Some(name) if name.contains(['*', '?']) => {
            let dir = path.parent().unwrap_or(Path::new("."));
            std::fs::read_dir(dir).is_ok_and(|entries| {
                entries
                    .flatten()
                    .any(|e| crate::configure::glob_match(&name, &e.file_name().to_string_lossy()))
            })
        }
        _ => path.symlink_metadata().is_ok(),
    }
}

/// Whether `program` can be found on the run's `PATH`.
pub fn which(ctx: &Ctx, program: &str) -> Option<PathBuf> {
    std::env::split_paths(&ctx.vars().path)
        .map(|dir| dir.join(program))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure::ctx::tests::ctx;

    #[tokio::test]
    async fn captures_output_and_codes() {
        let out = ctx(false)
            .shell("echo out; echo err >&2; exit 3")
            .any_code()
            .output()
            .await
            .unwrap();
        assert_eq!(out.code, 3);
        assert_eq!(out.stdout, "out\n");
        assert_eq!(out.stderr, "err\n");
    }

    #[tokio::test]
    async fn fails_on_unexpected_codes_with_the_output() {
        let err = ctx(false)
            .shell("echo nope >&2; exit 2")
            .output()
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("exited with 2") && msg.contains("nope"),
            "{msg}"
        );
    }

    #[tokio::test]
    async fn creates_guard_skips() {
        let outcome = ctx(false)
            .shell("exit 1")
            .creates("/")
            .run_step()
            .await
            .unwrap();
        assert_eq!(outcome, Outcome::Ok);
    }

    #[tokio::test]
    async fn check_mode_does_not_run() {
        let marker = std::env::temp_dir().join(format!("uc-check-{}", std::process::id()));
        let script = format!("touch {}", marker.display());
        let outcome = ctx(true).shell(script).run_step().await.unwrap();
        assert_eq!(outcome, Outcome::Changed);
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn feeds_stdin() {
        let out = ctx(false)
            .cmd("cat")
            .stdin("hi")
            .read_only()
            .output()
            .await
            .unwrap();
        assert_eq!(out.stdout, "hi\n");
    }

    #[test]
    fn globs_in_creates() {
        assert!(path_exists(Path::new("/etc/host*")));
        assert!(!path_exists(Path::new("/etc/definitely-not-*-here")));
    }
}
