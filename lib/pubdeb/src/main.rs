// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Publish ./dist/*.deb to the apt repository: sync it from `shadow`, add the
//! packages with reprepro (trixie), sync it back and push it to the archive
//! with rclone. Stops at the first failing step (set -euo pipefail).

use std::env;
use std::fs;
use std::io;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, exit};

fn required(key: &str) -> String {
    env::var(key).unwrap_or_else(|_| {
        eprintln!("pubdeb: {key}: unbound variable");
        exit(1);
    })
}

fn step(prog: &str, args: &[String]) {
    let rc = match Command::new(prog).args(args).status() {
        Ok(s) => s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            eprintln!("pubdeb: {prog}: command not found");
            127
        }
        Err(e) => {
            eprintln!("pubdeb: {prog}: {e}");
            126
        }
    };
    if rc != 0 {
        exit(rc);
    }
}

/// Expand `./dist/*.deb` like bash: sorted matches (no dotfiles), or the
/// literal pattern when nothing matches.
fn debs(names: impl Iterator<Item = String>) -> Vec<String> {
    let mut v: Vec<String> = names
        .filter(|n| !n.starts_with('.') && n.ends_with(".deb"))
        .map(|n| format!("./dist/{n}"))
        .collect();
    v.sort();
    if v.is_empty() {
        v.push("./dist/*.deb".into());
    }
    v
}

fn main() {
    let home = required("HOME");
    let repo = format!("{home}/.cache/blueprint/apt/");
    let remote = format!("shadow:{repo}");
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();

    step("rsync", &s(&["-avP", "--delete", &remote, &repo]));

    let names = fs::read_dir("./dist")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok());
    let mut include = s(&["-b", &repo, "includedeb", "trixie"]);
    include.extend(debs(names));
    step("reprepro", &include);
    step("reprepro", &s(&["-b", &repo, "export"]));

    step("rsync", &s(&["-avP", "--delete", &repo, &remote]));

    let user = required("USER");
    let err = Command::new("ssh")
        .args(["shadow", &format!("{home}/.local/bin/rclone"), "-v", "sync"])
        .arg(&repo)
        .arg(format!("archive:{user}/debian/"))
        .exec();
    eprintln!("pubdeb: ssh: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}

#[cfg(test)]
mod tests {
    use super::debs;

    #[test]
    fn glob_expansion() {
        let names = ["b.deb", ".h.deb", "a.deb", "x.txt"].map(String::from);
        assert_eq!(debs(names.into_iter()), ["./dist/a.deb", "./dist/b.deb"]);
        assert_eq!(debs(std::iter::empty()), ["./dist/*.deb"]);
    }
}
