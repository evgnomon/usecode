// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The configuration itself: one module per former Ansible role, each
//! adding its tasks to the plan. Task ids are `<role>/<step>`, and the role
//! part matches the tag the Ansible role used, so `-t zls` or `-t dotfiles`
//! select what they did before.
//!
//! Where Ansible relied on play order, a task names what it needs with
//! [`Task::after`](crate::configure::engine::Task::after); everything else is
//! free to run in parallel.

pub mod alacritty;
pub mod apt;
pub mod bash_history_backup;
pub mod brave_bookmarks;
pub mod docker;
pub mod dotfiles;
pub mod extrepo;
pub mod fonts;
pub mod git;
pub mod gnome;
pub mod goose;
pub mod local_bin;
pub mod mise;
pub mod pam_sudo;
pub mod pkg;
pub mod rust;
pub mod sudo_nopasswd;
pub mod sysctl;
pub mod udev_hwdb;
pub mod usecode;
pub mod wslview;
pub mod zls;

use crate::configure::engine::{Plan, Task};
use crate::configure::glob_match;
use crate::configure::vars::Vars;

/// Synced on every machine: what uc needs to find its config, run and
/// update itself.
const ESSENTIAL: &[&str] = &["local_bin", "usecode", "dotfiles/usecode-config"];

/// What a part pulls in when it is synced: the Alacritty config names the
/// CaskaydiaMono Nerd Font.
const REQUIRES: &[(&str, &[&str])] = &[("alacritty", &["fonts", "pkg/cascadia-mono"])];

/// Whether `pattern`, as in the `sync` list, names the task or its role.
fn names(pattern: &str, task: &Task) -> bool {
    pattern == "all" || glob_match(pattern, &task.id) || glob_match(pattern, task.role())
}

/// Whether the task is essential or the user's `sync` list asks for it,
/// directly or through what a listed part requires.
fn wanted(sync: &[String], task: &Task) -> bool {
    let required = REQUIRES
        .iter()
        .filter(|(part, _)| sync.iter().any(|p| p == "all" || glob_match(p, part)))
        .flat_map(|(_, needs)| needs.iter().copied());
    ESSENTIAL
        .iter()
        .copied()
        .chain(required)
        .chain(sync.iter().map(String::as_str))
        .any(|p| names(p, task))
}

/// Every task: the Debian-family roles, gated on the OS family, then the
/// roles for all platforms.
pub fn plan(v: &Vars) -> Plan {
    let mut plan = Plan::default();

    local_bin::tasks(&mut plan, v);
    pkg::tasks(&mut plan, v);
    usecode::tasks(&mut plan, v);
    apt::tasks(&mut plan, v);
    extrepo::tasks(&mut plan, v);
    dotfiles::tasks(&mut plan, v);
    git::tasks(&mut plan, v);
    pam_sudo::tasks(&mut plan, v);
    sudo_nopasswd::tasks(&mut plan, v);
    sysctl::tasks(&mut plan, v);
    udev_hwdb::tasks(&mut plan, v);
    gnome::tasks(&mut plan, v);
    fonts::tasks(&mut plan, v);
    mise::tasks(&mut plan, v);
    bash_history_backup::tasks(&mut plan, v);
    brave_bookmarks::tasks(&mut plan, v);
    wslview::tasks(&mut plan, v);
    docker::tasks(&mut plan, v);
    zls::tasks(&mut plan, v);
    plan.gate(
        0,
        v.facts.is_debian_family(),
        "only configured on Debian-family systems",
    );

    rust::tasks(&mut plan, v);
    goose::tasks(&mut plan, v);
    alacritty::tasks(&mut plan, v);
    plan.opt_in(
        |task| wanted(&[], task),
        |task| wanted(&v.config.sync, task),
    );
    plan
}

/// Reasons shown for tasks the profile leaves out.
pub const NOT_IN_DEV_CONTAINER: &str = "not installed in dev containers";
pub const DESKTOP_ONLY: &str = "only on the desktop (workstation) profile";
pub const WORKSTATION_ONLY: &str = "only on the workstation profile";
