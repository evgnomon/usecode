// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `ensure_csharp_ls`: the C# language server as a dotnet global tool.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::roles::apt;
use crate::configure::vars::Vars;

pub fn tasks(plan: &mut Plan, _v: &Vars) {
    plan.add(
        Task::new("cs/csharp-ls", "Install csharp-ls")
            .tags(&["cs"])
            // The dotnet SDK comes with the share package group.
            .after([apt::PACKAGES])
            .run(|ctx| async move {
                let bin = ctx
                    .vars()
                    .home
                    .join(".dotnet/tools/csharp-ls")
                    .display()
                    .to_string();
                let version = || {
                    ctx.cmd(bin.clone())
                        .arg("--version")
                        .read_only()
                        .any_code()
                        .output()
                };
                if version().await.is_ok_and(|o| o.success()) {
                    return Ok(Outcome::Ok);
                }
                let outcome = ctx
                    .cmd("dotnet")
                    .args(["tool", "install", "--global", "csharp-ls"])
                    .run_step()
                    .await?;
                if !ctx.check() {
                    let out = ctx
                        .cmd(bin.clone())
                        .arg("--version")
                        .read_only()
                        .output()
                        .await?;
                    ctx.log(&format!("csharp-ls version: {}", out.stdout.trim()));
                }
                Ok(outcome)
            }),
    );
}
