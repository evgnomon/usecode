//! Loads, validates, and mutates the uc daemon configuration file. There
//! is no client/server "mode": every host runs the same commands, and a
//! host's role falls out of what its config contains - a `[[service]]`
//! with only `local_port` is something this host runs; a `[[service]]`
//! with `remote_bind` + `client_address` is something this host forwards
//! to a peer.

use std::fs;
use std::io::Write;
use std::net::IpAddr;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Context, Error, Result};
use crate::net::{parse_cidr, split_host_port};

/// The well-known location for the uc daemon config file.
pub const DEFAULT_PATH: &str = "/etc/uc/config.toml";

/// The local WireGuard interface settings. The private key is
/// deliberately not part of the config file: uc daemon generates and
/// persists it itself (see [`crate::keys`]) so it never has to be typed,
/// pasted, or committed anywhere.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Interface {
    #[serde(rename = "interface", default)]
    pub name: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub listen_port: i64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub mtu: i64,
}

/// A remote WireGuard peer. Peers are normally added with
/// `uc daemon import`, not hand-edited.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Peer {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub public_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub preshared_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub endpoint: String,
    #[serde(default)]
    pub allowed_ips: Vec<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub persistent_keepalive: i64,
}

/// Either a local declaration ("I run this on `local_port`") or a
/// forward rule ("public traffic on `remote_bind` goes to
/// `client_address:local_port`"), set with `uc daemon forward` /
/// `uc daemon unforward`. Which one it is follows from whether
/// `remote_bind` is set - there is no separate flag for it.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Service {
    #[serde(default)]
    pub name: String,
    /// "tcp" or "udp", default "tcp".
    #[serde(default)]
    pub protocol: String,
    /// e.g. "0.0.0.0:443" (only set on a forward rule).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub remote_bind: String,
    /// WireGuard address of the peer being forwarded to.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_address: String,
    /// The port this rule ultimately targets.
    #[serde(default)]
    pub local_port: i64,
}

impl Service {
    /// Whether this is a forward rule (public bind -> a peer's port)
    /// rather than a plain local service declaration.
    pub fn is_forward(&self) -> bool {
        !self.remote_bind.is_empty()
    }

    /// The service's protocol, defaulting to tcp.
    pub fn protocol_or_default(&self) -> &str {
        if self.protocol.is_empty() {
            "tcp"
        } else {
            &self.protocol
        }
    }
}

/// The root uc daemon configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(rename = "wireguard", default)]
    pub interface: Interface,
    #[serde(rename = "peer", default, skip_serializing_if = "Vec::is_empty")]
    pub peers: Vec<Peer>,
    #[serde(rename = "service", default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<Service>,
}

/// The small, non-secret bundle a host hands to another host so it can
/// be added as a peer, via `uc daemon export` / `uc daemon import`. It never
/// contains a private key.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Descriptor {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub public_key: String,
    /// This host's bare WireGuard IP (no prefix length).
    #[serde(default)]
    pub address: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub endpoint: String,
}

fn is_zero(n: &i64) -> bool {
    *n == 0
}

/// Written to the config path when no file exists yet. It is a starting
/// point, not a working config.
const DEFAULT_TEMPLATE: &str = r#"# uc daemon config - run "uc daemon export" to fill this in, then "uc daemon
# import" to add peers and "uc daemon forward" to declare services. See
# README.md for a worked recipe.

[wireguard]
interface = "wg-uc"
address   = "10.10.0.2/24" # this host's WireGuard address (CIDR)
"#;

impl Config {
    /// Read and validate the config file at `path`. If no file exists
    /// there, write a commented-out template (root-only) and return an
    /// error asking the caller to run `uc daemon export` first, rather
    /// than failing with a bare "no such file". Refuses to load a file
    /// that is readable or writable by anyone other than its owner,
    /// since it may contain WireGuard preshared keys.
    pub fn load(path: &str) -> Result<Config> {
        if ensure_default(path)? {
            bail!(
                "no config found; wrote a template to {path} - run `uc daemon export` to fill in \
                 this host's identity, then `uc daemon import` to add peers"
            );
        }

        check_permissions(path)?;

        let body = fs::read_to_string(path).with_ctx(|| format!("read {path}"))?;
        let cfg: Config = toml::from_str(&body).with_ctx(|| format!("parse {path}"))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Create a minimal config with just the wireguard interface section
    /// (no peers, no services) and save it to `path`. Fails if a config
    /// already exists there - use [`Config::load`] in that case. This is
    /// what `uc daemon export` calls the first time it runs on a host.
    pub fn new_at(path: &str, iface: &str, address: &str, listen_port: i64) -> Result<Config> {
        if Path::new(path).exists() {
            bail!("{path} already exists");
        }

        let cfg = Config {
            interface: Interface {
                name: iface.to_string(),
                address: address.to_string(),
                listen_port,
                mtu: 0,
            },
            ..Config::default()
        };
        parse_cidr(address)
            .with_ctx(|| format!("address {address:?} must be a CIDR (e.g. 10.10.0.2/24)"))?;
        cfg.save(path)?;
        Ok(cfg)
    }

    /// Validate and write the config back to `path`, root-only. Hand
    /// edits and comments in an existing file are lost once this is
    /// called - after the first `uc daemon export`/`import`/`forward`, the
    /// config file is owned by uc daemon's own commands.
    pub fn save(&self, path: &str) -> Result<()> {
        self.validate()?;

        ensure_private_dir(path)?;

        let body = toml::to_string(self).with_ctx(|| format!("encode {path}"))?;

        let tmp = format!("{path}.tmp");
        let write = || -> Result<()> {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)?;
            f.write_all(body.as_bytes())?;
            f.sync_all()?;
            Ok(())
        };
        if let Err(e) = write() {
            let _ = fs::remove_file(&tmp);
            return Err(Error(format!("write {path}: {e}")));
        }
        if let Err(e) = fs::rename(&tmp, path) {
            let _ = fs::remove_file(&tmp);
            return Err(Error(format!("write {path}: {e}")));
        }
        Ok(())
    }

    /// Add `p`, or replace the existing peer with the same name if one
    /// exists (re-importing a descriptor updates it in place instead of
    /// duplicating it).
    pub fn add_or_replace_peer(&mut self, p: Peer) {
        match self.peers.iter_mut().find(|e| e.name == p.name) {
            Some(slot) => *slot = p,
            None => self.peers.push(p),
        }
    }

    /// Add `s`, or replace the existing service with the same name if
    /// one exists (`uc daemon forward` on an existing name repoints it
    /// instead of duplicating it).
    pub fn add_or_replace_service(&mut self, s: Service) {
        match self.services.iter_mut().find(|e| e.name == s.name) {
            Some(slot) => *slot = s,
            None => self.services.push(s),
        }
    }

    /// Delete the service named `name`, reporting whether anything was
    /// removed.
    pub fn remove_service(&mut self, name: &str) -> bool {
        let before = self.services.len();
        if let Some(i) = self.services.iter().position(|s| s.name == name) {
            self.services.remove(i);
        }
        self.services.len() != before
    }

    /// Look up a peer by name.
    pub fn peer_by_name(&self, name: &str) -> Option<&Peer> {
        self.peers.iter().find(|p| p.name == name)
    }

    /// The services that are forward rules (`remote_bind` set). A
    /// non-empty result is what makes this host run DNAT/IP-forwarding.
    pub fn forwards(&self) -> Vec<&Service> {
        self.services.iter().filter(|s| s.is_forward()).collect()
    }

    /// This host's bare WireGuard IP (the CIDR in `wireguard.address`
    /// without its prefix length).
    pub fn address(&self) -> Result<String> {
        let (ip, _) = parse_cidr(&self.interface.address).with_ctx(|| {
            format!(
                "wireguard.address {:?} must be a CIDR",
                self.interface.address
            )
        })?;
        Ok(ip.to_string())
    }

    /// Check that the config is internally consistent. There is no mode
    /// to check against: every field is validated on its own terms, and
    /// a config with zero peers or zero services is valid (a freshly
    /// exported host that hasn't imported or forwarded anything yet).
    pub fn validate(&self) -> Result<()> {
        if self.interface.name.is_empty() {
            bail!("wireguard.interface is required");
        }
        if self.interface.address.is_empty() {
            bail!("wireguard.address is required");
        }
        parse_cidr(&self.interface.address).with_ctx(|| {
            format!(
                "wireguard.address {:?} must be a CIDR (e.g. 10.10.0.2/24)",
                self.interface.address
            )
        })?;

        let mut errs: Vec<String> = Vec::new();
        for (i, p) in self.peers.iter().enumerate() {
            if p.name.is_empty() {
                errs.push(format!("peer[{i}]: name is required"));
            }
            if p.public_key.is_empty() {
                errs.push(format!("peer[{i}]: public_key is required"));
            }
            if p.allowed_ips.is_empty() {
                errs.push(format!("peer[{i}]: allowed_ips is required"));
            }
        }

        for (i, s) in self.services.iter().enumerate() {
            if s.name.is_empty() {
                errs.push(format!("service[{i}]: name is required"));
            }
            if !matches!(s.protocol.as_str(), "" | "tcp" | "udp") {
                errs.push(format!("service[{i}]: protocol must be \"tcp\" or \"udp\""));
            }
            if s.local_port == 0 {
                errs.push(format!("service[{i}]: local_port is required"));
            }
            let (has_bind, has_addr) = (!s.remote_bind.is_empty(), !s.client_address.is_empty());
            if has_bind != has_addr {
                errs.push(format!(
                    "service[{i}]: remote_bind and client_address must be set together (it's a \
                     forward rule) or both empty (it's a local declaration)"
                ));
            }
            if has_bind {
                if let Err(e) = split_host_port(&s.remote_bind) {
                    errs.push(format!(
                        "service[{i}]: remote_bind {:?} must be host:port: {e}",
                        s.remote_bind
                    ));
                }
            }
            if has_addr && s.client_address.parse::<IpAddr>().is_err() {
                errs.push(format!(
                    "service[{i}]: client_address {:?} is not a valid IP",
                    s.client_address
                ));
            }
        }

        join_errs(errs)
    }
}

fn join_errs(errs: Vec<String>) -> Result<()> {
    if errs.is_empty() {
        return Ok(());
    }
    let mut b = format!("{} config error(s):", errs.len());
    for e in &errs {
        b.push_str(&format!("\n  - {e}"));
    }
    Err(Error(b))
}

/// Make sure the directory holding the config exists. A directory
/// uc daemon creates is root-only from the start; one that already exists
/// is left at whatever mode its owner chose (the Ansible role gives
/// /etc/uc 0750 so a group can list it).
fn ensure_private_dir(path: &str) -> Result<()> {
    let dir = Path::new(path).parent().unwrap_or(Path::new("."));
    if dir.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(dir)
        .and_then(|_| fs::set_permissions(dir, fs::Permissions::from_mode(0o700)))
        .with_ctx(|| format!("create {}", dir.display()))
}

/// Write [`DEFAULT_TEMPLATE`] to `path`, root-only, if no file exists
/// there yet. Reports whether it created the file.
fn ensure_default(path: &str) -> Result<bool> {
    match fs::metadata(path) {
        Ok(_) => return Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => bail!("stat {path}: {e}"),
    }

    ensure_private_dir(path)?;

    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_ctx(|| format!("write {path}"))?;
    f.write_all(DEFAULT_TEMPLATE.as_bytes())
        .with_ctx(|| format!("write {path}"))?;

    Ok(true)
}

/// Reject config files that are group- or world-readable, since they
/// contain secret key material that must stay root-only.
fn check_permissions(path: &str) -> Result<()> {
    let info = fs::metadata(path).with_ctx(|| format!("stat {path}"))?;
    let perm = info.permissions().mode() & 0o7777;
    if perm & 0o077 != 0 {
        bail!(
            "refusing to load {path}: mode {perm:04o} is accessible to group/other; \
             run `chmod 600 {path}`"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Config {
        Config {
            interface: Interface {
                name: "wg-uc".into(),
                address: "10.10.0.2/24".into(),
                listen_port: 51820,
                mtu: 0,
            },
            ..Config::default()
        }
    }

    #[test]
    fn a_host_with_no_peers_is_valid() {
        base().validate().unwrap();
    }

    #[test]
    fn a_forward_needs_both_halves() {
        let mut cfg = base();
        cfg.services.push(Service {
            name: "web".into(),
            protocol: "tcp".into(),
            remote_bind: "0.0.0.0:443".into(),
            client_address: String::new(),
            local_port: 8443,
        });
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("must be set together"), "{err}");
    }

    #[test]
    fn round_trips_through_toml() {
        let mut cfg = base();
        cfg.add_or_replace_peer(Peer {
            name: "shadow".into(),
            public_key: "abc=".into(),
            allowed_ips: vec!["10.10.0.1/32".into()],
            persistent_keepalive: 25,
            ..Peer::default()
        });
        cfg.add_or_replace_service(Service {
            name: "web".into(),
            protocol: "tcp".into(),
            remote_bind: "0.0.0.0:443".into(),
            client_address: "10.10.0.1".into(),
            local_port: 8443,
        });

        let text = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        back.validate().unwrap();
        assert_eq!(back.peers.len(), 1);
        assert_eq!(back.forwards().len(), 1);
        assert_eq!(back.address().unwrap(), "10.10.0.2");
        assert!(!text.contains("preshared_key"), "{text}");
    }

    #[test]
    fn replacing_a_peer_keeps_one_entry() {
        let mut cfg = base();
        for key in ["one=", "two="] {
            cfg.add_or_replace_peer(Peer {
                name: "shadow".into(),
                public_key: key.into(),
                allowed_ips: vec!["10.10.0.1/32".into()],
                ..Peer::default()
            });
        }
        assert_eq!(cfg.peers.len(), 1);
        assert_eq!(cfg.peers[0].public_key, "two=");
    }

    #[test]
    fn removing_a_service_reports_whether_it_existed() {
        let mut cfg = base();
        cfg.add_or_replace_service(Service {
            name: "web".into(),
            local_port: 80,
            ..Service::default()
        });
        assert!(cfg.remove_service("web"));
        assert!(!cfg.remove_service("web"));
    }
}
