// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `bash_history_backup`: an hourly systemd user timer backing up the bash
//! history.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::{copy, file, system};
use crate::configure::roles::local_bin;
use crate::configure::vars::{Profile, Vars};

const UNITS: &[&str] = &["backup-bash-history.service", "backup-bash-history.timer"];

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let files = v.role("bash_history_backup").join("files");
    let wanted = !matches!(v.profile, Profile::DevContainer | Profile::Wsl);
    let reason = "not on the dev_container and wsl profiles";

    plan.add(
        Task::new(
            "bash_history_backup/install",
            "Install the backup script and units",
        )
        .after([local_bin::DIR])
        .when(wanted, reason)
        .run(move |ctx| async move {
            let v = ctx.vars();
            let script = v.local_bin.join("backup-bash-history");
            let mut outcome = copy::copy(
                &ctx,
                &files.join("backup-bash-history"),
                &script,
                Some(0o755),
                false,
            )
            .await?;
            let units = v.home.join(".config/systemd/user");
            outcome = outcome.and(file::directory(&ctx, &units, Some(0o755), false).await?);
            for unit in UNITS {
                let step = copy::copy(
                    &ctx,
                    &files.join(unit),
                    &units.join(unit),
                    Some(0o644),
                    false,
                )
                .await?;
                outcome = outcome.and(step);
            }
            Ok(outcome)
        }),
    );

    plan.add(
        Task::new("bash_history_backup/timer", "Enable the backup timer")
            .after(["bash_history_backup/install"])
            .when(wanted, reason)
            .run(|ctx| async move {
                if !system::user_manager_running(&ctx).await? {
                    return Ok(Outcome::Skipped("no systemd user instance".into()));
                }
                let mut outcome = Outcome::Ok;
                if ctx.deps_changed() {
                    outcome = system::user_daemon_reload(&ctx).await?;
                }
                Ok(outcome.and(system::user_enable_now(&ctx, "backup-bash-history.timer").await?))
            }),
    );
}
