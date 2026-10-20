// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `wsl_sudo`: the Windows-aware sudo alternative under WSL.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::vars::{Profile, Vars};
use std::path::Path;

const SUDO_WS: &str = "/usr/bin/sudo.ws";

pub fn tasks(plan: &mut Plan, v: &Vars) {
    plan.add(
        Task::new("wsl_sudo/alternative", "Switch sudo to sudo.ws")
            .tags(&["wsl_sudo"])
            .sudo()
            .when(v.profile == Profile::Wsl, "only on the wsl profile")
            .run(|ctx| async move {
                if !Path::new(SUDO_WS).is_file() {
                    return Ok(Outcome::Skipped(format!("no {SUDO_WS}")));
                }
                let current = ctx
                    .cmd("update-alternatives")
                    .args(["--query", "sudo"])
                    .read_only()
                    .any_code()
                    .output()
                    .await?;
                if current
                    .stdout
                    .lines()
                    .any(|l| l == format!("Value: {SUDO_WS}"))
                {
                    return Ok(Outcome::Ok);
                }
                ctx.cmd("update-alternatives")
                    .args(["--set", "sudo", SUDO_WS])
                    .sudo()
                    .run_step()
                    .await
            }),
    );
}
