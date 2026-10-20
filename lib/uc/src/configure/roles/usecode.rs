// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `usecode`: update, build, install and link the usecode checkout.
//!
//! Python's Makefile reads its version through `jsonc`, and vim links
//! against the installed libpython, so those two are installed before the
//! whole tree is built. The packages, Rust and the mise tools it needs are
//! pulled in with it (`REQUIRES` in `roles/mod.rs`).

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::system::make;
use crate::configure::roles::{apt, mise, rust};
use crate::configure::vars::Vars;

pub fn tasks(plan: &mut Plan, _v: &Vars) {
    // A checkout with local commits or on a branch without upstream stays
    // where it is.
    plan.add(
        Task::new("usecode/pull", "Fast-forward the usecode checkout")
            .tags(&["usecode"])
            .ignore_errors()
            .run(|ctx| async move {
                let dir = ctx.vars().usecode_dir.display().to_string();
                let head = || {
                    ctx.cmd("git")
                        .args(["-C", &dir, "rev-parse", "HEAD"])
                        .read_only()
                        .output()
                };
                let before = head().await?.stdout;
                ctx.cmd("git")
                    .args(["-C", &dir, "pull", "--ff-only"])
                    .output()
                    .await?;
                Ok(Outcome::changed(head().await?.stdout != before))
            }),
    );

    plan.add(
        Task::new("usecode/submodules", "Check out the usecode submodules")
            .tags(&["usecode"])
            .after(["usecode/pull"])
            .run(|ctx| async move {
                let dir = ctx.vars().usecode_dir.display().to_string();
                let status = ctx
                    .cmd("git")
                    .args(["-C", &dir, "submodule", "status", "--recursive"])
                    .read_only()
                    .output()
                    .await?;
                // `-` is not initialised, `+` is at another commit.
                if !status.stdout.lines().any(|l| l.starts_with(['-', '+'])) {
                    return Ok(Outcome::Ok);
                }
                ctx.cmd("git")
                    .args(["-C", &dir, "submodule", "update", "--init", "--recursive"])
                    .run_step()
                    .await
            }),
    );

    plan.add(
        Task::new("usecode/jsonc", "Build and install jsonc")
            .tags(&["usecode"])
            .after(["usecode/submodules", rust::RUSTUP, apt::BUILD_DEPS])
            .sudo()
            .run(|ctx| async move {
                let dir = ctx.vars().usecode_dir.join("lib/jsonc");
                let built = make(&ctx, &dir, None, false).await?;
                Ok(built.and(make(&ctx, &dir, Some("install"), true).await?))
            }),
    );

    plan.add(
        Task::new("usecode/python", "Build and install Python")
            .tags(&["usecode"])
            .after(["usecode/jsonc"])
            .sudo()
            .run(|ctx| async move {
                let dir = ctx.vars().usecode_dir.join("lib/python");
                let built = make(&ctx, &dir, None, false).await?;
                let installed = make(&ctx, &dir, Some("install"), true).await?;
                if installed == Outcome::Changed {
                    ctx.cmd("ldconfig").sudo().output().await?;
                }
                Ok(built.and(installed))
            }),
    );

    plan.add(
        Task::new("usecode/build", "Build usecode")
            .tags(&["usecode"])
            .after(["usecode/python", rust::MUSL, mise::TOOLS])
            .run(|ctx| async move { make(&ctx, &ctx.vars().usecode_dir, None, false).await }),
    );
    plan.add(
        Task::new("usecode/install", "Install usecode")
            .tags(&["usecode"])
            .after(["usecode/build"])
            .sudo()
            .run(|ctx| async move {
                make(&ctx, &ctx.vars().usecode_dir, Some("install"), true).await
            }),
    );
    plan.add(
        Task::new("usecode/link", "Link usecode")
            .tags(&["usecode"])
            .after(["usecode/install"])
            .run(
                |ctx| async move { make(&ctx, &ctx.vars().usecode_dir, Some("link"), false).await },
            ),
    );
}
