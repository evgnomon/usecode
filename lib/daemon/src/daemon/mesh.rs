// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The WireGuard mesh, as a daemon module.
//!
//! Its precondition is a config with the mesh on, in /etc/uc/config.toml,
//! and this host's private key next to it - both delivered by
//! `uc net mesh apply` (see [`accept`]). Until they are there the module
//! has nothing to do. Once they are, it installs the packages the config
//! needs, persists IP forwarding if it forwards ports, and brings the
//! tunnel and the forwards up. It converges again whenever either file
//! changes, and keeps retrying while something it needs is missing.

use std::fs;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::app::{self, Progress};
use crate::config::{self, Config};
use crate::daemon::{Module, host};
use crate::error::{Context, Result};
use crate::keys;

/// Where IPv4 forwarding is persisted for a host that forwards ports.
const SYSCTL_PATH: &str = "/etc/sysctl.d/99-uc-daemon.conf";

#[derive(Default)]
pub struct Mesh {
    /// The config and key as they were when the mesh last converged;
    /// `None` while it hasn't, so every tick tries again.
    converged: Option<Stamp>,
}

impl Module for Mesh {
    fn name(&self) -> &'static str {
        "mesh"
    }

    fn reconcile(&mut self, force: bool) -> Result<()> {
        let stamp = Stamp::now();
        if !force && self.converged.as_ref() == Some(&stamp) {
            return Ok(());
        }
        self.converged = None;

        let cfg = Config::load(config::DEFAULT_PATH)?;
        if cfg.mesh_enabled() {
            prepare(&cfg)?;
        }
        match app::apply(&cfg)? {
            Progress::Done => {
                if force || cfg.mesh_enabled() {
                    eprintln!("mesh: {}", summary(&cfg));
                }
                self.converged = Some(stamp);
            }
            Progress::Waiting(note) => eprintln!("mesh: {note}"),
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        app::down(&Config::load(config::DEFAULT_PATH)?)
    }
}

/// The packages the config needs and, with forwards, IP forwarding
/// that survives a reboot.
fn prepare(cfg: &Config) -> Result<()> {
    let mut tools = vec![("wg", "wireguard-tools"), ("ip", "iproute2")];
    if !cfg.forwards().is_empty() {
        tools.push(("iptables", "iptables"));
        host::put(SYSCTL_PATH, b"net.ipv4.ip_forward=1\n", 0o644)?;
    }
    host::ensure_tools(&tools)
}

fn summary(cfg: &Config) -> String {
    if !cfg.mesh_enabled() {
        return "off".to_string();
    }
    format!(
        "up on {} ({}, {} peers, {} forwards)",
        cfg.interface.name,
        cfg.interface.address,
        cfg.peers.len(),
        cfg.forwards().len()
    )
}

/// When the files the mesh depends on last changed.
#[derive(PartialEq, Eq)]
struct Stamp([Option<SystemTime>; 2]);

impl Stamp {
    fn now() -> Stamp {
        let mtime = |p: &str| fs::metadata(p).and_then(|m| m.modified()).ok();
        Stamp([mtime(config::DEFAULT_PATH), mtime(keys::PRIVATE_KEY_PATH)])
    }
}

/// The mesh's part of what the control node delivers to a member (see
/// [`crate::bundle`]): its keypair and its config.toml.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Delivery {
    pub private_key: String,
    pub public_key: String,
    /// The rendered config.toml.
    pub config: String,
}

impl Delivery {
    /// The config carried here, checked the way `uc net mesh validate`
    /// would check it on disk.
    pub fn parsed_config(&self) -> Result<Config> {
        let cfg: Config = toml::from_str(&self.config).ctx("parse the delivered config")?;
        cfg.validate()?;
        if !cfg.mesh_enabled() {
            bail!("the delivered config has no [wireguard] address");
        }
        Ok(cfg)
    }
}

/// Put a delivery's files in place. Reports whether anything changed;
/// the daemon picks it up on reload.
pub fn accept(d: &Delivery) -> Result<bool> {
    // Checked first, so a bad delivery never replaces a working config.
    d.parsed_config()?;

    host::ensure_dir("/etc/uc", 0o750)?;
    host::ensure_dir(keys::DIR, 0o700)?;
    let mut changed = false;
    let private_key = format!("{}\n", d.private_key.trim());
    changed |= host::put(keys::PRIVATE_KEY_PATH, private_key.as_bytes(), 0o600)?;
    let public_key = format!("{}\n", d.public_key.trim());
    changed |= host::put(keys::PUBLIC_KEY_PATH, public_key.as_bytes(), 0o600)?;
    changed |= host::put(config::DEFAULT_PATH, d.config.as_bytes(), 0o600)?;
    Ok(changed)
}
