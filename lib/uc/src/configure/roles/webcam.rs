// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `webcam`: keeps USB webcams that break under autosuspend awake.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::copy::copy;
use crate::configure::roles::WORKSTATION_ONLY;
use crate::configure::vars::{Profile, Vars};
use std::path::Path;

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let rules = v.role("webcam").join("files").join("99-webcam-awake.rules");
    let workstation = v.profile == Profile::Workstation;

    plan.add(
        Task::new("webcam/udev", "Keep USB webcams out of autosuspend")
            .tags(&["webcam"])
            .sudo()
            .when(workstation, WORKSTATION_ONLY)
            .run(move |ctx| async move {
                let dest = Path::new("/etc/udev/rules.d/99-webcam-awake.rules");
                copy(&ctx, &rules, dest, Some(0o644), true).await
            }),
    );

    plan.add(
        Task::new(
            "webcam/reload",
            "Apply the webcam rules to connected cameras",
        )
        .tags(&["webcam"])
        .sudo()
        .after(["webcam/udev"])
        .when(workstation, WORKSTATION_ONLY)
        .run(|ctx| async move {
            if !ctx.deps_changed() {
                return Ok(Outcome::Skipped("webcam rules unchanged".into()));
            }
            ctx.shell(
                "udevadm control --reload && udevadm trigger --subsystem-match=usb --action=change",
            )
            .sudo()
            .run_step()
            .await
        }),
    );
}
