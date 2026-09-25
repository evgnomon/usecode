// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `digitalocean_galaxy_collection`: the DigitalOcean Ansible collection
//! the deploy playbooks use.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::command::which;
use crate::configure::vars::Vars;

const COLLECTION: &str = "digitalocean.cloud";

pub fn tasks(plan: &mut Plan, _v: &Vars) {
    plan.add(
        Task::new(
            "digitalocean/collection",
            "Install the digitalocean.cloud Ansible collection",
        )
        .tags(&["digitalocean"])
        .run(|ctx| async move {
            if which(&ctx, "ansible-galaxy").is_none() {
                return Ok(Outcome::Skipped("ansible-galaxy is not installed".into()));
            }
            let listed = ctx
                .cmd("ansible-galaxy")
                .args(["collection", "list", COLLECTION])
                .read_only()
                .any_code()
                .output()
                .await?;
            if listed.success() && listed.stdout.contains(COLLECTION) {
                return Ok(Outcome::Ok);
            }
            ctx.cmd("ansible-galaxy")
                .args(["collection", "install", COLLECTION])
                .run_step()
                .await
        }),
    );
}
