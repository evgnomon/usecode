// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Orchestrates the WireGuard interface and, on any host that has
//! forward rules, the DNAT rules that carry traffic into the mesh.
//!
//! A host gets the daemon before it gets the mesh, so none of this is
//! allowed to fail just because a prerequisite isn't there yet: the mesh
//! being off, no private key yet, or wireguard-tools / iptables missing
//! makes the step that needs it a no-op with a note. Only something
//! going wrong in a step that could run is an error. Getting those
//! prerequisites onto the host is the daemon's mesh module
//! ([`crate::daemon::mesh`]); this is only the tunnel and the rules.

use std::fs;
use std::path::Path;

use crate::config::Config;
use crate::daemon::host::missing;
use crate::error::{Context, Result};
use crate::{iptables, keys, wg};

/// How far [`apply`] got.
#[derive(Debug, PartialEq, Eq)]
pub enum Progress {
    /// Everything the config asks for is in place.
    Done,
    /// Something it needs isn't there yet; the note says what.
    Waiting(String),
}

/// Bring the tunnel up, and apply DNAT/forwarding rules if `cfg` has any
/// forward-rule services ([`Config::forwards`]), printing a note for
/// whatever is skipped.
pub fn up(cfg: &Config) -> Result<()> {
    if let Progress::Waiting(note) = apply(cfg)? {
        eprintln!("{note}");
    }
    Ok(())
}

/// [`up`], reporting rather than printing whether everything is in
/// place, so the daemon knows to try again later.
pub fn apply(cfg: &Config) -> Result<Progress> {
    if !cfg.mesh_enabled() {
        // It may have been on before - don't leave its forwards behind.
        flush_forwards()?;
        return Ok(Progress::Done);
    }
    if !Path::new(keys::PRIVATE_KEY_PATH).exists() {
        return Ok(Progress::Waiting(format!(
            "no private key at {} yet; skipping the mesh (`uc net mesh apply` puts it there)",
            keys::PRIVATE_KEY_PATH
        )));
    }
    if let Some(tool) = missing(&["ip", "wg"]) {
        return Ok(Progress::Waiting(format!(
            "{tool} is not installed (wireguard-tools, iproute2); skipping the mesh"
        )));
    }

    wg::up(cfg)?;

    if cfg.forwards().is_empty() {
        // Nothing to forward (any more) - make sure a stale ruleset from
        // a previous config isn't left behind.
        flush_forwards()?;
        return Ok(Progress::Done);
    }
    if missing(&["iptables"]).is_some() {
        return Ok(Progress::Waiting(
            "iptables is not installed; skipping the port forwards".to_string(),
        ));
    }
    enable_ip_forwarding()?;
    iptables::apply(cfg)?;
    Ok(Progress::Done)
}

/// Tear down the DNAT rules (if any) and the WireGuard interface.
pub fn down(cfg: &Config) -> Result<()> {
    flush_forwards()?;
    if !cfg.mesh_enabled() || missing(&["ip"]).is_some() {
        return Ok(());
    }
    wg::down(&cfg.interface.name)
}

/// Remove this daemon's DNAT rules, if iptables is there to have any.
fn flush_forwards() -> Result<()> {
    if missing(&["iptables"]).is_some() {
        return Ok(());
    }
    iptables::flush()
}

/// Reapply the WireGuard peer/service configuration without tearing down
/// the interface, e.g. after `uc net mesh import`/`forward`.
pub fn reload(cfg: &Config) -> Result<()> {
    up(cfg)
}

/// The WireGuard link state and, if this host has forward rules, the
/// active DNAT ruleset. The text is returned even on failure, since it
/// is what the caller prints.
pub fn status(cfg: &Config) -> (String, Result<()>) {
    if !cfg.mesh_enabled() {
        return ("mesh: off\n".to_string(), Ok(()));
    }
    let (mut out, res) = wg::status(&cfg.interface.name);
    if res.is_err() {
        return (out, res);
    }

    if !cfg.forwards().is_empty()
        && let Ok(rules) = iptables::ruleset()
    {
        out.push('\n');
        out.push_str(&rules);
    }

    (out, Ok(()))
}

/// Turn on net.ipv4.ip_forward, which is required to route DNAT'd
/// traffic on to WireGuard peers.
fn enable_ip_forwarding() -> Result<()> {
    const PATH: &str = "/proc/sys/net/ipv4/ip_forward";
    fs::write(PATH, b"1\n").with_ctx(|| format!("enable ip forwarding ({PATH})"))
}
