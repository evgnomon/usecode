// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `wslview`: open links in the Windows browser from WSL.

use crate::configure::engine::{Plan, Task};
use crate::configure::modules::{copy, file};
use crate::configure::roles::local_bin;
use crate::configure::vars::{Profile, Vars};

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let script = v.role("wslview").join("files/wslview");
    let wsl = v.profile == Profile::Wsl;

    plan.add(
        Task::new("wslview/link", "Link wslview to ~/.local/bin")
            .tags(&["wslview"])
            .after([local_bin::DIR])
            .when(wsl, "only on the wsl profile")
            .run(move |ctx| async move {
                file::link(&ctx, &script, &ctx.vars().local_bin.join("wslview"), false).await
            }),
    );

    plan.add(
        Task::new("wslview/browser", "Set BROWSER to wslview")
            .tags(&["wslview"])
            .when(wsl, "only on the wsl profile")
            .run(|ctx| async move {
                let dest = ctx.vars().home.join(".bashrc.d/wslview.sh");
                let text = "command -v wslview >/dev/null 2>&1 && export BROWSER=wslview\n";
                copy::content(&ctx, text, &dest, Some(0o644), false).await
            }),
    );
}
