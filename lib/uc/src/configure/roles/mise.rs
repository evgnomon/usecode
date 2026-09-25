// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `mise`: the mise tool manager and the tools of ~/.config/mise/config.toml.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::apt;
use crate::configure::roles::{apt as apt_role, dotfiles};
use crate::configure::vars::Vars;
use anyhow::bail;

pub const TOOLS: &str = "mise/tools";

const GLOBAL_TOOLS: &[&str] = &["packer"];

pub fn tasks(plan: &mut Plan, _v: &Vars) {
    // The mise Apt repository is enabled by the extrepo role.
    plan.add(
        Task::new("mise/install", "Install mise")
            .tags(&["mise"])
            .sudo()
            .after([apt_role::UPDATE])
            .run(|ctx| async move { apt::install(&ctx, &["mise".to_string()], None).await }),
    );

    plan.add(
        Task::new("mise/global-tools", "Install the global mise tools")
            .tags(&["mise"])
            .after(["mise/install", dotfiles::LINKS])
            .run(|ctx| async move {
                let mut outcome = Outcome::Ok;
                for tool in GLOBAL_TOOLS {
                    let dry = ctx
                        .cmd("mise")
                        .args(["use", "--global", "--dry-run-code", tool])
                        .read_only()
                        .any_code()
                        .output()
                        .await?;
                    match dry.code {
                        0 => {}
                        1 => {
                            let step = ctx
                                .cmd("mise")
                                .args(["--yes", "use", "--global", tool])
                                .run_step()
                                .await?;
                            outcome = outcome.and(step);
                        }
                        code => bail!(
                            "mise use --dry-run-code {tool} exited with {code}\n{}",
                            dry.stderr
                        ),
                    }
                }
                Ok(outcome)
            }),
    );

    plan.add(
        Task::new(TOOLS, "Install the mise tools")
            .tags(&["mise"])
            .after(["mise/global-tools"])
            .run(|ctx| async move {
                let out = ctx.cmd("mise").args(["install", "--yes"]).output().await?;
                Ok(Outcome::changed(out.stderr.contains("installed")))
            }),
    );
}
