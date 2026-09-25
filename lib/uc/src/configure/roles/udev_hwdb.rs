// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `udev_hwdb`: keyboard maps, with the hwdb rebuilt when they change.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::copy::copy;
use crate::configure::roles::DESKTOP_ONLY;
use crate::configure::vars::Vars;
use std::path::Path;

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let files = v.role("udev_hwdb").join("files");
    let desktop = v.profile.desktop();

    let keyboard = files.join("61-keyboard-builtin-altwin.hwdb");
    plan.add(
        Task::new("udev_hwdb/keyboard", "Install the per-device keyboard map")
            .tags(&["udev_hwdb"])
            .sudo()
            .when(desktop, DESKTOP_ONLY)
            .when(v.facts.distribution == "Debian", "only on Debian")
            .run(move |ctx| async move {
                let dest = Path::new("/etc/udev/hwdb.d/61-keyboard-builtin-altwin.hwdb");
                copy(&ctx, &keyboard, dest, Some(0o644), true).await
            }),
    );

    let apple = files.join("hid_apple.conf");
    plan.add(
        Task::new(
            "udev_hwdb/apple",
            "Install the Apple keyboard module options",
        )
        .tags(&["udev_hwdb"])
        .sudo()
        .when(desktop, DESKTOP_ONLY)
        .run(move |ctx| async move {
            let dest = Path::new("/etc/modprobe.d/hid_apple.conf");
            copy(&ctx, &apple, dest, Some(0o644), true).await
        }),
    );

    plan.add(
        Task::new("udev_hwdb/rebuild", "Rebuild the hwdb")
            .tags(&["udev_hwdb"])
            .sudo()
            .after(["udev_hwdb/keyboard"])
            .when(desktop, DESKTOP_ONLY)
            .run(|ctx| async move {
                if !ctx.deps_changed() {
                    return Ok(Outcome::Skipped("keyboard map unchanged".into()));
                }
                ctx.shell("systemd-hwdb update && udevadm trigger --subsystem-match=input --action=change")
                    .sudo()
                    .run_step()
                    .await
            }),
    );
}
