// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `ensure_rust`: the rust-analyzer component.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::vars::Vars;

pub fn tasks(plan: &mut Plan, _v: &Vars) {
    plan.add(
        Task::new("rust/analyzer", "Add the rust-analyzer component")
            .tags(&["rust"])
            .run(|ctx| async move {
                let installed = ctx
                    .cmd("rustup")
                    .args(["component", "list", "--installed"])
                    .read_only()
                    .output()
                    .await?;
                if installed
                    .stdout
                    .lines()
                    .any(|l| l.starts_with("rust-analyzer"))
                {
                    return Ok(Outcome::Ok);
                }
                ctx.cmd("rustup")
                    .args(["component", "add", "rust-analyzer"])
                    .run_step()
                    .await
            }),
    );
}
