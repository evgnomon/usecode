// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `packer`: the Packer plugins the image builds use.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::roles::{NOT_IN_DEV_CONTAINER, mise};
use crate::configure::vars::{Profile, Vars};

const PLUGINS: &[&str] = &["github.com/hashicorp/ansible", "github.com/hashicorp/qemu"];

pub fn tasks(plan: &mut Plan, v: &Vars) {
    plan.add(
        Task::new("packer/plugins", "Install the Packer plugins")
            .tags(&["packer"])
            // packer itself is a global mise tool.
            .after([mise::TOOLS])
            .when(v.profile != Profile::DevContainer, NOT_IN_DEV_CONTAINER)
            .run(|ctx| async move {
                let installed = ctx
                    .cmd("packer")
                    .args(["plugins", "installed"])
                    .read_only()
                    .output()
                    .await?;
                let mut outcome = Outcome::Ok;
                for plugin in PLUGINS {
                    if installed.stdout.contains(plugin) {
                        continue;
                    }
                    outcome = outcome.and(
                        ctx.cmd("packer")
                            .args(["plugins", "install", plugin])
                            .run_step()
                            .await?,
                    );
                }
                Ok(outcome)
            }),
    );
}
