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

pub mod apt;
pub mod bash_history_backup;
pub mod csharp_ls;
pub mod digitalocean;
pub mod docker;
pub mod dotfiles;
pub mod extrepo;
pub mod fonts;
pub mod git;
pub mod gnome;
pub mod goose;
pub mod llama_cpp;
pub mod local_bin;
pub mod mise;
pub mod packer;
pub mod pam_sudo;
pub mod pkg;
pub mod rust;
pub mod sysctl;
pub mod udev_hwdb;
pub mod usecode;
pub mod vscode;
pub mod wslview;
pub mod zls;

use crate::configure::engine::Plan;
use crate::configure::vars::Vars;

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
    sysctl::tasks(&mut plan, v);
    udev_hwdb::tasks(&mut plan, v);
    gnome::tasks(&mut plan, v);
    fonts::tasks(&mut plan, v);
    mise::tasks(&mut plan, v);
    bash_history_backup::tasks(&mut plan, v);
    wslview::tasks(&mut plan, v);
    vscode::tasks(&mut plan, v);
    docker::tasks(&mut plan, v);
    packer::tasks(&mut plan, v);
    llama_cpp::tasks(&mut plan, v);
    zls::tasks(&mut plan, v);
    plan.gate(
        0,
        v.facts.is_debian_family(),
        "only configured on Debian-family systems",
    );

    rust::tasks(&mut plan, v);
    csharp_ls::tasks(&mut plan, v);
    goose::tasks(&mut plan, v);
    digitalocean::tasks(&mut plan, v);
    plan
}

/// Reasons shown for tasks the profile leaves out.
pub const NOT_IN_DEV_CONTAINER: &str = "not installed in dev containers";
pub const DESKTOP_ONLY: &str = "only on the desktop (workstation) profile";
pub const WORKSTATION_ONLY: &str = "only on the workstation profile";
