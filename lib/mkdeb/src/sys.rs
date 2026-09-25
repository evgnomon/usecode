// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Process and path helpers mirroring the shell semantics of the original.

use std::env;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::process::{self, Command, ExitStatus, Stdio};

fn exit_like(status: ExitStatus) -> ! {
    process::exit(status.code().unwrap_or(1))
}

fn spawn_failed(cmd: &Command, e: std::io::Error) -> ! {
    eprintln!("mkdeb: {}: {e}", cmd.get_program().to_string_lossy());
    process::exit(127)
}

/// Run a command with inherited stdio; exit with its status on failure
/// (like `set -e`).
pub fn run(cmd: &mut Command) {
    let status = cmd.status().unwrap_or_else(|e| spawn_failed(cmd, e));
    if !status.success() {
        exit_like(status);
    }
}

/// Run a command capturing stdout; exit with its status on failure.
pub fn output(cmd: &mut Command) -> Vec<u8> {
    let out = cmd
        .stdout(Stdio::piped())
        .output()
        .unwrap_or_else(|e| spawn_failed(cmd, e));
    if !out.status.success() {
        exit_like(out.status);
    }
    out.stdout
}

/// Like `$(cmd)`: stdout as a string with trailing newlines removed.
pub fn capture(cmd: &mut Command) -> String {
    let out = output(cmd);
    String::from_utf8_lossy(&out)
        .trim_end_matches('\n')
        .to_string()
}

pub fn is_executable(p: &Path) -> bool {
    p.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Equivalent of `command -v name` for a plain program name.
pub fn in_path(name: &str) -> bool {
    env::var_os("PATH")
        .map(|p| env::split_paths(&p).any(|d| is_executable(&d.join(name))))
        .unwrap_or(false)
}

/// Lexically resolve `.` and `..` in an absolute path, like bash's logical `cd`.
pub fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::from("/");
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(s) => out.push(s),
            _ => {}
        }
    }
    out
}

/// Equivalent of `$(cd "$dir" && pwd)`: logical absolute path of `dir`.
pub fn logical_abs(dir: &str) -> PathBuf {
    let base = env::var_os("PWD")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && same_dir(p, Path::new(".")))
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"));
    let logical = normalize(&base.join(dir));
    match logical.is_dir() {
        true => logical,
        false => match Path::new(dir).canonicalize() {
            Ok(p) if p.is_dir() => p,
            _ => crate::log::die(&format!("cd: {dir}: No such file or directory")),
        },
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (a.metadata(), b.metadata()) {
        (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
        _ => false,
    }
}

/// `dirname` for the paths mkdeb deals with.
pub fn dirname(p: &Path) -> PathBuf {
    match p.parent() {
        Some(d) if d.as_os_str().is_empty() => PathBuf::from("."),
        Some(d) => d.to_path_buf(),
        None => PathBuf::from("/"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_lexically() {
        assert_eq!(
            normalize(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
        assert_eq!(normalize(Path::new("/..")), PathBuf::from("/"));
    }

    #[test]
    fn dirname_like_shell() {
        assert_eq!(dirname(Path::new("/tmp/tmp.X")), PathBuf::from("/tmp"));
        assert_eq!(dirname(Path::new("x")), PathBuf::from("."));
        assert_eq!(dirname(Path::new("/")), PathBuf::from("/"));
    }
}
