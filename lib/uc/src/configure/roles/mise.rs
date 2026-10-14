// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `mise`: the mise tool manager and the tools of ~/.config/mise/config.toml.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::apt;
use crate::configure::roles::{apt as apt_role, dotfiles};
use crate::configure::vars::Vars;

pub const TOOLS: &str = "mise/tools";

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
        Task::new(TOOLS, "Install the mise tools")
            .tags(&["mise"])
            .after(["mise/install", dotfiles::LINKS])
            .run(|ctx| async move {
                let out = ctx.cmd("mise").args(["install", "--yes"]).output().await?;
                Ok(Outcome::changed(out.stderr.contains("installed")))
            }),
    );
}
