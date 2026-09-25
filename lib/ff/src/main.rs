// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Fuzzy-pick files (multi-select) under a directory (default: $HOME).

use std::env;
use std::ffi::OsString;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio, exit};

fn root(arg: Option<OsString>, home: Option<OsString>) -> OsString {
    match arg {
        Some(a) if !a.is_empty() => a,
        _ => home.unwrap_or_default(),
    }
}

fn main() {
    let dir = root(env::args_os().nth(1), env::var_os("HOME"));
    let mut fzf = Command::new("fzf");
    fzf.arg("-m");
    match Command::new("fd")
        .args(["-I", "--hidden", "--exclude", ".git", "."])
        .arg(&dir)
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            if let Some(out) = child.stdout.take() {
                fzf.stdin(out);
            }
        }
        Err(e) => {
            eprintln!("ff: fd: {e}");
            fzf.stdin(Stdio::null());
        }
    }
    let err = fzf.exec();
    eprintln!("ff: fzf: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}

#[cfg(test)]
mod tests {
    use super::root;

    #[test]
    fn defaults_to_home() {
        assert_eq!(root(None, Some("/h".into())), "/h");
        assert_eq!(root(Some("".into()), Some("/h".into())), "/h");
        assert_eq!(root(Some("/x".into()), Some("/h".into())), "/x");
    }
}
