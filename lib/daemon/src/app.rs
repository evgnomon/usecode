//! Orchestrates the WireGuard interface and, on any host that has
//! forward rules, the DNAT rules that carry traffic into the mesh.

use std::fs;

use crate::config::Config;
use crate::error::{Context, Result};
use crate::{iptables, wg};

/// Bring the tunnel up, and apply DNAT/forwarding rules if `cfg` has any
/// forward-rule services ([`Config::forwards`]).
pub fn up(cfg: &Config) -> Result<()> {
    wg::up(cfg)?;

    if !cfg.forwards().is_empty() {
        enable_ip_forwarding()?;
        iptables::apply(cfg)?;
    } else {
        // Nothing to forward (any more) - make sure a stale ruleset from
        // a previous config isn't left behind.
        iptables::flush()?;
    }

    Ok(())
}

/// Tear down the DNAT rules (if any) and the WireGuard interface.
pub fn down(cfg: &Config) -> Result<()> {
    iptables::flush()?;
    wg::down(&cfg.interface.name)
}

/// Reapply the WireGuard peer/service configuration without tearing down
/// the interface, e.g. after `uc daemon import`/`forward`.
pub fn reload(cfg: &Config) -> Result<()> {
    up(cfg)
}

/// The WireGuard link state and, if this host has forward rules, the
/// active DNAT ruleset. The text is returned even on failure, since it
/// is what the caller prints.
pub fn status(cfg: &Config) -> (String, Result<()>) {
    let (mut out, res) = wg::status(&cfg.interface.name);
    if res.is_err() {
        return (out, res);
    }

    if !cfg.forwards().is_empty() {
        if let Ok(rules) = iptables::ruleset() {
            out.push('\n');
            out.push_str(&rules);
        }
    }

    (out, Ok(()))
}

/// Turn on net.ipv4.ip_forward, which is required to route DNAT'd
/// traffic on to WireGuard peers.
fn enable_ip_forwarding() -> Result<()> {
    const PATH: &str = "/proc/sys/net/ipv4/ip_forward";
    fs::write(PATH, b"1\n").with_ctx(|| format!("enable ip forwarding ({PATH})"))
}
