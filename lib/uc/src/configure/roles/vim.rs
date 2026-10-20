// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `vim`: vim-plug and the plugin directory; the vim configs themselves are
//! linked by `usecode/link`.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::{copy, file};
use crate::configure::vars::Vars;
use anyhow::Context;

const PLUG: &str = "https://raw.githubusercontent.com/junegunn/vim-plug/master/plug.vim";

pub fn tasks(plan: &mut Plan, _v: &Vars) {
    plan.add(
        Task::new("vim/plugin-dir", "Ensure ~/.vim/plugin exists")
            .tags(&["vim"])
            .run(|ctx| async move {
                let dir = ctx.vars().home.join(".vim/plugin");
                file::directory(&ctx, &dir, None, false).await
            }),
    );

    plan.add(
        Task::new("vim/plug", "Install vim-plug")
            .tags(&["vim"])
            .run(|ctx| async move {
                let dest = ctx.vars().home.join(".vim/autoload/plug.vim");
                if dest.exists() {
                    return Ok(Outcome::Ok);
                }
                if ctx.check() {
                    return Ok(Outcome::Changed);
                }
                let text = crate::kube::cloudflare::client()
                    .get(PLUG)
                    .send()
                    .await
                    .and_then(reqwest::Response::error_for_status)
                    .with_context(|| format!("downloading {PLUG}"))?
                    .text()
                    .await
                    .with_context(|| format!("downloading {PLUG}"))?;
                copy::content(&ctx, &text, &dest, Some(0o644), false).await
            }),
    );
}
