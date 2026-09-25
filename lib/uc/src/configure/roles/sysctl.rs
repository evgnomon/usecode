// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `sysctl`: kernel settings, applied by a handler when they change.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::copy::{Copy, Source, template};
use crate::configure::roles::{DESKTOP_ONLY, WORKSTATION_ONLY};
use crate::configure::vars::{Profile, Vars};
use std::path::Path;

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let role = v.role("sysctl");
    let desktop = v.profile.desktop();

    let conf = role.join("templates/sysctl.conf.j2");
    plan.add(
        Task::new("sysctl/conf", "Install the sysctl configuration")
            .tags(&["sysctl"])
            .sudo()
            .when(desktop, DESKTOP_ONLY)
            .run(move |ctx| async move {
                template(
                    &ctx,
                    &conf,
                    Path::new("/etc/sysctl.conf"),
                    Some(0o644),
                    true,
                )
                .await
            }),
    );

    let ipv6 = role.join("files/99-disable-ipv6.conf");
    plan.add(
        Task::new(
            "sysctl/ipv6",
            "Install the IPv6 disable sysctl configuration",
        )
        .tags(&["sysctl"])
        .sudo()
        .when(desktop, DESKTOP_ONLY)
        .when(v.profile == Profile::Workstation, WORKSTATION_ONLY)
        .run(move |ctx| async move {
            let dest = Path::new("/etc/sysctl.d/99-disable-ipv6.conf");
            Copy::new(Source::File(&ipv6), dest)
                .mode(0o644)
                .sudo()
                .backup()
                .run(&ctx)
                .await
        }),
    );

    plan.add(
        Task::new("sysctl/apply", "Apply sysctl settings")
            .tags(&["sysctl"])
            .sudo()
            .after(["sysctl/conf", "sysctl/ipv6"])
            .when(desktop, DESKTOP_ONLY)
            .run(|ctx| async move {
                if !ctx.deps_changed() {
                    return Ok(Outcome::Skipped("settings unchanged".into()));
                }
                ctx.cmd("/usr/sbin/sysctl")
                    .arg("--system")
                    .sudo()
                    .run_step()
                    .await
            }),
    );
}
