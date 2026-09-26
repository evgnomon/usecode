// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `fonts`: copy the downloaded Nerd Fonts into ~/.fonts.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::{file, walk_files};
use crate::configure::roles::{WORKSTATION_ONLY, apt};
use crate::configure::vars::{Profile, Vars};
use anyhow::Context;

pub fn tasks(plan: &mut Plan, v: &Vars) {
    plan.add(
        Task::new(
            "fonts/install",
            "Install the TTF fonts from the blueprint cache",
        )
        .tags(&["fonts"])
        .after(["pkg/*", apt::PACKAGES])
        .when(v.profile == Profile::Workstation, WORKSTATION_ONLY)
        .run(|ctx| async move {
            let v = ctx.vars();
            let fonts = v.home.join(".fonts");
            let mut outcome = file::directory(&ctx, &fonts, None, false).await?;
            let cache = &v.cache_dir;
            let found = if cache.is_dir() {
                walk_files(cache)?
            } else {
                Vec::new()
            };
            for rel in found
                .iter()
                .filter(|p| p.extension().is_some_and(|e| e == "ttf"))
            {
                let src = cache.join(rel);
                let dest = fonts.join(rel.file_name().unwrap_or_default());
                let same =
                    dest.metadata().ok().map(|m| m.len()) == src.metadata().ok().map(|m| m.len());
                if same {
                    continue;
                }
                if !ctx.check() {
                    std::fs::copy(&src, &dest)
                        .with_context(|| format!("copying {}", src.display()))?;
                }
                outcome = Outcome::Changed;
            }
            if outcome == Outcome::Changed {
                ctx.cmd("fc-cache").arg("-f").output().await?;
            }
            Ok(outcome)
        }),
    );
}
