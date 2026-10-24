// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `usecode`: clone, update, build, install and link the usecode checkout.
//! uc itself comes as a binary and never needs the checkout; this role is
//! for working on usecode.
//!
//! Vim links against the installed libpython, so Python is installed
//! before the whole tree is built. The packages, Rust and the mise tools it needs are
//! pulled in with it (`REQUIRES` in `roles/mod.rs`).

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::git;
use crate::configure::modules::system::make;
use crate::configure::roles::{apt, mise, rust};
use crate::configure::vars::Vars;

/// Where the checkout is cloned from.
const REPO: &str = "ssh://git@github.com/evgnomon/usecode.git";

pub fn tasks(plan: &mut Plan, _v: &Vars) {
    plan.add(
        Task::new("usecode/clone", format!("Clone {REPO}"))
            .tags(&["usecode"])
            // git comes with the build packages, and the same checkout may be
            // in the user's `git_repos` as well.
            .after([apt::BUILD_DEPS, "git/clone:evgnomon/usecode*"])
            .run(|ctx| async move {
                let spec = git::Checkout {
                    repo: REPO,
                    dest: &ctx.vars().usecode_dir,
                    ..git::Checkout::default()
                };
                spec.run(&ctx).await
            }),
    );

    // A checkout with local commits or on a branch without upstream stays
    // where it is.
    plan.add(
        Task::new("usecode/pull", "Fast-forward the usecode checkout")
            .tags(&["usecode"])
            .after(["usecode/clone"])
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
                // Never prompt: a passphrase asked for amid the parallel
                // progress lines looks like a hang. Without a key in the
                // agent the pull fails and the checkout stays where it is.
                ctx.cmd("git")
                    .args(["-C", &dir, "pull", "--ff-only"])
                    .env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes")
                    .env("GIT_TERMINAL_PROMPT", "0")
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
        Task::new("usecode/python", "Build and install Python")
            .tags(&["usecode"])
            .after(["usecode/submodules", apt::BUILD_DEPS])
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
