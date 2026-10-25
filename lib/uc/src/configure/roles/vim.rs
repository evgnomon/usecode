// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `vim`: the vim configs from `roles/vim/files`, copied into the home,
//! vim-plug and the plugin directory.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::{copy, file};
use crate::configure::vars::Vars;
use anyhow::Context;

const PLUG: &str = "https://raw.githubusercontent.com/junegunn/vim-plug/master/plug.vim";

/// The configs, relative to the role's files and the home directory.
const CONFIGS: &[&str] = &[
    ".vimrc",
    ".ideavimrc",
    ".vim/coc-settings.json",
    ".vim/plugin/wl_clipboard.vim",
];

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let files = v.role("vim").join("files");
    plan.add(
        Task::new("vim/config", "Copy the vim configs")
            .tags(&["vim"])
            .run(move |ctx| async move {
                let home = &ctx.vars().home;
                let mut outcome = Outcome::Ok;
                for entry in CONFIGS {
                    let step = copy::copy(&ctx, &files.join(entry), &home.join(entry), None, false)
                        .await?;
                    outcome = outcome.and(step);
                }
                Ok(outcome)
            }),
    );

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

#[cfg(test)]
mod tests {
    use super::CONFIGS;
    use crate::configure::assets::Assets;
    use std::path::Path;

    #[test]
    fn every_config_is_built_in() {
        for entry in CONFIGS {
            let src = Path::new("vim/files").join(entry);
            assert!(Assets::Embedded.is_file(&src), "{entry}");
        }
    }
}
