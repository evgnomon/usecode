// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `local_bin`: the directory other roles link executables into. Ansible
//! pulled it in as a role dependency, so it runs whatever the tags.

use crate::configure::engine::{Plan, Task};
use crate::configure::modules::file;
use crate::configure::vars::Vars;

pub const DIR: &str = "local_bin/dir";

pub fn tasks(plan: &mut Plan, _v: &Vars) {
    plan.add(
        Task::new(DIR, "Ensure ~/.local/bin exists")
            .tags(&["always"])
            .run(|ctx| async move {
                file::directory(&ctx, &ctx.vars().local_bin, Some(0o755), false).await
            }),
    );
}
