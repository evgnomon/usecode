// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The host firewall, as a daemon module.
//!
//! By default a host closes every inbound port except SSH: the settings
//! file [`crate::firewall::SETTINGS_PATH`] does not have to exist for the
//! policy to be in force. It is read here, turned into one iptables chain
//! ([`crate::firewall::CHAIN`]) the module owns in both IPv4 and IPv6,
//! and reapplied whenever the file or the mesh config changes. `uc net
//! firewall` writes the file on the host and reloads the daemon, which
//! converges this module.
//!
//! A host that is in the mesh keeps working: the WireGuard listen port
//! and the tunnel interface are always allowed, next to SSH.
//!
//! The module also makes sure nothing else on the host will flush the
//! rules out from under it: because iptables here is the nftables
//! backend, an enabled `nftables.service` (which loads a config that
//! usually starts with `flush ruleset`) would wipe them, so it is
//! disabled. The daemon is the firewall's owner now, and reapplies on
//! every boot.

use std::fs;
use std::process::Command;
use std::time::SystemTime;

use crate::config::{self, Config};
use crate::daemon::{Module, host};
use crate::error::Result;
use crate::firewall::{self as fw, Settings};

#[derive(Default)]
pub struct Firewall {
    /// The settings and config as they were when the firewall last
    /// converged; `None` while it hasn't, so every tick tries again.
    converged: Option<Stamp>,
}

impl Module for Firewall {
    fn name(&self) -> &'static str {
        "firewall"
    }

    fn reconcile(&mut self, force: bool) -> Result<()> {
        let stamp = Stamp::now();
        if !force && self.converged.as_ref() == Some(&stamp) {
            return Ok(());
        }
        self.converged = None;

        let settings = Settings::load(fw::SETTINGS_PATH)?;
        if !settings.enabled {
            fw::flush()?;
            self.converged = Some(stamp);
            if force {
                eprintln!("firewall: off");
            }
            return Ok(());
        }

        fw::prepare()?;
        yield_conflicting_managers()?;
        let cfg = Config::load(config::DEFAULT_PATH)?;
        let ssh = fw::ssh_port(&settings);
        fw::apply(&settings, ssh, &cfg)?;

        self.converged = Some(stamp);
        eprintln!(
            "firewall: default-deny inbound (ipv4+ipv6), ssh {ssh}, {} extra rule(s)",
            settings.allow.len()
        );
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        // Leave the host reachable if the daemon goes away: the daemon is
        // what keeps the rules current, and a stale default-deny can lock
        // you out of a host whose daemon will not come back.
        fw::flush()
    }
}

/// Units that would load a ruleset of their own and wipe this module's
/// chains at boot or on restart (the canonical `nftables.conf` starts with
/// `flush ruleset`). The daemon owns the firewall now, so turn them off.
/// Nothing to do when they aren't enabled, which is the usual case.
fn yield_conflicting_managers() -> Result<()> {
    for unit in ["nftables", "netfilter-persistent", "iptables"] {
        if !enabled(unit) {
            continue;
        }
        host::systemctl(&["disable", unit])?;
        eprintln!("firewall: disabled {unit} (the daemon owns the rules now)");
    }
    Ok(())
}

fn enabled(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["is-enabled", unit])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// When the files the firewall depends on last changed.
#[derive(PartialEq, Eq)]
struct Stamp([Option<SystemTime>; 2]);

impl Stamp {
    fn now() -> Stamp {
        let mtime = |p: &str| fs::metadata(p).and_then(|m| m.modified()).ok();
        Stamp([mtime(fw::SETTINGS_PATH), mtime(config::DEFAULT_PATH)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_module_is_called_firewall() {
        assert_eq!(Firewall::default().name(), "firewall");
    }
}
