// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! List git repositories under a root directory (default `.`) that need
//! attention: dirty working tree, `main` not tracking or behind `origin/main`,
//! or the current branch not tracking a remote or behind it.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio, exit};

/// Runs `git -C repo <args>` and returns (success, stdout without trailing
/// newlines). Stderr is discarded.
fn git(repo: &str, args: &[&str]) -> (bool, String) {
    match Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        Ok(o) => (
            o.status.success(),
            String::from_utf8_lossy(&o.stdout)
                .trim_end_matches('\n')
                .to_string(),
        ),
        Err(_) => (false, String::new()),
    }
}

fn git_out(repo: &str, args: &[&str]) -> String {
    git(repo, args).1
}

fn ref_exists(repo: &str, r: &str) -> bool {
    git(repo, &["show-ref", "--verify", "--quiet", r]).0
}

/// True when `git rev-list --count <range>` reports more than zero commits.
fn is_behind(repo: &str, range: &str) -> bool {
    git_out(repo, &["rev-list", "--count", range])
        .trim()
        .parse::<u64>()
        .is_ok_and(|n| n > 0)
}

fn main_needs_action(repo: &str) -> bool {
    let tracking = git_out(repo, &["config", "--get", "branch.main.remote"]);
    let merge = git_out(repo, &["config", "--get", "branch.main.merge"]);
    if tracking != "origin" || merge != "refs/heads/main" {
        // main doesn't track origin/main (or main doesn't exist: skip)
        return ref_exists(repo, "refs/heads/main");
    }
    ref_exists(repo, "refs/remotes/origin/main") && is_behind(repo, "main..origin/main")
}

fn current_needs_action(repo: &str) -> bool {
    let current = git_out(repo, &["symbolic-ref", "--short", "HEAD"]);
    if current.is_empty() {
        return false;
    }
    let remote = git_out(
        repo,
        &["config", "--get", &format!("branch.{current}.remote")],
    );
    let merge = git_out(
        repo,
        &["config", "--get", &format!("branch.{current}.merge")],
    );
    if remote.is_empty() || merge.is_empty() {
        // Current branch doesn't track any remote
        return true;
    }
    let branch = merge.strip_prefix("refs/heads/").unwrap_or(&merge);
    ref_exists(repo, &format!("refs/remotes/{remote}/{branch}"))
        && is_behind(repo, &format!("{current}..{remote}/{branch}"))
}

fn needs_action(repo: &str) -> bool {
    !git_out(repo, &["status", "--porcelain"]).is_empty()
        || main_needs_action(repo)
        || current_needs_action(repo)
}

fn main() {
    let root = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());

    let mut find = Command::new("find")
        .arg(&root)
        .args(["-name", ".git", "-type", "d"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("git_repos: find: {e}");
            exit(127);
        });

    let stdout = find.stdout.take().expect("piped stdout");
    for gitdir in BufReader::new(stdout).lines().map_while(Result::ok) {
        let repo = gitdir.strip_suffix("/.git").unwrap_or(&gitdir);
        if needs_action(repo) {
            println!("{repo}");
        }
    }

    // pipefail: a failing find makes the whole script fail.
    let status = find.wait().map_or(1, |s| s.code().unwrap_or(1));
    exit(status);
}
