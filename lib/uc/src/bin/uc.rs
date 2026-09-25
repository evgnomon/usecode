// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc` — the top level usecode command.
//!
//! Like `git`, it holds no subcommand logic of its own: `uc encrypt ...` runs
//! the `uc-encrypt` executable, so new subcommands are added by dropping a
//! `uc-<name>` executable next to `uc` or anywhere on `PATH`.

use std::collections::BTreeSet;
use std::process::{Command, ExitCode};
use uc::dispatch::{self, is_executable, resolve, search_dirs};

const PREFIX: &str = "uc-";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let subcommand = match args.first().map(String::as_str) {
        None | Some("help") | Some("-h") | Some("--help") => {
            print_usage();
            return ExitCode::SUCCESS;
        }
        Some("--version") | Some("-v") => {
            println!("uc {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Some(arg) if arg.starts_with('-') => {
            eprintln!("uc: unknown option '{arg}'");
            print_usage();
            return ExitCode::from(2);
        }
        Some(arg) => arg.to_string(),
    };

    let program = format!("{PREFIX}{subcommand}");
    if resolve(&program).is_none() {
        eprintln!("uc: '{subcommand}' is not a uc command. See 'uc help'.");
        return ExitCode::from(2);
    }
    dispatch::exec(&program, &args[1..])
}

fn print_usage() {
    println!("Usage: uc <command> [<args>]\n");
    let commands = discover();
    if commands.is_empty() {
        println!("No uc commands found on PATH.");
    } else {
        println!("Commands:");
        let width = commands.iter().map(String::len).max().unwrap_or(0);
        for command in &commands {
            match summary(command) {
                Some(summary) => println!("  {command:width$}  {summary}"),
                None => println!("  {command}"),
            }
        }
        println!("\nRun 'uc <command> -h' for the options of a command.");
    }
}

/// One line describing a subcommand, as printed by `uc-<name> --summary`.
/// A subcommand that does not answer is still listed, just without a summary.
fn summary(command: &str) -> Option<String> {
    let path = resolve(&format!("{PREFIX}{command}"))?;
    let output = Command::new(path).arg("--summary").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8(output.stdout).ok()?;
    let line = line.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// Every `uc-<name>` executable reachable from the search path, deduplicated
/// and sorted for the help output.
fn discover() -> BTreeSet<String> {
    let mut commands = BTreeSet::new();
    for dir in search_dirs() {
        let Ok(entries) = dir.read_dir() else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str().and_then(|n| n.strip_prefix(PREFIX)) else {
                continue;
            };
            if !name.is_empty() && is_executable(&entry.path()) {
                commands.insert(name.to_string());
            }
        }
    }
    commands
}
