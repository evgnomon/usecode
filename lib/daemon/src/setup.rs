// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Putting things on the host `usecoded` runs on. Neither command here
//! sets anything up - that is the daemon's job, done by its modules
//! whenever it runs (see [`crate::daemon`]).
//!
//! - `usecoded setup` installs the daemon: this binary at [`BINARY_PATH`]
//!   and usecode.service, enabled and started. It is what
//!   `uc daemon install` runs on the target after copying the binary
//!   there, and it needs nothing else on the host.
//! - `usecoded join BUNDLE` hands each section of a [`Bundle`] to the
//!   module it belongs to, which puts its files in place, and reloads
//!   the daemon if anything changed.
//!
//! Each file is only written when it differs, and the service is
//! restarted or reloaded only if one was, so running either again is
//! cheap.

use std::fs;

use crate::bundle::Bundle;
use crate::daemon::{host, mesh};
use crate::error::{Context, Result};

pub const BINARY_PATH: &str = "/usr/local/bin/usecoded";
const CONFIG_DIR: &str = "/etc/uc";
const UNIT_PATH: &str = "/etc/systemd/system/usecode.service";
const UNIT: &str = include_str!("../init/systemd/usecode.service");

/// The unit's name before it became usecode.service. Left behind, it
/// would be a second unit driving the same interface.
const OLD_UNIT: &str = "uc-daemon";

/// Install the daemon on this host.
pub fn run() -> Result<()> {
    let mut restart = false;

    let exe = std::env::current_exe().ctx("locate usecoded executable")?;
    let binary = fs::read(&exe).with_ctx(|| format!("read {}", exe.display()))?;
    restart |= host::put(BINARY_PATH, &binary, 0o755)?;

    host::ensure_dir(CONFIG_DIR, 0o750)?;

    restart |= remove_old_unit()?;
    restart |= host::put(UNIT_PATH, UNIT.as_bytes(), 0o644)?;

    host::systemctl(&["daemon-reload"])?;
    host::systemctl(&["enable", "--quiet", "usecode"])?;
    host::systemctl(&[if restart { "restart" } else { "start" }, "usecode"])?;
    println!(
        "usecode is {}",
        if restart { "restarted" } else { "running" }
    );
    Ok(())
}

/// Hand every section of the bundle at `bundle_path` to its module and
/// have the daemon pick the result up.
pub fn join(bundle_path: &str) -> Result<()> {
    let bundle = Bundle::load(bundle_path)?;
    let mut changed = false;
    if let Some(d) = &bundle.mesh {
        changed |= mesh::accept(d).ctx("mesh")?;
    }

    if changed {
        // reload-or-restart also starts a stopped unit.
        host::systemctl(&["reload-or-restart", "usecode"]).ctx(
            "reload usecode (is the daemon installed? `uc daemon install` puts it on the host)",
        )?;
        println!("{}: delivered; usecode reloaded", bundle.host);
    } else {
        println!("{}: already up to date", bundle.host);
    }
    Ok(())
}

/// Stop, disable and delete the pre-rename unit if it is still around.
fn remove_old_unit() -> Result<bool> {
    let path = format!("/etc/systemd/system/{OLD_UNIT}.service");
    if !std::path::Path::new(&path).exists() {
        return Ok(false);
    }
    // It may already be stopped or broken; removing it is what matters.
    let _ = host::systemctl(&["disable", "--now", OLD_UNIT]);
    fs::remove_file(&path).with_ctx(|| format!("remove {path}"))?;
    println!("removed {path}");
    Ok(true)
}
