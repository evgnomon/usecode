// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Small helpers for running external commands.

use std::env;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

/// Equivalent of `command -v name` for executables on PATH.
pub fn has_cmd(name: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|dir| {
        std::fs::metadata(dir.join(name))
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

/// Run a command with inherited stdio. Returns `Err(code)` on failure, so
/// callers can mimic `set -e`.
pub fn run(prog: &str, args: &[&str]) -> Result<(), i32> {
    match Command::new(prog).args(args).status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(s.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("{prog}: {e}");
            Err(127)
        }
    }
}

/// Run a command with stderr discarded, returning (success, stdout).
pub fn capture(prog: &str, args: &[&str], stdin: Option<&str>) -> (bool, String) {
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return (false, String::new());
    };
    if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let _ = pipe.write_all(input.as_bytes());
    }
    match child.wait_with_output() {
        Ok(out) => (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        ),
        Err(_) => (false, String::new()),
    }
}

/// Like `$(cmd)`: stdout with trailing newlines removed.
pub fn trim_nl(s: &str) -> &str {
    s.trim_end_matches('\n')
}

/// `stat -c '%y' file | cut -d. -f1`
pub fn modified(file: &Path) -> String {
    let (_, out) = capture("stat", &["-c", "%y", &file.to_string_lossy()], None);
    let out = trim_nl(&out);
    out.split('.').next().unwrap_or("").to_string()
}
