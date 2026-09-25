// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `usecode`: build, install and link the usecode checkout.

use crate::configure::engine::{Plan, Task};
use crate::configure::modules::system::make;
use crate::configure::vars::Vars;

pub fn tasks(plan: &mut Plan, _v: &Vars) {
    plan.add(
        Task::new("usecode/build", "Build usecode")
            .tags(&["usecode"])
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
