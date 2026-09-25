// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Configure git identity and signing for a repo using the blueprint config
//! at `$HOME/src/github.com/$USER/config/config.yaml` (read with `yq`).

use std::env;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio, exit};

fn usage() -> ! {
    println!("Usage: set_git_conf [-p PROFILE] [REPO_PATH]");
    println!();
    println!("Configure git identity and signing for a repo using blueprint config.");
    println!();
    println!("  -p PROFILE   Profile name (default: from default_user_profile in config)");
    println!("  REPO_PATH    Path to repo (default: current directory)");
    exit(1);
}

#[derive(Debug, PartialEq)]
enum Parsed {
    Ok {
        profile: String,
        rest: Vec<String>,
    },
    /// Invalid option or missing argument; message for stderr (like getopts).
    Err(String),
    Help,
}

/// POSIX getopts-style parsing of the option string "p:h".
fn getopts(args: &[String]) -> Parsed {
    let mut profile = String::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            i += 1;
            break;
        }
        if !a.starts_with('-') || a == "-" {
            break;
        }
        // Every option in "p:h" either consumes the rest of the word (-p) or
        // ends parsing (-h, invalid), so only the first letter matters.
        let mut chars = a[1..].chars();
        match chars.next() {
            Some('p') => {
                let tail: String = chars.collect();
                if !tail.is_empty() {
                    profile = tail;
                } else if let Some(v) = args.get(i + 1) {
                    profile = v.clone();
                    i += 1;
                } else {
                    return Parsed::Err("option requires an argument -- p".into());
                }
            }
            Some('h') => return Parsed::Help,
            c => return Parsed::Err(format!("illegal option -- {}", c.unwrap_or('-'))),
        }
        i += 1;
    }
    Parsed::Ok {
        profile,
        rest: args[i..].to_vec(),
    }
}

fn run_yq(config: &str, expr: &str) -> String {
    let out = Command::new("yq")
        .args(["-r", expr, config])
        .stderr(Stdio::inherit())
        .output()
        .unwrap_or_else(|e| {
            eprintln!("set_git_conf: yq: {e}");
            exit(127);
        });
    if !out.status.success() {
        exit(out.status.code().unwrap_or(1));
    }
    String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string()
}

fn git(repo: &str) -> Command {
    let mut c = Command::new("git");
    c.arg("-C").arg(repo);
    c
}

/// Runs a command, exiting with its status on failure (like `set -e`).
fn must(mut c: Command) {
    match c.status() {
        Ok(s) if s.success() => {}
        Ok(s) => exit(s.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("set_git_conf: {e}");
            exit(127);
        }
    }
}

fn quiet_ok(mut c: Command) -> bool {
    c.stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Lexically normalized absolute path, like `cd "$repo" && pwd` (logical).
fn logical_abs(repo: &str) -> PathBuf {
    let base = env::var("PWD")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && same_dir(p, Path::new(".")))
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"));
    let joined = base.join(repo);
    let mut out = PathBuf::from("/");
    for c in joined.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(n) => out.push(n),
            _ => {}
        }
    }
    out
}

fn same_dir(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (a.metadata(), b.metadata()) {
        (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
        _ => false,
    }
}

/// `basename` semantics for an absolute, normalized path ("/" for the root).
fn base(p: &Path) -> String {
    p.file_name()
        .map_or_else(|| "/".to_string(), |n| n.to_string_lossy().into_owned())
}

fn parent(p: &Path) -> PathBuf {
    p.parent()
        .map_or_else(|| PathBuf::from("/"), Path::to_path_buf)
}

fn main() {
    let home = env::var("HOME").unwrap_or_default();
    let user = env::var("USER").unwrap_or_default();
    let config = format!("{home}/src/github.com/{user}/config/config.yaml");

    let args: Vec<String> = env::args().skip(1).collect();
    let (mut profile, rest) = match getopts(&args) {
        Parsed::Ok { profile, rest } => (profile, rest),
        Parsed::Help => usage(),
        Parsed::Err(msg) => {
            eprintln!("set_git_conf: {msg}");
            usage();
        }
    };
    let repo = rest.first().cloned().unwrap_or_else(|| ".".to_string());

    if !Path::new(&config).is_file() {
        eprintln!("Error: config not found at {config}");
        exit(1);
    }

    if profile.is_empty() {
        profile = run_yq(&config, ".default_user_profile");
    }

    let p = format!(".profiles.{profile}");
    let email = run_yq(&config, &format!("{p}.useremail"));
    let name = run_yq(&config, &format!("{p}.fullname // {p}.username"));
    let gpg_key = run_yq(&config, &format!("{p}.gpg_key"));
    let sign_format = run_yq(&config, &format!("{p}.git_sign_format // \"\""));
    let signers_file = run_yq(
        &config,
        &format!("{p}.allowed_signers_file // \"{home}/.ssh/allowed_signers\""),
    );

    if email == "null" || name == "null" || gpg_key == "null" {
        eprintln!("Error: profile '{profile}' is missing required fields");
        exit(1);
    }

    for (k, v) in [
        ("user.email", email.as_str()),
        ("user.name", name.as_str()),
        ("user.signingKey", gpg_key.as_str()),
        ("commit.gpgsign", "true"),
        ("tag.gpgSign", "true"),
    ] {
        let mut c = git(&repo);
        c.args(["config", k, v]);
        must(c);
    }

    if !sign_format.is_empty() {
        let mut c = git(&repo);
        c.args(["config", "gpg.format", &sign_format]);
        must(c);
    } else {
        let mut c = git(&repo);
        c.args(["config", "--unset", "gpg.format"])
            .stderr(Stdio::null());
        let _ = c.status();
    }

    let mut c = git(&repo);
    c.args(["config", "gpg.ssh.allowedSignersFile", &signers_file]);
    must(c);

    // Set origin remote if missing, derived from the repo's absolute path
    let mut c = git(&repo);
    c.args(["remote", "get-url", "origin"]);
    if !quiet_ok(c) {
        let abs_repo = logical_abs(&repo);
        // Expect path like .../src/<host>/<owner>/<reponame>
        let repo_name = base(&abs_repo);
        let owner = base(&parent(&abs_repo));
        let host = base(&parent(&parent(&abs_repo)));
        if !host.is_empty() && !owner.is_empty() && !repo_name.is_empty() {
            let slug = format!("{owner}/{repo_name}");
            let mut view = Command::new("gh");
            view.args(["repo", "view", &slug]);
            if host == "github.com" && !quiet_ok(view) {
                let mut create = Command::new("gh");
                create.args(["repo", "create", &slug, "--private"]);
                must(create);
                println!("Created private repo: {slug}");
            }
            let url = format!("git@{host}:{slug}.git");
            let mut c = git(&repo);
            c.args(["remote", "add", "origin", &url]);
            must(c);
            println!("Added origin remote: {url}");
        }
    }

    println!("Configured '{repo}' with profile '{profile}' ({name} <{email}>)");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_options() {
        assert_eq!(
            getopts(&v(&["-p", "work", "/r"])),
            Parsed::Ok {
                profile: "work".into(),
                rest: v(&["/r"])
            }
        );
        assert_eq!(
            getopts(&v(&["-pwork"])),
            Parsed::Ok {
                profile: "work".into(),
                rest: vec![]
            }
        );
        assert_eq!(
            getopts(&v(&["--", "-p"])),
            Parsed::Ok {
                profile: "".into(),
                rest: v(&["-p"])
            }
        );
        assert_eq!(
            getopts(&v(&["repo", "-p", "x"])),
            Parsed::Ok {
                profile: "".into(),
                rest: v(&["repo", "-p", "x"])
            }
        );
    }

    #[test]
    fn parse_errors() {
        assert_eq!(getopts(&v(&["-h"])), Parsed::Help);
        assert_eq!(
            getopts(&v(&["-p"])),
            Parsed::Err("option requires an argument -- p".into())
        );
        assert_eq!(
            getopts(&v(&["--public"])),
            Parsed::Err("illegal option -- -".into())
        );
    }

    #[test]
    fn path_parts() {
        let p = PathBuf::from("/home/u/src/github.com/o/r");
        assert_eq!(base(&p), "r");
        assert_eq!(base(&parent(&p)), "o");
        assert_eq!(base(&parent(&parent(&p))), "github.com");
        assert_eq!(base(Path::new("/")), "/");
        assert_eq!(parent(Path::new("/")), PathBuf::from("/"));
    }
}
