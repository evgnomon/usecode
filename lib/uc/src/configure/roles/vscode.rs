// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `vscode`: install the extensions listed in ~/.config/Code/extensions.txt.
//! Settings and the list itself are linked by the dotfiles role.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::command::which;
use crate::configure::roles::{apt, dotfiles};
use crate::configure::vars::Vars;
use anyhow::Context;
use std::collections::BTreeSet;

pub fn tasks(plan: &mut Plan, _v: &Vars) {
    plan.add(
        Task::new("vscode/extensions", "Install VS Code extensions")
            .tags(&["vscode"])
            .after([dotfiles::LINKS, apt::PACKAGES])
            .ignore_errors()
            .run(|ctx| async move {
                if which(&ctx, "code").is_none() {
                    return Ok(Outcome::Skipped("code is not installed".into()));
                }
                let list = ctx.vars().home.join(".config/Code/extensions.txt");
                let wanted = std::fs::read_to_string(&list)
                    .with_context(|| format!("reading {}", list.display()))?;
                let installed = ctx
                    .cmd("code")
                    .arg("--list-extensions")
                    .read_only()
                    .output()
                    .await?;
                let have: BTreeSet<String> = installed
                    .stdout
                    .lines()
                    .map(|l| l.trim().to_lowercase())
                    .collect();
                let mut outcome = Outcome::Ok;
                for ext in wanted.lines().map(str::trim) {
                    if ext.is_empty()
                        || ext.starts_with('#')
                        || ext.starts_with("//")
                        || have.contains(&ext.to_lowercase())
                    {
                        continue;
                    }
                    let step = ctx
                        .cmd("code")
                        .args(["--install-extension", ext])
                        .run_step()
                        .await?;
                    outcome = outcome.and(step);
                }
                Ok(outcome)
            }),
    );
}
