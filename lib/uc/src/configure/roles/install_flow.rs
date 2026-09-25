// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `install_flow`: link every script in a `bin` directory of
//! `lib/workflows` into ~/.local/bin.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::{file, walk_files};
use crate::configure::roles::local_bin;
use crate::configure::vars::Vars;

pub fn tasks(plan: &mut Plan, _v: &Vars) {
    plan.add(
        Task::new("install_flow/link", "Link workflow scripts to ~/.local/bin")
            .tags(&["install_flow"])
            .after([local_bin::DIR])
            .run(|ctx| async move {
                let v = ctx.vars();
                let root = v.usecode_dir.join("lib/workflows");
                let mut outcome = Outcome::Ok;
                for rel in walk_files(&root)? {
                    let in_bin = rel
                        .parent()
                        .is_some_and(|dir| dir.components().any(|c| c.as_os_str() == "bin"));
                    let Some(name) = rel.file_name().filter(|_| in_bin) else {
                        continue;
                    };
                    let step =
                        file::link(&ctx, &root.join(&rel), &v.local_bin.join(name), false).await?;
                    outcome = outcome.and(step);
                }
                Ok(outcome)
            }),
    );
}
