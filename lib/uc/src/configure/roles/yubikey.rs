// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `yubikey`: udev access to FIDO keys, smart cards for the plugdev group
//! and `usbip` on the PATH, for the packages of the hardware group.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::copy::copy;
use crate::configure::modules::file;
use crate::configure::roles::{WORKSTATION_ONLY, apt};
use crate::configure::vars::{Profile, Vars};
use std::path::{Path, PathBuf};

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let files = v.role("yubikey").join("files");
    let workstation = v.profile == Profile::Workstation;

    let rules = files.join("70-u2f.rules");
    plan.add(
        Task::new("yubikey/udev", "Install the FIDO U2F udev rules")
            .tags(&["yubikey"])
            .sudo()
            .when(workstation, WORKSTATION_ONLY)
            .run(move |ctx| async move {
                let dest = Path::new("/etc/udev/rules.d/70-u2f.rules");
                copy(&ctx, &rules, dest, Some(0o644), true).await
            }),
    );

    let polkit = files.join("90-pcscd.rules");
    plan.add(
        Task::new("yubikey/pcscd", "Let plugdev use the smart card daemon")
            .tags(&["yubikey"])
            .sudo()
            .after([apt::PACKAGES])
            .when(workstation, WORKSTATION_ONLY)
            .run(move |ctx| async move {
                let dest = Path::new("/etc/polkit-1/rules.d/90-pcscd.rules");
                let copied = copy(&ctx, &polkit, dest, Some(0o644), true).await?;
                // Earlier setups wrote it without the `s`, which polkit
                // never read.
                let old = Path::new("/etc/polkit-1/rules.d/90-pcscd.rule");
                Ok(copied.and(file::absent(&ctx, old, true).await?))
            }),
    );

    plan.add(
        Task::new("yubikey/usbip", "Put the kernel's usbip on the PATH")
            .tags(&["yubikey"])
            .sudo()
            .after([apt::PACKAGES])
            .when(workstation, WORKSTATION_ONLY)
            .run(|ctx| async move {
                let Some(usbip) = newest_usbip() else {
                    return Ok(Outcome::Skipped("no linux-tools usbip".into()));
                };
                let link = Path::new("/usr/local/bin/usbip");
                if std::fs::canonicalize(link).ok() == std::fs::canonicalize(&usbip).ok() {
                    return Ok(Outcome::Ok);
                }
                ctx.cmd("update-alternatives")
                    .args(["--install", "/usr/local/bin/usbip", "usbip"])
                    .arg(usbip.display().to_string())
                    .arg("20")
                    .sudo()
                    .run_step()
                    .await
            }),
    );
}

/// `/usr/lib/linux-tools/<version>/usbip` of the last version, as Ubuntu
/// ships it.
fn newest_usbip() -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir("/usr/lib/linux-tools")
        .ok()?
        .filter_map(|e| Some(e.ok()?.path().join("usbip")))
        .filter(|p| p.is_file())
        .collect();
    found.sort();
    found.pop()
}
