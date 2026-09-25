// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Command groups: `uc <group> <command> ...` for the tools that live on as
//! their own executables.
//!
//! A [`Group`] is a table of commands, each handing over to a tool (with
//! arguments of its own put in front of the user's), to a nested group, or to
//! a function in this crate. A group may also have a fallback tool that gets
//! any other command line, so `uc cert init` still reaches `certgen init`.
//! The tables themselves are in [`crate::groups`].

use crate::dispatch;
use std::ffi::OsString;
use std::process::ExitCode;

pub struct Group {
    /// The words that reach this group, e.g. `uc cloud play`.
    pub path: &'static str,
    /// One line for `uc help` and the parent group's listing.
    pub summary: &'static str,
    pub commands: &'static [Sub],
    pub fallback: Option<Fallback>,
}

/// The tool that gets the command lines naming none of a group's commands.
#[derive(Clone, Copy)]
pub struct Fallback {
    /// The program and the arguments that go before the user's.
    pub argv: &'static [&'static str],
    /// What it does, for the help text.
    pub summary: &'static str,
    /// Whether it also gets the empty command line, rather than the group
    /// printing its usage.
    pub bare: bool,
}

pub struct Sub {
    pub name: &'static str,
    pub summary: &'static str,
    pub run: Run,
}

pub enum Run {
    /// A program and the arguments that go before the user's.
    Exec(&'static [&'static str]),
    Group(&'static Group),
    Builtin(fn(&[OsString]) -> ExitCode),
}

/// What a command line comes down to.
#[derive(Debug)]
enum Action<'a> {
    Help(&'a Group),
    Missing(&'a Group),
    Unknown(&'a Group, String),
    Exec(&'static [&'static str], &'a [OsString]),
    Builtin(fn(&[OsString]) -> ExitCode, &'a [OsString]),
}

impl std::fmt::Debug for Group {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Group({})", self.path)
    }
}

impl Group {
    fn find(&self, name: &str) -> Option<&Sub> {
        self.commands.iter().find(|sub| sub.name == name)
    }

    fn route<'a>(&'a self, args: &'a [OsString]) -> Action<'a> {
        // A group that is only a new name for one tool hands it everything.
        if let (true, Some(fallback)) = (self.commands.is_empty(), self.fallback) {
            return Action::Exec(fallback.argv, args);
        }
        let first = args.first().map(|arg| arg.to_string_lossy());
        match first.as_deref() {
            None => match self.fallback {
                Some(fallback) if fallback.bare => Action::Exec(fallback.argv, args),
                _ => Action::Missing(self),
            },
            Some("-h" | "--help" | "help") => Action::Help(self),
            Some(name) => match (self.find(name), self.fallback) {
                (Some(sub), _) => match &sub.run {
                    Run::Exec(argv) => Action::Exec(argv, &args[1..]),
                    Run::Group(group) => group.route(&args[1..]),
                    Run::Builtin(run) => Action::Builtin(*run, &args[1..]),
                },
                (None, Some(fallback)) => Action::Exec(fallback.argv, args),
                (None, None) => Action::Unknown(self, name.to_string()),
            },
        }
    }

    fn usage(&self) -> String {
        let mut out = format!(
            "Usage: {} <command> [<args>]\n\n{}\n\nCommands:\n",
            self.path, self.summary
        );
        let width = self
            .commands
            .iter()
            .map(|sub| sub.name.len())
            .max()
            .unwrap_or(0);
        for sub in self.commands {
            let missing = match sub.run {
                Run::Exec([program, ..]) if dispatch::resolve(program).is_none() => {
                    format!(" [{program} not installed]")
                }
                _ => String::new(),
            };
            out += &format!("  {:width$}  {}{missing}\n", sub.name, sub.summary);
        }
        if let Some(fallback) = self.fallback {
            out += &format!("\nAny other command: {}\n", fallback.summary);
        }
        out += &format!(
            "\nRun '{} <command> -h' for the options of a command.",
            self.path
        );
        out
    }

    /// Runs the command line `args` (without the group's own words).
    pub fn run(&self, args: &[OsString]) -> ExitCode {
        match self.route(args) {
            Action::Help(group) => {
                println!("{}", group.usage());
                ExitCode::SUCCESS
            }
            Action::Missing(group) => {
                eprintln!("{}", group.usage());
                ExitCode::from(2)
            }
            Action::Unknown(group, name) => {
                eprintln!(
                    "uc: '{name}' is not a '{}' command. See '{} help'.",
                    group.path, group.path
                );
                ExitCode::from(2)
            }
            Action::Exec([program, lead @ ..], args) => dispatch::exec(
                program,
                lead.iter().map(OsString::from).chain(args.iter().cloned()),
            ),
            Action::Exec([], _) => unreachable!("a command names its program"),
            Action::Builtin(run, args) => run(args),
        }
    }
}

/// The `main` of a `uc-<group>` executable.
pub fn main(group: &Group) -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--summary") {
        println!("{}", group.summary);
        return ExitCode::SUCCESS;
    }
    group.run(&args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groups::ALL;

    fn args(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    fn exec_of(group: &Group, words: &[&str]) -> Vec<String> {
        let args = args(words);
        match group.route(&args) {
            Action::Exec(argv, rest) => argv
                .iter()
                .map(|s| s.to_string())
                .chain(rest.iter().map(|s| s.to_string_lossy().into_owned()))
                .collect(),
            other => panic!("{words:?} did not run a tool: {other:?}"),
        }
    }

    fn walk(group: &'static Group, seen: &mut Vec<&'static Group>) {
        seen.push(group);
        for sub in group.commands {
            if let Run::Group(nested) = sub.run {
                assert!(
                    nested.path.ends_with(&format!(" {}", sub.name)),
                    "{}",
                    nested.path
                );
                walk(nested, seen);
            }
        }
    }

    #[test]
    fn tables_are_well_formed() {
        let mut groups = Vec::new();
        for group in ALL {
            walk(group, &mut groups);
        }
        for group in groups {
            assert!(group.path.starts_with("uc "), "{}", group.path);
            assert!(!group.summary.is_empty(), "{}", group.path);
            assert!(!group.commands.is_empty() || group.fallback.is_some());
            let mut names: Vec<_> = group.commands.iter().map(|sub| sub.name).collect();
            names.sort();
            names.dedup();
            assert_eq!(
                names.len(),
                group.commands.len(),
                "{}: duplicate",
                group.path
            );
            for sub in group.commands {
                assert!(!sub.summary.is_empty(), "{} {}", group.path, sub.name);
                assert!(!matches!(sub.name, "help" | "-h" | "--help" | "--summary"));
                if let Run::Exec(argv) = sub.run {
                    assert!(!argv.is_empty(), "{} {}", group.path, sub.name);
                }
            }
        }
    }

    #[test]
    fn commands_reach_their_tools() {
        use crate::groups::*;
        assert_eq!(exec_of(&IMAGE, &["push", "a:1"]), ["uc-push", "a:1"]);
        assert_eq!(exec_of(&IMAGE, &["run", "yacht", "-v"]), ["yacht", "-v"]);
        assert_eq!(exec_of(&REPO, &["headers", "check"]), ["hgl", "check"]);
        assert_eq!(
            exec_of(&CERT, &["init", "--days", "1"]),
            ["certgen", "init", "--days", "1"]
        );
        assert_eq!(exec_of(&CERT, &["p12", "user_1"]), ["mkp12", "user_1"]);
        assert_eq!(exec_of(&CERT, &["p12", "fetch"]), ["zcdump"]);
        assert_eq!(
            exec_of(&DB, &["resources", "sync", "."]),
            ["ysys", "sync", "."]
        );
        assert_eq!(
            exec_of(&NET, &["mesh", "add", "edge"]),
            ["uc-daemon", "add", "edge"]
        );
        assert_eq!(exec_of(&VM, &[]), ["vm"]);
        assert_eq!(exec_of(&VM, &["-h"]), ["vm", "-h"]);
        assert_eq!(exec_of(&CLOUD, &["play"]), ["y"]);
        assert_eq!(exec_of(&CLOUD, &["play", "-t", "x"]), ["y", "-t", "x"]);
        assert_eq!(exec_of(&CLOUD, &["play", "host", "ps"]), ["plat", "ps"]);
    }

    #[test]
    fn help_and_mistakes_stay_in_the_group() {
        use crate::groups::*;
        let route =
            |group: &'static Group, words: &[&str]| format!("{:?}", group.route(&args(words)));
        assert_eq!(route(&IMAGE, &[]), "Missing(Group(uc image))");
        assert_eq!(route(&IMAGE, &["-h"]), "Help(Group(uc image))");
        assert_eq!(route(&IMAGE, &["run", "help"]), "Help(Group(uc image run))");
        assert_eq!(route(&CERT, &[]), "Missing(Group(uc cert))");
        assert_eq!(route(&CERT, &["--help"]), "Help(Group(uc cert))");
        assert_eq!(route(&REPO, &["nope"]), "Unknown(Group(uc repo), \"nope\")");
        assert!(matches!(
            REPO.route(&args(&["authors"])),
            Action::Builtin(..)
        ));
        assert!(IMAGE.usage().contains("\n  run   "));
    }
}
