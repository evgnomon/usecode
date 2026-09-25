// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-configure`: the machine configuration as a native, parallel task
//! graph.
//!
//! The layout follows the Ansible tree it replaced:
//!
//! * [`modules`] are the reusable steps — `copy`, `template`, `file`,
//!   `inflate`, `command`, `apt`, `git`, … — each idempotent and reporting
//!   whether it changed anything.
//! * [`roles`] are the configuration functions, one per Ansible role, that
//!   add tasks built from those modules to a [`Plan`](engine::Plan).
//! * [`engine`] schedules the plan: a task starts as soon as the tasks it
//!   runs after have finished or were excluded by the tag selection, with at
//!   most `--jobs` running at once.
//!
//! The roles' files and templates are read from `lib/uc/roles`.

pub mod ctx;
pub mod engine;
pub mod facts;
pub mod modules;
pub mod report;
pub mod roles;
pub mod vars;

use crate::configure::ctx::Shared;
use crate::configure::engine::{Plan, RunOptions, Selection};
use crate::configure::report::Reporter;
use crate::configure::vars::{LoadOptions, Vars};
use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

/// What `uc configure` was asked to do.
pub struct Options {
    pub selection: Selection,
    pub jobs: usize,
    pub check: bool,
    pub fail_fast: bool,
    pub verbose: bool,
    pub color: bool,
    pub list: bool,
    pub list_tags: bool,
    pub graph: bool,
    pub load: LoadOptions,
}

/// Plans and runs the configuration. Returns whether every task succeeded.
pub fn run(opts: Options) -> Result<bool> {
    let vars = Arc::new(Vars::load(opts.load)?);
    let mut plan = roles::plan(&vars);
    plan.resolve()?;
    let selected = plan.select(&opts.selection);

    if opts.list_tags || opts.list || opts.graph {
        let mut out = std::io::stdout().lock();
        let printed = if opts.list_tags {
            print_tags(&mut out, &plan)
        } else if opts.list {
            print_list(&mut out, &plan, &selected)
        } else {
            print_graph(&mut out, &plan, &selected)
        };
        // A closed pipe, as with `| head`, is not an error.
        return match printed {
            Err(err) if err.kind() != std::io::ErrorKind::BrokenPipe => Err(err.into()),
            _ => Ok(true),
        };
    }
    if selected.is_empty() {
        bail!("no task matches the selected tags; see --list-tags");
    }

    let is_root = vars.facts.uid == 0;
    let needs_sudo = !is_root
        && selected
            .iter()
            .any(|&i| plan.tasks()[i].sudo && plan.tasks()[i].disabled.is_none());
    if needs_sudo {
        prime_sudo(opts.check)?;
    }

    let width = selected
        .iter()
        .map(|&i| plan.tasks()[i].id.len())
        .max()
        .unwrap_or(0);
    let out = Reporter::new(opts.color, opts.verbose, width, selected.len());
    eprintln!(
        "uc configure: {} tasks, profile {}, {} at a time{}",
        selected.len(),
        vars.profile.name(),
        opts.jobs,
        if opts.check { ", check mode" } else { "" }
    );
    let shared = Arc::new(Shared::new(vars, opts.check, out, is_root));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?;
    let summary = runtime.block_on(async {
        let keepalive = needs_sudo.then(|| tokio::spawn(keep_sudo_alive()));
        let run_opts = RunOptions {
            jobs: opts.jobs,
            fail_fast: opts.fail_fast,
        };
        let summary = engine::run(plan, &selected, shared.clone(), run_opts).await;
        if let Some(handle) = keepalive {
            handle.abort();
        }
        summary
    });
    shared.out.recap(&summary);
    Ok(summary.success())
}

/// Asks for the sudo password once, before any output of the parallel run,
/// so the prompt is not buried and tasks can use `sudo -n`. A check run
/// without a terminal goes on without it; reads that need root then fail
/// in their own task.
fn prime_sudo(check: bool) -> Result<()> {
    use std::io::IsTerminal;
    use std::process::{Command, Stdio};

    let cached = Command::new("sudo")
        .args(["-n", "true"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if cached {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        if check {
            eprintln!("uc configure: no cached sudo credentials; root-only checks may fail");
            return Ok(());
        }
        bail!("some tasks need sudo: run from a terminal, or cache credentials with `sudo -v`");
    }
    let status = Command::new("sudo")
        .arg("-v")
        .status()
        .context("running sudo")?;
    if !status.success() {
        bail!("sudo authentication failed");
    }
    Ok(())
}

/// Refreshes the sudo timestamp for as long as the run lasts.
async fn keep_sudo_alive() {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let _ = tokio::process::Command::new("sudo")
            .args(["-n", "-v"])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;
    }
}

fn print_tags(out: &mut impl Write, plan: &Plan) -> std::io::Result<()> {
    for (tag, count) in engine::tag_counts(plan) {
        writeln!(out, "{tag:<24} {count}")?;
    }
    Ok(())
}

fn print_list(
    out: &mut impl Write,
    plan: &Plan,
    selected: &BTreeSet<usize>,
) -> std::io::Result<()> {
    let tasks = plan.tasks();
    let width = selected
        .iter()
        .map(|&i| tasks[i].id.len())
        .max()
        .unwrap_or(0);
    for &i in selected {
        let task = &tasks[i];
        let tags: Vec<&str> = task.tags.iter().map(String::as_str).collect();
        let after: Vec<&str> = plan.deps(i).iter().map(|&d| tasks[d].id.as_str()).collect();
        let sudo = if task.sudo { "  (sudo)" } else { "" };
        writeln!(out, "{:<width$}  {}{sudo}", task.id, task.name)?;
        if !tags.is_empty() {
            writeln!(out, "{:<width$}    tags:  {}", "", tags.join(", "))?;
        }
        if !after.is_empty() {
            writeln!(out, "{:<width$}    after: {}", "", after.join(", "))?;
        }
        if let Some(reason) = &task.disabled {
            writeln!(out, "{:<width$}    skip:  {reason}", "")?;
        }
    }
    writeln!(
        out,
        "\n{} of {} tasks selected",
        selected.len(),
        tasks.len()
    )
}

/// The selected part of the graph in Graphviz DOT, e.g. for `| dot -Tsvg`.
fn print_graph(
    out: &mut impl Write,
    plan: &Plan,
    selected: &BTreeSet<usize>,
) -> std::io::Result<()> {
    let tasks = plan.tasks();
    writeln!(out, "digraph uc_configure {{")?;
    writeln!(out, "  rankdir=LR;")?;
    writeln!(out, "  node [shape=box, fontname=\"monospace\"];")?;
    for &i in selected {
        let style = if tasks[i].disabled.is_some() {
            ", style=dashed"
        } else {
            ""
        };
        writeln!(
            out,
            "  \"{}\" [tooltip=\"{}\"{style}];",
            tasks[i].id, tasks[i].name
        )?;
        for &d in plan.deps(i) {
            if selected.contains(&d) {
                writeln!(out, "  \"{}\" -> \"{}\";", tasks[d].id, tasks[i].id)?;
            }
        }
    }
    writeln!(out, "}}")
}

/// Shell-style wildcard match of a whole string: `*` is any run of
/// characters, `?` any single one.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(glob_match("git/clone:*", "git/clone:evgnomon/usecode"));
        assert!(glob_match("*.ttf", "a.ttf"));
        assert!(glob_match(
            "JetBrainsMonoNerdFontMono-*.ttf",
            "JetBrainsMonoNerdFontMono-Bold.ttf"
        ));
        assert!(glob_match("a?c", "abc"));
        assert!(glob_match("apt", "apt"));
        assert!(!glob_match("apt", "apt/update"));
        assert!(!glob_match("*.ttf", "a.otf"));
    }

    /// The real plan must resolve on any machine: no duplicate ids, no
    /// dangling or cyclic dependencies.
    #[test]
    fn the_configuration_plan_resolves() {
        let vars = Vars::for_tests();
        let mut plan = roles::plan(&vars);
        plan.resolve().unwrap();
        let ids: BTreeSet<&str> = plan.tasks().iter().map(|t| t.id.as_str()).collect();
        for id in [
            "apt/update",
            "dotfiles/links",
            "mise/tools",
            "extrepo/enable:mise",
            "zls/build",
        ] {
            assert!(ids.contains(id), "missing {id}");
        }
        let upgrade = plan.tasks().iter().find(|t| t.id == "apt/upgrade").unwrap();
        assert!(!Selection::default().includes(upgrade));
    }
}
