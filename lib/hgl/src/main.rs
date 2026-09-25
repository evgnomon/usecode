// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! CLI: check or add the license header in files tracked by git.
//!
//! Use:  hgl [check|fix] [-x PATHSPEC]... [PATH]...
//!
//! `check` (the default) lists files missing the header and exits 1 if any.
//! `fix` adds the header to them. Symlinks and submodules are skipped, so
//! third-party code checked in as a submodule is never touched.

use std::process::{Command, ExitCode};

const USAGE: &str = "\
usage: hgl [check|fix] [-x PATHSPEC]... [PATH]...

Check or add the license header in files tracked by git.

commands:
  check   list files missing the header, exit 1 if any (default)
  fix     add the header to files missing it

options:
  -x, --exclude PATHSPEC   skip files matching PATHSPEC (repeatable)
  -h, --help               show this help
";

/// Git index modes of entries that are not regular files.
const SYMLINK_MODE: &str = "120000";
const GITLINK_MODE: &str = "160000";

struct Args {
    fix: bool,
    excludes: Vec<String>,
    paths: Vec<String>,
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut args = Args {
        fix: false,
        excludes: Vec::new(),
        paths: Vec::new(),
    };
    let mut it = std::env::args().skip(1).peekable();
    match it.peek().map(String::as_str) {
        Some("check") => {
            it.next();
        }
        Some("fix") => {
            args.fix = true;
            it.next();
        }
        _ => {}
    }
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "-x" | "--exclude" => {
                let spec = it.next().ok_or_else(|| format!("{arg} needs a pathspec"))?;
                args.excludes.push(spec);
            }
            s if s.starts_with('-') => return Err(format!("unknown option: {s}")),
            _ => args.paths.push(arg),
        }
    }
    Ok(Some(args))
}

/// Regular files tracked by git, relative to the current directory.
fn tracked_files(args: &Args) -> Result<Vec<String>, String> {
    let mut cmd = Command::new("git");
    cmd.args(["ls-files", "-s", "-z", "--"]);
    cmd.args(&args.paths);
    cmd.args(args.excludes.iter().map(|x| format!(":!{x}")));
    let out = cmd.output().map_err(|e| format!("running git: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let listing = String::from_utf8_lossy(&out.stdout);
    Ok(listing
        .split('\0')
        .filter_map(|entry| {
            let (meta, path) = entry.split_once('\t')?;
            let mode = meta.split(' ').next()?;
            (mode != SYMLINK_MODE && mode != GITLINK_MODE).then(|| path.to_string())
        })
        .collect())
}

fn run(args: &Args) -> Result<usize, String> {
    let mut missing = 0;
    for path in tracked_files(args)? {
        let Ok(bytes) = std::fs::read(&path) else {
            continue; // deleted from the work tree
        };
        if !hgl::is_text(&bytes) {
            continue;
        }
        let Ok(content) = String::from_utf8(bytes) else {
            continue;
        };
        if hgl::has_header(&content) {
            continue;
        }
        let Some(style) = hgl::style_for(&path, &content) else {
            continue;
        };
        missing += 1;
        if args.fix {
            std::fs::write(&path, hgl::apply(&content, style))
                .map_err(|e| format!("{path}: {e}"))?;
            println!("fixed: {path}");
        } else {
            println!("missing header: {path}");
        }
    }
    Ok(missing)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Some(args)) => args,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(err) => {
            eprintln!("hgl: {err}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&args) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(n) if args.fix => {
            eprintln!("hgl: added the header to {n} file(s)");
            ExitCode::SUCCESS
        }
        Ok(n) => {
            eprintln!("hgl: {n} file(s) missing the header; run `hgl fix`");
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!("hgl: {err}");
            ExitCode::from(2)
        }
    }
}
