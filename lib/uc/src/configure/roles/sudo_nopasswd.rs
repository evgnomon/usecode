// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `sudo_nopasswd`: members of the `sudo` group run sudo without a password.

use crate::configure::engine::{Plan, Task};
use crate::configure::modules::copy;
use crate::configure::roles::NOT_IN_DEV_CONTAINER;
use crate::configure::vars::{Profile, Vars};
use std::path::Path;

/// sudo skips files in `/etc/sudoers.d` whose names contain a dot, and reads
/// them after the `%sudo` line of `/etc/sudoers`, so this rule wins.
const DEST: &str = "/etc/sudoers.d/sudo-nopasswd";
const RULE: &str = "%sudo ALL=(ALL:ALL) NOPASSWD: ALL\n";

pub fn tasks(plan: &mut Plan, v: &Vars) {
    plan.add(
        Task::new(
            "sudo_nopasswd/rule",
            "Let the sudo group use sudo without a password",
        )
        .tags(&["sudo_nopasswd"])
        .sudo()
        .when(v.profile != Profile::DevContainer, NOT_IN_DEV_CONTAINER)
        .run(|ctx| async move {
            // sudo ignores a sudoers file that is not 0440 and owned by root;
            // the write installs it as root.
            copy::content(&ctx, RULE, Path::new(DEST), Some(0o440), true).await
        }),
    );
}
