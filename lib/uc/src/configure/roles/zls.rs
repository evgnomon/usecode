// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `ensure_zls`: build the Zig language server at a pinned version.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::{copy, git};
use crate::configure::roles::{local_bin, mise};
use crate::configure::vars::Vars;

const VERSION: &str = "0.16.0";

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let repo = v.home.join("src/github.com/zigtools/zls");

    let src = repo.clone();
    plan.add(
        Task::new("zls/source", "Clone zigtools/zls")
            .tags(&["zls"])
            .run(move |ctx| async move {
                let spec = git::Checkout {
                    repo: "https://github.com/zigtools/zls.git",
                    dest: &src,
                    version: Some(VERSION),
                    update: true,
                    force: true,
                    ..git::Checkout::default()
                };
                spec.run(&ctx).await
            }),
    );

    plan.add(
        Task::new("zls/build", "Build and install zls")
            .tags(&["zls"])
            // zig comes from mise.
            .after(["zls/source", mise::TOOLS, local_bin::DIR])
            .run(move |ctx| async move {
                let bin = ctx.vars().local_bin.join("zls");
                let mut outcome = Outcome::Ok;
                if !bin.exists() || ctx.deps_changed() {
                    ctx.cmd("zig")
                        .args(["build", "-Doptimize=ReleaseSafe"])
                        .cwd(&repo)
                        .output()
                        .await?;
                    let built = repo.join("zig-out/bin/zls");
                    outcome = copy::copy(&ctx, &built, &bin, Some(0o755), false).await?;
                }
                let version = ctx
                    .cmd(bin.display().to_string())
                    .arg("--version")
                    .read_only()
                    .any_code()
                    .output()
                    .await?;
                ctx.log(&format!("zls version: {}", version.stdout.trim()));
                Ok(outcome)
            }),
    );
}
