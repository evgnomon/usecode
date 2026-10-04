// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `tm`: windows, tabs and splits in the terminal, the way a browser has
//! them, kept by tmux underneath.
//!
//! A window here is a tmux session, a tab is a tmux window and a split is a
//! tmux pane. Only the browser words ever reach the user: in the commands,
//! the help and the output, tmux's own messages included.

use clap::{Parser, Subcommand};
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};

#[derive(Parser)]
#[command(
    name = "tm",
    bin_name = "tm",
    about = "Windows, tabs and splits in the terminal, like a browser has them",
    after_help = "Examples:\n  \
        tm window new work\n  \
        tm window open work\n  \
        tm tab new logs --in work\n  \
        tm tab move logs --to play\n  \
        tm split right"
)]
struct Cli {
    #[command(subcommand)]
    command: Noun,
}

#[derive(Subcommand)]
enum Noun {
    /// Open, close and list windows
    #[command(subcommand)]
    Window(WindowCmd),
    /// Open, close, move and list the tabs of a window
    #[command(subcommand)]
    Tab(TabCmd),
    /// Split the current tab and close or list its splits
    #[command(subcommand)]
    Split(SplitCmd),
}

#[derive(Subcommand)]
enum WindowCmd {
    /// List the windows
    #[command(alias = "ls")]
    List,
    /// Make a new window, without opening it
    New {
        /// Its name (one is picked when left out)
        name: Option<String>,
    },
    /// Open a window, or switch to it from inside another one
    Open {
        /// The window (the last one used when left out)
        name: Option<String>,
    },
    /// Close a window and everything in it
    Close {
        /// The window (the current one when left out)
        name: Option<String>,
    },
    /// Give a window a new name
    Rename { name: String, new_name: String },
}

#[derive(Subcommand)]
enum TabCmd {
    /// List the tabs of a window
    #[command(alias = "ls")]
    List {
        /// The window (the current one when left out)
        #[arg(long = "in", value_name = "WINDOW")]
        window: Option<String>,
    },
    /// Open a new tab
    New {
        /// Its name (one is picked when left out)
        name: Option<String>,
        /// Put it in this window, staying where you are
        #[arg(long = "in", value_name = "WINDOW")]
        window: Option<String>,
    },
    /// Go to a tab
    Open {
        /// The tab, by name or number
        tab: String,
        #[arg(long = "in", value_name = "WINDOW")]
        window: Option<String>,
    },
    /// Close a tab and its splits
    Close {
        /// The tab, by name or number (the current one when left out)
        tab: Option<String>,
        #[arg(long = "in", value_name = "WINDOW")]
        window: Option<String>,
    },
    /// Give a tab a new name
    Rename {
        /// The tab, by name or number
        tab: String,
        new_name: String,
        #[arg(long = "in", value_name = "WINDOW")]
        window: Option<String>,
    },
    /// Move a tab to another window
    Move {
        /// The tab, by name or number
        tab: String,
        /// The window it goes to
        #[arg(long, value_name = "WINDOW")]
        to: String,
        /// The window it is in now (the current one when left out)
        #[arg(long = "in", value_name = "WINDOW")]
        window: Option<String>,
    },
}

#[derive(Subcommand)]
enum SplitCmd {
    /// Split the current tab side by side, the new split on the right
    Right,
    /// Split the current tab top and bottom, the new split below
    Down,
    /// List the splits of the current tab
    #[command(alias = "ls")]
    List,
    /// Close a split
    Close {
        /// The split's number, from `tm split list` (the current one when left out)
        number: Option<u32>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("tm: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run(noun: Noun) -> Result<(), String> {
    match noun {
        Noun::Window(cmd) => window(cmd),
        Noun::Tab(cmd) => tab(cmd),
        Noun::Split(cmd) => split(cmd),
    }
}

fn window(cmd: WindowCmd) -> Result<(), String> {
    match cmd {
        WindowCmd::List => {
            let out = match tmux(&[
                "list-sessions",
                "-F",
                "#{session_name}\t#{session_windows}\t#{session_attached}",
            ]) {
                Ok(out) => out,
                Err(msg) if msg == NOTHING_OPEN => {
                    println!("No windows yet. Make one with: tm window new <name>");
                    return Ok(());
                }
                Err(msg) => return Err(msg),
            };
            let current = current("#{session_name}");
            let rows: Vec<Vec<String>> = out
                .lines()
                .map(|line| {
                    let f: Vec<&str> = line.split('\t').collect();
                    let open = if f[2] != "0" { "open" } else { "" };
                    vec![
                        mark(current.as_deref() == Some(f[0])),
                        f[0].to_string(),
                        count(f[1], "tab"),
                        open.to_string(),
                    ]
                })
                .collect();
            print_table(&rows);
            Ok(())
        }
        WindowCmd::New { name } => {
            let mut args = vec!["new-session", "-d", "-P", "-F", "#{session_name}"];
            if let Some(name) = &name {
                args.extend(["-s", name]);
            }
            let made = tmux(&args)?;
            println!(
                "Made window {}. Open it with: tm window open {}",
                made.trim(),
                made.trim()
            );
            Ok(())
        }
        WindowCmd::Open { name } => {
            let inside = std::env::var_os("TMUX").is_some();
            match (name, inside) {
                (Some(name), true) => {
                    tmux(&["switch-client", "-t", &window_target(&name)]).map(drop)
                }
                (None, true) => {
                    Err("you are in a window already; name the one to switch to".into())
                }
                (Some(name), false) => attach(&["attach-session", "-t", &window_target(&name)]),
                (None, false) => attach(&["attach-session"]),
            }
        }
        WindowCmd::Close { name } => {
            let mut args = vec!["kill-session"];
            let target = name.as_deref().map(window_target);
            if let Some(target) = &target {
                args.extend(["-t", target]);
            }
            tmux(&args).map(drop)
        }
        WindowCmd::Rename { name, new_name } => {
            tmux(&["rename-session", "-t", &window_target(&name), &new_name]).map(drop)
        }
    }
}

fn tab(cmd: TabCmd) -> Result<(), String> {
    match cmd {
        TabCmd::List { window } => {
            let mut args = vec![
                "list-windows",
                "-F",
                "#{window_index}\t#{window_name}\t#{window_panes}\t#{window_active}",
            ];
            let target = window.as_deref().map(window_target);
            if let Some(target) = &target {
                args.extend(["-t", target]);
            }
            let out = tmux(&args)?;
            let rows: Vec<Vec<String>> = out
                .lines()
                .map(|line| {
                    let f: Vec<&str> = line.split('\t').collect();
                    let splits = if f[2] == "1" {
                        String::new()
                    } else {
                        count(f[2], "split")
                    };
                    vec![
                        mark(f[3] == "1"),
                        f[0].to_string(),
                        f[1].to_string(),
                        splits,
                    ]
                })
                .collect();
            print_table(&rows);
            Ok(())
        }
        TabCmd::New { name, window } => {
            let mut args = vec!["new-window"];
            let target = window.as_deref().map(|w| format!("{}:", window_target(w)));
            if let Some(target) = &target {
                // Opened in another window, it should not pull you over there.
                args.extend(["-d", "-t", target]);
            }
            if let Some(name) = &name {
                args.extend(["-n", name]);
            }
            tmux(&args).map(drop)
        }
        TabCmd::Open { tab, window } => {
            tmux(&["select-window", "-t", &tab_target(&tab, window.as_deref())]).map(drop)
        }
        TabCmd::Close { tab, window } => {
            let mut args = vec!["kill-window"];
            let target = match (tab, window) {
                (Some(tab), window) => Some(tab_target(&tab, window.as_deref())),
                (None, Some(window)) => Some(format!("{}:", window_target(&window))),
                (None, None) => None,
            };
            if let Some(target) = &target {
                args.extend(["-t", target]);
            }
            tmux(&args).map(drop)
        }
        TabCmd::Rename {
            tab,
            new_name,
            window,
        } => tmux(&[
            "rename-window",
            "-t",
            &tab_target(&tab, window.as_deref()),
            &new_name,
        ])
        .map(drop),
        TabCmd::Move { tab, to, window } => tmux(&[
            "move-window",
            "-s",
            &tab_target(&tab, window.as_deref()),
            "-t",
            &format!("{}:", window_target(&to)),
        ])
        .map(drop),
    }
}

fn split(cmd: SplitCmd) -> Result<(), String> {
    match cmd {
        SplitCmd::Right => tmux(&["split-window", "-h"]).map(drop),
        SplitCmd::Down => tmux(&["split-window", "-v"]).map(drop),
        SplitCmd::List => {
            let out = tmux(&[
                "list-panes",
                "-F",
                "#{pane_index}\t#{pane_current_command}\t#{pane_width}x#{pane_height}\t#{pane_active}",
            ])?;
            let rows: Vec<Vec<String>> = out
                .lines()
                .map(|line| {
                    let f: Vec<&str> = line.split('\t').collect();
                    vec![
                        mark(f[3] == "1"),
                        f[0].to_string(),
                        f[1].to_string(),
                        f[2].to_string(),
                    ]
                })
                .collect();
            print_table(&rows);
            Ok(())
        }
        SplitCmd::Close { number } => {
            let mut args = vec!["kill-pane"];
            let target = number.map(|n| format!(".{n}"));
            if let Some(target) = &target {
                args.extend(["-t", target]);
            }
            tmux(&args).map(drop)
        }
    }
}

/// What a failing command says when tmux has nothing running at all.
const NOTHING_OPEN: &str = "no windows are open";

/// Runs tmux and returns what it printed, or what went wrong in our words.
fn tmux(args: &[&str]) -> Result<String, String> {
    let out = Command::new("tmux")
        .args(args)
        .output()
        .map_err(|err| format!("could not run tmux: {err}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    Err(explain(&String::from_utf8_lossy(&out.stderr)))
}

/// Hands the terminal over to tmux for good, to show a window.
fn attach(args: &[&str]) -> Result<(), String> {
    // Check first, so a missing window is reported in our words rather than
    // by tmux after it took over.
    if args.len() > 1 {
        tmux(&["has-session", "-t", args[2]])?;
    } else {
        tmux(&["list-sessions"])?;
    }
    let err = Command::new("tmux").args(args).exec();
    Err(format!("could not run tmux: {err}"))
}

/// The value of a format for the window you are in, when you are in one.
fn current(format: &str) -> Option<String> {
    std::env::var_os("TMUX")?;
    let out = tmux(&["display-message", "-p", format]).ok()?;
    Some(out.trim().to_string())
}

/// A window by its exact name, so `work` never matches `workshop`.
fn window_target(name: &str) -> String {
    format!("={name}")
}

/// A tab by number, or by its exact name, in the given or current window.
fn tab_target(tab: &str, window: Option<&str>) -> String {
    let tab = if !tab.is_empty() && tab.bytes().all(|b| b.is_ascii_digit()) {
        tab.to_string()
    } else {
        format!("={tab}")
    };
    match window {
        Some(window) => format!("{}:{tab}", window_target(window)),
        None => format!(":{tab}"),
    }
}

/// Puts tmux's complaint in browser words.
fn explain(stderr: &str) -> String {
    let msg = stderr.trim();
    let no_server =
        msg.starts_with("error connecting") && msg.ends_with("(No such file or directory)");
    if msg.starts_with("no server running") || no_server {
        return NOTHING_OPEN.to_string();
    }
    if msg.starts_with("no current client") || msg.starts_with("no current session") {
        return "you are not in a window; open one or say which with --in".to_string();
    }
    let msg = msg
        .replace("pane", "split")
        .replace("window", "\0")
        .replace("session", "window")
        .replace('\0', "tab")
        .replace("can't find", "no such");
    // Names went in with `=` for exact matching; it is not part of them.
    msg.replace(": =", ": ")
}

fn mark(current: bool) -> String {
    if current { "*" } else { " " }.to_string()
}

fn count(n: &str, noun: &str) -> String {
    if n == "1" {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

fn print_table(rows: &[Vec<String>]) {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..cols)
        .map(|c| {
            rows.iter()
                .map(|r| r.get(c).map_or(0, |s| s.chars().count()))
                .max()
                .unwrap_or(0)
        })
        .collect();
    for row in rows {
        let line: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(c, cell)| format!("{cell:<width$}", width = widths[c]))
            .collect();
        println!("{}", line.join("  ").trim_end());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_by_name_or_number() {
        assert_eq!(tab_target("logs", None), ":=logs");
        assert_eq!(tab_target("2", None), ":2");
        assert_eq!(tab_target("logs", Some("work")), "=work:=logs");
    }

    #[test]
    fn tmux_words_never_show() {
        assert_eq!(
            explain("can't find session: =work\n"),
            "no such window: work"
        );
        assert_eq!(explain("can't find window: =logs"), "no such tab: logs");
        assert_eq!(explain("can't find pane: .3"), "no such split: .3");
        assert_eq!(explain("duplicate session: work"), "duplicate window: work");
        assert_eq!(
            explain("no server running on /tmp/tmux-1000/default"),
            NOTHING_OPEN
        );
    }

    #[test]
    fn cli_reads_like_english() {
        Cli::try_parse_from(["tm", "window", "rename", "work", "play"]).unwrap();
        Cli::try_parse_from(["tm", "tab", "new", "logs", "--in", "work"]).unwrap();
        Cli::try_parse_from(["tm", "tab", "move", "logs", "--to", "play"]).unwrap();
        Cli::try_parse_from(["tm", "split", "close"]).unwrap();
        assert!(Cli::try_parse_from(["tm", "tab", "move", "logs"]).is_err());
    }
}
