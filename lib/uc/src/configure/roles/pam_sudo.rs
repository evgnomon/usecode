// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `pam_sudo`: the sudo PAM stack with YubiKey (U2F) as a sufficient factor.

use crate::configure::engine::{Plan, Task};
use crate::configure::modules::copy;
use crate::configure::roles::apt;
use crate::configure::vars::Vars;
use std::path::Path;

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let distro = &v.facts.distribution;
    let template = v
        .role("pam_sudo")
        .join("templates")
        .join(distro)
        .join("sudo.j2");
    plan.add(
        Task::new("pam_sudo/config", "Install the sudo PAM configuration")
            .tags(&["pam_sudo"])
            .sudo()
            // The U2F module comes with the hardware package group.
            .after([apt::PACKAGES])
            .when(
                template.is_file(),
                format!("no sudo PAM template for {distro}"),
            )
            .run(move |ctx| async move {
                let dest = Path::new("/etc/pam.d/sudo");
                copy::template(&ctx, &template, dest, Some(0o644), true).await
            }),
    );
}
