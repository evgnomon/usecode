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

/// Short names for commands: `uc ws` is `uc workspace`, `uc a` is `uc agent`.
const ALIASES: &[(&str, &str)] = &[("ws", "workspace"), ("a", "agent")];

/// The agent's options: `uc --ask --output --commands --no-shell --long-cache`, and the search engine.
const CHAT_FLAGS: &[&str] = &[
    "--ask",
    "--output",
    "--commands",
    "--no-shell",
    "--long-cache",
    "--no-subagents",
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

/// The agent's flags that take a value, `--subagent-model haiku` or
/// `--subagent-model=haiku`: the subagents' model, for all kinds or one
/// (`--subagent-model-explore`), and their effort; and the providers to take
/// a model from first (`--provider-order claude,openrouter`).
fn is_value_flag(flag: &str) -> bool {
    flag == "--subagent-effort"
        || flag == "--subagent-model"
        || flag == "--provider-order"
        || flag
            .strip_prefix("--subagent-model-")
            .is_some_and(|kind| !kind.is_empty() && kind.chars().all(|c| c.is_ascii_lowercase()))
}

/// Environment variables to set: names and values.
type Vars = Vec<(String, String)>;

/// `args` without the agent's flags that take a value, and those as the
/// `UC_<FLAG>` variables they stand for; `None` when one has no value.
fn split_values(args: &[String]) -> Option<(Vec<String>, Vars)> {
    let (mut rest, mut values) = (Vec::new(), Vec::new());
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let (flag, value) = match arg.split_once('=') {
            Some((flag, value)) if is_value_flag(flag) => (flag, value.to_string()),
            _ if is_value_flag(arg) => (arg.as_str(), args.next()?.clone()),
            _ => {
                rest.push(arg.clone());
                continue;
            }
        };
        let name = flag
            .trim_start_matches('-')
            .replace('-', "_")
            .to_uppercase();
        values.push((format!("UC_{name}"), value));
    }
    Some((rest, values))
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // The agent's flags with a value go in the environment, where
    // `uc::flag::value` reads them, the command line winning over what was
    // there; only for the agent, not for a command they came before.
    if let Some((rest, values)) = split_values(&args)
        && !values.is_empty()
        && rest.iter().all(|a| CHAT_FLAGS.contains(&a.as_str()))
    {
        for (name, value) in values {
            // SAFETY: at the start of main, before any other thread runs.
            unsafe { std::env::set_var(name, value) };
        }
        args = rest;
    }
    // No command is the agent. Each of its flags may also come from
    // `UC_<FLAG>` or the user config, see `uc::flag`.
    if args.iter().all(|a| CHAT_FLAGS.contains(&a.as_str())) {
        let has = |flag: &str| uc::flag::on(&args, flag);
        let search = SEARCH_FLAGS.iter().find(|(f, _)| has(f)).map(|(_, s)| *s);
        uc::claude::LONG_CACHE.store(has("--long-cache"), std::sync::atomic::Ordering::Relaxed);
        uc::chat::NO_SUBAGENTS.store(has("--no-subagents"), std::sync::atomic::Ordering::Relaxed);
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
        "Usage: uc [--ask] [--output] [--commands] [--no-shell] [--long-cache] [--no-subagents] [--subagent-model MODEL] [--subagent-effort LEVEL] [--provider-order P1,P2] [--parallel|--exa|--duckduckgo] | <command> [<args>]\n"
    );
    println!("With no command, uc is a coding agent: talk to it and it runs commands and");
    println!("edits files for you, through the providers in ~/.config/usecode/agent.yaml:");
    println!("OpenRouter, any OpenAI-compatible API, a Claude subscription (`uc agent login");
    println!("claude`) or GitHub Copilot (`uc agent login copilot`). `uc agent model` lists");
    println!("the models of each one with a key; provider:, model: and effort: in the same");
    println!("file (or UC_PROVIDER, UC_MODEL, UC_EFFORT) pick what it starts on, and naming");
    println!("a model there or in the chat saves it. A model more than one provider serves");
    println!("comes from the one model_providers: names for it, else the first of");
    println!("provider_order: (or `--provider-order claude,openrouter`) that has it.");
    println!("It runs them straight away; `uc --ask`");
    println!("confirms each one first.");
    println!("You see its answers; `uc --output` also shows what the commands print.");
    println!("Each command it runs shows as a few words on what it does, like");
    println!("`· list the rust files`; `uc --commands` shows the command line instead.");
    println!("A line that is a command, like `ls -al`, runs on the terminal straight");
    println!("away; `uc --no-shell` sends every line to the model instead.");
    println!("Your Claude subscription keeps the conversation cached for an hour; Claude on");
    println!("OpenRouter for five minutes, and `uc --long-cache` makes that an hour too.");
    println!("It hands looking around, web research, test runs and big changes side by");
    println!("side to subagents on a faster model, so your context stays small and cheap;");
    println!("`--subagent-model` (fast, balanced, strong, same or a model) and");
    println!("`--subagent-effort` pick theirs, and `uc --no-subagents` keeps it all on one.");
    println!("Every flag can also be set as UC_<FLAG> (UC_NO_SHELL=1) or under `uc:` in");
    println!("config.yaml (no_shell: true); the command line wins, then the environment.");
    println!("It searches the web with Parallel, else Exa (their keys under search: in");
    println!("agent.yaml), else DuckDuckGo; `uc --exa` or `uc --duckduckgo` picks one.\n");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn the_subagent_flags_with_a_value_go_to_the_environment() {
        let line = "--ask --subagent-model haiku --subagent-effort=medium";
        let (rest, values) = split_values(&words(line)).unwrap();
        assert_eq!(rest, ["--ask"]);
        assert_eq!(
            values,
            [
                ("UC_SUBAGENT_MODEL".to_string(), "haiku".to_string()),
                ("UC_SUBAGENT_EFFORT".to_string(), "medium".to_string()),
            ]
        );
        let (_, values) = split_values(&words("--subagent-model-research strong")).unwrap();
        assert_eq!(values[0].0, "UC_SUBAGENT_MODEL_RESEARCH");
        assert!(split_values(&words("--subagent-model")).is_none());
        let (rest, values) = split_values(&words("vm create --name x")).unwrap();
        assert_eq!((rest.len(), values.len()), (4, 0));
    }
}
