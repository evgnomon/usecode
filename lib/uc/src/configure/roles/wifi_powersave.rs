// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `wifi_powersave`: Wi-Fi power saving on battery only, off while the
//! charger is plugged in, switched by udev as the charger comes and goes.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::{apt, copy::copy};
use crate::configure::roles::{WORKSTATION_ONLY, apt as apt_role};
use crate::configure::vars::{Profile, Vars};
use std::path::Path;

const SCRIPT: &str = "/usr/local/sbin/wifi-powersave";

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let files = v.role("wifi_powersave").join("files");
    let workstation = v.profile == Profile::Workstation;

    plan.add(
        Task::new(
            "wifi_powersave/iw",
            "Install iw, which switches power saving",
        )
        .tags(&["wifi_powersave"])
        .sudo()
        .after([apt_role::UPDATE])
        .when(workstation, WORKSTATION_ONLY)
        .run(|ctx| async move { apt::install(&ctx, &["iw".to_string()], None).await }),
    );

    plan.add(
        Task::new(
            "wifi_powersave/install",
            "Switch Wi-Fi power saving with the charger",
        )
        .tags(&["wifi_powersave"])
        .sudo()
        .when(workstation, WORKSTATION_ONLY)
        .run(move |ctx| async move {
            let mut outcome = Outcome::Ok;
            for (name, dest, mode) in [
                ("wifi-powersave", SCRIPT, 0o755),
                (
                    "70-wifi-powersave.rules",
                    "/etc/udev/rules.d/70-wifi-powersave.rules",
                    0o644,
                ),
                (
                    "wifi-powersave.conf",
                    "/etc/NetworkManager/conf.d/wifi-powersave.conf",
                    0o644,
                ),
            ] {
                let step = copy(&ctx, &files.join(name), Path::new(dest), Some(mode), true).await?;
                outcome = outcome.and(step);
            }
            Ok(outcome)
        }),
    );

    plan.add(
        Task::new(
            "wifi_powersave/apply",
            "Apply Wi-Fi power saving for the charger as it is now",
        )
        .tags(&["wifi_powersave"])
        .sudo()
        .after(["wifi_powersave/iw", "wifi_powersave/install"])
        .when(workstation, WORKSTATION_ONLY)
        .run(|ctx| async move {
            if !ctx.deps_changed() {
                return Ok(Outcome::Skipped("Wi-Fi power saving unchanged".into()));
            }
            ctx.shell(&format!(
                "udevadm control --reload && \
                 systemctl try-reload-or-restart NetworkManager && {SCRIPT}"
            ))
            .sudo()
            .run_step()
            .await
        }),
    );
}
