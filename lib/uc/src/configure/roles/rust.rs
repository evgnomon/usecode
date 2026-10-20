// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `ensure_rust`: rustup with its toolchain, the musl target uc builds
//! for, and the rust-analyzer component.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::vars::Vars;
use anyhow::Context;

pub const RUSTUP: &str = "rust/rustup";
pub const MUSL: &str = "rust/musl";

/// The target uc builds for, static with no C dependencies.
const MUSL_TARGET: &str = "x86_64-unknown-linux-musl";

const INSTALLER: &str = "https://sh.rustup.rs";

pub fn tasks(plan: &mut Plan, _v: &Vars) {
    plan.add(
        Task::new(RUSTUP, "Install rustup and the Rust toolchain")
            .tags(&["rust"])
            .run(|ctx| async move {
                if ctx.vars().home.join(".cargo/bin/rustc").exists() {
                    return Ok(Outcome::Ok);
                }
                if ctx.check() {
                    return Ok(Outcome::Changed);
                }
                let script = crate::kube::cloudflare::client()
                    .get(INSTALLER)
                    .send()
                    .await
                    .and_then(reqwest::Response::error_for_status)
                    .with_context(|| format!("downloading {INSTALLER}"))?
                    .bytes()
                    .await
                    .with_context(|| format!("downloading {INSTALLER}"))?;
                ctx.cmd("sh")
                    .args(["-s", "--", "-y"])
                    .stdin(script.to_vec())
                    .run_step()
                    .await
            }),
    );

    plan.add(
        Task::new(MUSL, format!("Add the {MUSL_TARGET} target"))
            .tags(&["rust"])
            .after([RUSTUP])
            .run(|ctx| async move {
                let installed = ctx
                    .cmd("rustup")
                    .args(["target", "list", "--installed"])
                    .read_only()
                    .output()
                    .await?;
                if installed.stdout.lines().any(|l| l == MUSL_TARGET) {
                    return Ok(Outcome::Ok);
                }
                ctx.cmd("rustup")
                    .args(["target", "add", MUSL_TARGET])
                    .run_step()
                    .await
            }),
    );

    plan.add(
        Task::new("rust/analyzer", "Add the rust-analyzer component")
            .tags(&["rust"])
            .after([RUSTUP])
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
