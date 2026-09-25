// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `gnome`: avatar, shell settings and removal of the bundled applications.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::apt;
use crate::configure::roles::{DESKTOP_ONLY, WORKSTATION_ONLY, apt as apt_role};
use crate::configure::vars::{Profile, Vars};
use anyhow::Context;

const BUNDLED_APPS: &[&str] = &[
    "gnome-games",
    "gnome-mahjongg",
    "gnome-chess",
    "gnome-sudoku",
    "aisleriot",
    "quadrapassel",
    "gnome-mines",
    "iagno",
    "gnome-2048",
    "baobab",
    "simple-scan",
    "file-roller",
    "firefox",
    "firefox-esr",
    "gnome-calculator",
    "gnome-calendar",
    "gnome-characters",
    "gnome-clocks",
    "gnome-contacts",
    "gnome-disk-utility",
    "gnome-font-viewer",
    "gnome-logs",
    "gnome-maps",
    "gnome-music",
    "gnome-software",
    "gnome-software-plugin-deb",
    "gnome-software-plugin-fwupd",
    "gnome-system-monitor",
    "gnome-text-editor",
    "gnome-tour",
    "gnome-user-docs",
    "gnome-weather",
    "libreoffice-common",
    "libreoffice-writer",
    "libreoffice-calc",
    "libreoffice-impress",
    "libreoffice-base",
    "libreoffice-draw",
    "libreoffice-math",
    "malcontent",
    "malcontent-gui",
    "mate-user-guide",
    "rhythmbox",
    "shotwell",
    "totem",
    "transmission-gtk",
    "yelp",
];

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let desktop = v.profile.desktop();

    plan.add(
        Task::new("gnome/avatar", "Set the GNOME avatar")
            .tags(&["gnome"])
            .when(desktop, DESKTOP_ONLY)
            .run(|ctx| async move {
                let home = &ctx.vars().home;
                let (face, dest) = (home.join(".ssh/.face"), home.join(".face"));
                if !face.exists() || dest.exists() {
                    return Ok(Outcome::Ok);
                }
                if !ctx.check() {
                    std::fs::copy(&face, &dest)
                        .with_context(|| format!("copying {}", face.display()))?;
                }
                Ok(Outcome::Changed)
            }),
    );

    plan.add(
        Task::new(
            "gnome/app-switcher",
            "Limit the app switcher to the current workspace",
        )
        .tags(&["gnome"])
        .when(desktop, DESKTOP_ONLY)
        .run(|ctx| async move {
            let schema = "org.gnome.shell.app-switcher";
            let schemas = ctx
                .cmd("gsettings")
                .arg("list-schemas")
                .read_only()
                .any_code()
                .output()
                .await;
            let present =
                schemas.is_ok_and(|o| o.success() && o.stdout.lines().any(|l| l == schema));
            if !present {
                return Ok(Outcome::Skipped(format!("no {schema} schema")));
            }
            let key = "current-workspace-only";
            let current = ctx
                .cmd("gsettings")
                .args(["get", schema, key])
                .read_only()
                .output()
                .await?;
            if current.stdout.trim() == "true" {
                return Ok(Outcome::Ok);
            }
            ctx.cmd("gsettings")
                .args(["set", schema, key, "true"])
                .run_step()
                .await
        }),
    );

    plan.add(
        Task::new("gnome/remove-apps", "Remove GNOME games and applications")
            .tags(&["gnome"])
            .sudo()
            .after([apt_role::PACKAGES])
            .when(v.profile == Profile::Workstation, WORKSTATION_ONLY)
            .run(|ctx| async move {
                let names: Vec<String> = BUNDLED_APPS.iter().map(|s| s.to_string()).collect();
                let removed = apt::remove(&ctx, &names, true).await?;
                if removed == Outcome::Changed {
                    apt::autoremove(&ctx).await?;
                }
                Ok(removed)
            }),
    );
}
