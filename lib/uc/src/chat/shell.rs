// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The commands' bash, and where it leaves the shell for the next one.
//!
//! Each command runs in a bash of its own, and what it changes, a `cd`, an
//! `export`, `z`, `source venv/bin/activate`, an alias, carries over: bash
//! says where it ended up on its way out, and the next one starts there.
//! Nothing goes through a file or the command line, where other users could
//! see it: the definitions and the command go in on a pipe, fd 4, and the
//! state comes back on another, fd 3, which the command itself never has.
//! What comes back is taken whole or not at all: a bash killed, or a command
//! that broke uc's functions, leaves the state as it was.
//!
//! The model's commands carry over the directory and the environment; the
//! user's, typed at the prompt, their shell's variables, options, aliases and
//! functions too, after their prompt's hooks have run (zoxide's, mise's,
//! direnv's) as they would at a prompt. See `shell.bash`.

use anyhow::{Context, Result, bail};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Receiver;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Reads what uc sends, runs the command and says where the shell is.
const RUNNER: &str = concat!(
    include_str!("shell.bash"),
    r#"
IFS= builtin read -r -d '' -u 4 __uc_full
IFS= builtin read -r -d '' -u 4 __uc_cmd
# Before the definitions, which could make `exec` a function. A bare `exec`:
# `builtin exec 4<&-` closes it for the builtin alone, not the shell.
if [[ $__uc_full == [012] ]]; then
  exec {__uc_fd}>&3
  builtin readonly __uc_fd __uc_full
fi
exec 3>&-
builtin source /dev/fd/4 2>/dev/null
exec 4<&-
builtin shopt -s expand_aliases
builtin trap '__uc_save "$?"' EXIT
builtin eval -- "$__uc_cmd"
__uc_save "$?"
"#
);

/// More than this coming back is not a shell's state.
const MAX_STATE: usize = 64 << 20;

/// How much a command's bash says about where it ended up.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Mode {
    /// The model's: the directory and the environment.
    Model,
    /// The user's: their shell's definitions too, after the prompt's hooks.
    Typed,
    /// The user's bash at the start: definitions, completions, history file.
    Read,
    /// Nothing: Tab's completions, which only print.
    Quiet,
}

impl Mode {
    fn flag(self) -> &'static str {
        match self {
            Mode::Model => "0",
            Mode::Typed => "1",
            Mode::Read => "2",
            Mode::Quiet => "x",
        }
    }
}

/// The user's shell's own definitions, as bash printed them.
#[derive(Clone, Default, Debug, PartialEq)]
pub(super) struct Definitions {
    /// `shopt -s` and `-u` lines.
    pub options: String,
    /// `alias -p`.
    pub aliases: String,
    /// `declare -p` lines for the variables it doesn't export.
    pub vars: String,
    /// The functions' names, a line each.
    pub names: String,
    /// `declare -f`.
    pub functions: String,
}

impl Definitions {
    /// A bash script that defines them all again.
    pub fn script(&self) -> String {
        // The options first: bash-completion's functions need extglob to parse.
        format!(
            "{}\nshopt -s expand_aliases\n{}\n{}\n{}\n",
            self.options, self.aliases, self.vars, self.functions
        )
    }
}

/// Where a bash ended up.
#[derive(Default, Debug, PartialEq)]
pub(super) struct State {
    pub pwd: PathBuf,
    /// Its environment, but `_` and `SHLVL`; None when it didn't say.
    pub env: Option<Vec<(OsString, OsString)>>,
    pub definitions: Option<Definitions>,
    pub completions: Option<String>,
    pub histfile: Option<String>,
}

/// The environment the commands start with: uc's own until a command changes it.
static ENV: Mutex<Option<Vec<(OsString, OsString)>>> = Mutex::new(None);

/// bash itself, found on uc's own PATH once: the commands' may lose it.
fn bash() -> &'static Path {
    static BASH: OnceLock<PathBuf> = OnceLock::new();
    BASH.get_or_init(|| {
        std::env::var_os("PATH")
            .and_then(|path| {
                std::env::split_paths(&path)
                    .map(|dir| dir.join("bash"))
                    .find(|bash| bash.is_file())
            })
            .unwrap_or_else(|| PathBuf::from("/bin/bash"))
    })
}

/// A pipe, both ends close-on-exec and numbered 10 and up, out of the way of
/// the 3 and 4 they become in bash: (read, write).
fn pipe() -> Result<(OwnedFd, OwnedFd)> {
    let mut fds = [-1; 2];
    // SAFETY: pipe2 writes the two descriptors it opens into fds.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(std::io::Error::last_os_error()).context("opening a pipe");
    }
    // SAFETY: both were just opened and nothing else owns them.
    let (r, w) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    let high = |fd: OwnedFd| -> Result<OwnedFd> {
        // SAFETY: F_DUPFD_CLOEXEC opens a new descriptor, which we own.
        let new = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 10) };
        if new < 0 {
            return Err(std::io::Error::last_os_error()).context("moving a pipe");
        }
        // SAFETY: just opened above.
        Ok(unsafe { OwnedFd::from_raw_fd(new) })
    };
    Ok((high(r)?, high(w)?))
}

/// A bash that runs a command, before it is started.
pub(super) struct Bash {
    pub command: Command,
    input: Vec<u8>,
    /// bash's ends, kept open until it has started.
    theirs: Vec<OwnedFd>,
    /// uc's ends: where the input goes, where the state comes from.
    to: OwnedFd,
    from: Option<OwnedFd>,
}

/// A bash that was started: its state, once it has finished.
pub(super) struct Started(Option<Receiver<Vec<u8>>>);

/// A bash handed over as tokio's command, waiting to be told it started.
pub(super) struct Pending(Bash);

impl Pending {
    pub fn started(self) -> Started {
        self.0.started()
    }
}

impl Bash {
    /// `command` in bash, with `definitions` defined first and the commands'
    /// environment; `args` are its `$1` and on. Start it with `command`'s
    /// spawn, std's or tokio's, then call [`Bash::started`].
    pub fn new(mode: Mode, command: &str, definitions: &str, args: &[&str]) -> Result<Self> {
        if command.contains('\0') {
            bail!("the command has a NUL in it");
        }
        let (their_in, to) = pipe()?;
        let (from, their_out) = match mode {
            Mode::Quiet => (None, None),
            _ => {
                let (r, w) = pipe()?;
                (Some(r), Some(w))
            }
        };
        let mut bash = Command::new(bash());
        if mode == Mode::Read {
            bash.arg("-i");
        }
        bash.arg("-c").arg(RUNNER).arg("uc").args(args);
        if let Some(env) = &*ENV.lock().unwrap_or_else(|e| e.into_inner()) {
            bash.env_clear().envs(env.iter().map(|(k, v)| (k, v)));
            if let Some(level) = std::env::var_os("SHLVL") {
                bash.env("SHLVL", level);
            }
        }
        let (fd4, fd3) = (
            their_in.as_raw_fd(),
            their_out.as_ref().map(|w| w.as_raw_fd()),
        );
        // SAFETY: dup2 is async-signal-safe; it makes the pipes bash's 4 and
        // 3, without close-on-exec, and touches no memory.
        unsafe {
            bash.pre_exec(move || {
                if libc::dup2(fd4, 4) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                if let Some(fd3) = fd3
                    && libc::dup2(fd3, 3) == -1
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut input = Vec::with_capacity(definitions.len() + command.len() + 4);
        for part in [mode.flag(), command] {
            input.extend_from_slice(part.as_bytes());
            input.push(0);
        }
        input.extend_from_slice(definitions.as_bytes());
        Ok(Bash {
            command: bash,
            input,
            theirs: [Some(their_in), their_out].into_iter().flatten().collect(),
            to,
            from,
        })
    }

    /// The command as tokio's, and the rest, for [`Bash::started`].
    pub fn into_tokio(self) -> (tokio::process::Command, Pending) {
        let Bash {
            command,
            input,
            theirs,
            to,
            from,
        } = self;
        (
            command.into(),
            Pending(Bash {
                command: Command::new("true"),
                input,
                theirs,
                to,
                from,
            }),
        )
    }

    /// Once bash has started (or failed to): sends it the input and reads
    /// its state, on threads of their own, so a full pipe never waits on uc.
    pub fn started(self) -> Started {
        drop(self.theirs);
        let (mut to, input) = (std::fs::File::from(self.to), self.input);
        std::thread::spawn(move || drop(to.write_all(&input)));
        Started(self.from.map(|from| {
            let (tx, rx) = std::sync::mpsc::channel();
            // Until the state is whole: something the command left running
            // may hold the pipe open long after bash is gone.
            std::thread::spawn(move || {
                let mut from = std::fs::File::from(from);
                let (mut out, mut buf) = (Vec::new(), vec![0; 64 << 10]);
                while let Ok(n @ 1..) = from.read(&mut buf) {
                    out.extend_from_slice(&buf[..n]);
                    if out.len() > MAX_STATE {
                        return;
                    }
                    if out.ends_with(b"end\0") && parse(&out).is_ok() {
                        break;
                    }
                }
                let _ = tx.send(out);
            });
            rx
        }))
    }
}

impl Started {
    /// Where the shell ended up, when bash said so in full; bash has exited.
    pub fn state(self) -> Option<State> {
        let out = self.0?.recv_timeout(Duration::from_secs(2)).ok()?;
        parse(&out).ok()
    }
}

/// What bash wrote to fd 3, see `__uc_save`.
pub(super) fn parse(out: &[u8]) -> Result<State> {
    let mut parts = out.split(|&b| b == 0);
    if parts.next() != Some(b"uc-state") {
        bail!("not a shell's state");
    }
    let text = |v: &[u8]| String::from_utf8_lossy(v).into_owned();
    let mut state = State::default();
    let mut env = Vec::new();
    let mut definitions = Definitions::default();
    let (mut pwd, mut full, mut ended) = (false, false, false);
    while let Some(key) = parts.next() {
        if key == b"end" {
            ended = true;
            break;
        }
        let value = parts.next().context("a key with no value")?;
        match key {
            b"pwd" => {
                state.pwd = PathBuf::from(OsString::from_vec(value.to_vec()));
                pwd = true;
            }
            b"env" => {
                let eq = value
                    .iter()
                    .position(|&b| b == b'=')
                    .context("env with no =")?;
                env.push((
                    OsString::from_vec(value[..eq].to_vec()),
                    OsString::from_vec(value[eq + 1..].to_vec()),
                ));
            }
            b"options" => definitions.options = text(value),
            b"aliases" => definitions.aliases = text(value),
            b"vars" => definitions.vars = text(value),
            b"names" => definitions.names = text(value),
            b"functions" => {
                definitions.functions = text(value);
                full = true;
            }
            b"completions" => state.completions = Some(text(value)),
            b"histfile" => state.histfile = Some(text(value)).filter(|h| !h.is_empty()),
            other => bail!("unknown key {}", String::from_utf8_lossy(other)),
        }
    }
    // Cut short or not from uc's functions: none of it.
    if !ended || !pwd || !state.pwd.is_absolute() {
        bail!("the state was cut short");
    }
    state.env = Some(env).filter(|env| !env.is_empty());
    state.definitions = full.then_some(definitions);
    Ok(state)
}

/// Takes on where a bash ended up: its directory, which uc and every command
/// after it works in, and its environment. The directory when it changed.
pub(super) fn take(state: &State) -> Option<PathBuf> {
    if let Some(env) = &state.env {
        *ENV.lock().unwrap_or_else(|e| e.into_inner()) = Some(env.clone());
    }
    let before = std::env::current_dir().ok();
    if before.as_deref() == Some(state.pwd.as_path()) {
        return None;
    }
    match std::env::set_current_dir(&state.pwd) {
        Ok(()) => Some(state.pwd.clone()),
        Err(err) => {
            super::out::note(&format!("cd {}: {err}", state.pwd.display()));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Runs `command` as `mode` with `definitions`, its output and state.
    fn run(mode: Mode, command: &str, definitions: &str) -> (String, Option<State>) {
        let mut bash = Bash::new(mode, command, definitions, &[]).unwrap();
        bash.command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let child = bash.command.spawn();
        let started = bash.started();
        let output = child.unwrap().wait_with_output().unwrap();
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        (
            String::from_utf8_lossy(&output.stdout).into_owned(),
            started.state(),
        )
    }

    #[test]
    fn the_state_comes_back_on_its_own_pipe() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap();
        let command = format!(
            "cd '{}'; export UC_TEST_A='a b\nc'; f() {{ echo f; }}; export -f f; \
             for fd in /proc/$BASHPID/fd/*; do echo ${{fd##*/}} $(readlink $fd); done; exit 3",
            path.display()
        );
        let (out, state) = run(Mode::Model, &command, "");
        // Neither pipe is where a script would use it: the input is closed,
        // the state's is out of the way, past 9. A 3 is readlink's own `$()`.
        let fds: BTreeMap<&str, &str> = out.lines().filter_map(|l| l.split_once(' ')).collect();
        assert!(!fds.contains_key("4"), "{out:?}");
        assert!(
            fds.contains_key("10") && fds.get("3") != fds.get("10"),
            "{out:?}"
        );
        let state = state.unwrap();
        assert_eq!(state.pwd, path);
        let env = state.env.unwrap();
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(get("UC_TEST_A"), Some("a b\nc".into()));
        assert_eq!(get("BASH_FUNC_f%%"), Some("() { \n    echo f\n}".into()));
        assert!(get("SHLVL").is_none() && get("_").is_none());
        assert!(state.definitions.is_none());
    }

    #[test]
    fn typed_commands_keep_the_shells_definitions() {
        let defs = Definitions {
            aliases: "alias hi='echo hi'".into(),
            vars: "declare -a x=([0]=\"1\")".into(),
            ..Default::default()
        };
        let (out, state) = run(
            Mode::Typed,
            "hi there; y=2; unalias hi; alias k=kubectl; g() { :; }",
            &defs.script(),
        );
        assert_eq!(out, "hi there\n");
        let defs = state.unwrap().definitions.unwrap();
        assert_eq!(defs.aliases, "alias k='kubectl'");
        assert!(
            defs.vars.contains("declare -a x=([0]=\"1\")"),
            "{}",
            defs.vars
        );
        assert!(defs.vars.contains("declare -- y=\"2\""), "{}", defs.vars);
        assert!(!defs.vars.contains("__uc"), "{}", defs.vars);
        assert_eq!(defs.names.trim(), "g");
        assert!(defs.options.contains("shopt -s expand_aliases"));
    }

    #[test]
    fn the_prompts_hooks_run_after_a_typed_command() {
        let defs = Definitions {
            vars: "declare -- PROMPT_COMMAND=\"hook\"".into(),
            functions: "hook() { local s=$?; [[ -o history ]] && export HOOKED=$s; }".into(),
            ..Default::default()
        };
        let (_, state) = run(Mode::Typed, "false", &defs.script());
        let env = state.unwrap().env.unwrap();
        assert!(
            env.iter().any(|(k, v)| k == "HOOKED" && v == "1"),
            "{env:?}"
        );
        // Not after the model's.
        let (_, state) = run(Mode::Model, "false", &defs.script());
        assert!(
            !state
                .unwrap()
                .env
                .unwrap()
                .iter()
                .any(|(k, _)| k == "HOOKED")
        );
    }

    #[test]
    fn a_broken_or_killed_shell_says_nothing() {
        let (_, state) = run(Mode::Model, "kill -9 $$", "");
        assert!(state.is_none());
        let (_, state) = run(Mode::Model, "unset -f __uc_save; exit", "");
        assert!(state.is_none());
        let (_, state) = run(Mode::Model, "__uc_fd=1", "");
        assert!(state.is_some(), "the pipe's number can't be changed");
        let (out, state) = run(Mode::Model, "echo still runs", "f() {");
        assert_eq!(out, "still runs\n");
        assert!(state.is_some());
        assert!(parse(b"uc-state\0pwd\0/x\0").is_err());
        assert!(parse(b"uc-state\0pwd\0x\0end\0").is_err());
        assert!(parse(b"junk\0pwd\0/x\0end\0").is_err());
        assert!(parse(b"uc-state\0pwd\0/x\0end\0").is_ok());
    }

    #[test]
    fn exit_and_ctrl_c_still_say_where_it_is() {
        let (_, state) = run(Mode::Model, "cd /; exit 4", "");
        assert_eq!(state.unwrap().pwd, PathBuf::from("/"));
        let (_, state) = run(Mode::Model, "cd /; kill -INT $$; sleep 1", "");
        assert_eq!(state.unwrap().pwd, PathBuf::from("/"));
    }
}
