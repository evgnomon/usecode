// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc` — the top level usecode command.
//!
//! Like `git`, it holds no subcommand logic of its own: `uc secret ...` runs
//! the `uc-secret` executable, so new subcommands are added by dropping a
//! `uc-<name>` executable next to `uc` or anywhere on `PATH`. With no
//! command at all it is the coding agent, [`uc::chat`].

use std::collections::BTreeSet;
use std::process::{Command, ExitCode};
use uc::chat::Search;
use uc::dispatch::{self, is_executable, resolve, search_dirs};

const PREFIX: &str = "uc-";

/// Short names for commands: `uc ws` is `uc workspace`.
const ALIASES: &[(&str, &str)] = &[("ws", "workspace")];

/// The agent's options: `uc --ask --output --commands --no-shell`, and the search engine.
const CHAT_FLAGS: &[&str] = &[
    "--ask",
    "--output",
    "--commands",
    "--no-shell",
    "--parallel",
    "--exa",
    "--duckduckgo",
];

/// The search engines' flags.
const SEARCH_FLAGS: &[(&str, Search)] = &[
    ("--parallel", Search::Parallel),
    ("--exa", Search::Exa),
    ("--duckduckgo", Search::DuckDuckGo),
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // No command is the agent. Each of its flags may also come from
    // `UC_<FLAG>` or the user config, see `uc::flag`.
    if args.iter().all(|a| CHAT_FLAGS.contains(&a.as_str())) {
        let has = |flag: &str| uc::flag::on(&args, flag);
        let search = SEARCH_FLAGS.iter().find(|(f, _)| has(f)).map(|(_, s)| *s);
        return uc::chat::main(
            has("--ask"),
            has("--output"),
            has("--commands"),
            !has("--no-shell"),
            search,
        );
    }
    let subcommand = match args[0].as_str() {
        "help" | "-h" | "--help" => {
            print_usage();
            return ExitCode::SUCCESS;
        }
        "--version" | "-v" => {
            println!("uc {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        arg if arg.starts_with('-') => {
            eprintln!("uc: unknown option '{arg}'");
            print_usage();
            return ExitCode::from(2);
        }
        arg => ALIASES
            .iter()
            .find(|(short, _)| *short == arg)
            .map_or(arg, |(_, long)| long)
            .to_string(),
    };

    let program = format!("{PREFIX}{subcommand}");
    if resolve(&program).is_none() {
        eprintln!("uc: '{subcommand}' is not a uc command. See 'uc help'.");
        return ExitCode::from(2);
    }
    dispatch::exec(&program, &args[1..])
}

fn print_usage() {
    println!(
        "Usage: uc [--ask] [--output] [--commands] [--no-shell] [--parallel|--exa|--duckduckgo] | <command> [<args>]\n"
    );
    println!("With no command, uc is a coding agent: talk to it and it runs commands and");
    println!("edits files for you, through OpenRouter (OPENROUTER_API_KEY, UC_MODEL,");
    println!("UC_EFFORT), a Claude subscription (`uc agent login claude`) or GitHub Copilot");
    println!("(`uc agent login copilot`); `uc agent model` lists the models of each one with a");
    println!(
        "key, and UC_PROVIDER picks the one it starts on; naming a model there or in the \
         chat makes it stick. It runs them straight away; `uc --ask` confirms each one first."
    );
    println!("You see its answers; `uc --output` also shows what the commands print.");
    println!("Each command it runs shows as a few words on what it does, like");
    println!("`· list the rust files`; `uc --commands` shows the command line instead.");
    println!("A line that is a command, like `ls -al`, runs on the terminal straight");
    println!("away; `uc --no-shell` sends every line to the model instead.");
    println!("Every flag can also be set as UC_<FLAG> (UC_NO_SHELL=1) or under `uc:` in");
    println!("config.yaml (no_shell: true); the command line wins, then the environment.");
    println!("It searches the web with Parallel (PARALLEL_API_KEY), else Exa (EXA_API_KEY),");
    println!("else DuckDuckGo; `uc --exa` or `uc --duckduckgo` picks one.\n");
    let commands = discover();
    if commands.is_empty() {
        println!("No uc commands found on PATH.");
    } else {
        println!("Commands:");
        // A command with a short name is listed as `workspace, ws`.
        let labels: Vec<(&String, String)> = commands
            .iter()
            .map(
                |command| match ALIASES.iter().find(|(_, long)| long == command) {
                    Some((short, _)) => (command, format!("{command}, {short}")),
                    None => (command, command.clone()),
                },
            )
            .collect();
        let width = labels.iter().map(|(_, l)| l.len()).max().unwrap_or(0);
        for (command, label) in &labels {
            match summary(command) {
                Some(summary) => println!("  {label:width$}  {summary}"),
                None => println!("  {label}"),
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
/// and sorted for the help output. Names with a dash are the tools behind a
/// group's commands (`uc-db-pg` for `uc db pg`), listed by their group.
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
            if !name.is_empty() && !name.contains('-') && is_executable(&entry.path()) {
                commands.insert(name.to_string());
            }
        }
    }
    commands
}
