// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Kubernetes on a host: the host as a k3s server (a controller), alone or
//! as one of an HA control plane, or as a k3s agent (a worker) that joins
//! a controller - and the settings file that says so.
//!
//! The settings are an ordinary file on the host, [`SETTINGS_PATH`],
//! written by `uc kube` and read by the daemon's kube module, which turns
//! them into k3s's own config file ([`K3S_CONFIG_PATH`]), installs k3s if
//! it isn't there and keeps it running. No file means Kubernetes is off.
//!
//! One controller is a cluster of its own (k3s with its built-in SQLite
//! datastore). An HA control plane is three or more: the first one is
//! started with `cluster-init` (embedded etcd), and every other one joins
//! it through [`Settings::server`] with the shared [`Settings::token`].
//! An existing single controller becomes the first of an HA control plane
//! the same way - k3s moves its data over to etcd when it restarts with
//! `cluster-init`.
//!
//! A worker ([`Settings::agent`]) runs pods and nothing else: it joins a
//! controller through [`Settings::server`] with the cluster's token, the
//! same way a controller joins the others.
//!
//! When the host is in the WireGuard mesh, the controllers talk to each
//! other over it: the node's address is its mesh address and pod traffic
//! between nodes rides the tunnel. Without the mesh, pod traffic is
//! encrypted by flannel's own WireGuard backend instead.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::{Context, Result};
use crate::net::parse_cidr;

/// Where the Kubernetes settings live on a host. Root-only: it holds the
/// cluster's join token.
pub const SETTINGS_PATH: &str = "/etc/uc/kube.toml";

/// k3s's own config file, rendered from the settings.
pub const K3S_CONFIG_PATH: &str = "/etc/rancher/k3s/config.yaml";

/// The admin kubeconfig a k3s server writes, which `uc kube connect`
/// copies to your machine.
pub const KUBECONFIG_PATH: &str = "/etc/rancher/k3s/k3s.yaml";

/// The join token a k3s server keeps, which more controllers and workers
/// need.
pub const TOKEN_PATH: &str = "/var/lib/rancher/k3s/server/token";

/// The Kubernetes API port.
pub const API_PORT: u16 = 6443;

/// The interfaces pods reach the host through (the CNI bridge and
/// flannel's tunnels), which the firewall has to let in.
const POD_INTERFACES: [&str; 3] = ["cni0", "flannel.1", "flannel-wg"];

/// What `uc kube` asks of a host.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Whether the host runs a k3s server. Off by default.
    #[serde(default)]
    pub enabled: bool,
    /// When turning it off, also uninstall k3s and remove its data.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub purge: bool,
    /// This host founds an HA control plane (embedded etcd).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cluster_init: bool,
    /// The host is a worker (a k3s agent), not a controller. It joins
    /// [`Settings::server`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub agent: bool,
    /// The API of the controller to join, e.g. `https://10.10.0.2:6443`.
    /// Empty for a single controller or the first of an HA one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub server: String,
    /// The secret every controller of the cluster shares.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
    /// Extra names and addresses the API certificate is valid for - the
    /// ones kubectl reaches it by. The mesh address is always added.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tls_san: Vec<String>,
    /// The addresses of the other nodes of the cluster, which the firewall
    /// lets in (etcd, kubelet and pod traffic between nodes).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub peers: Vec<String>,
    /// The k3s version to run, e.g. `v1.31.4+k3s1`. Empty means the
    /// current stable release, installed once and then left alone.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
}

impl Settings {
    /// Read the settings at `path`. A missing file is Kubernetes off, not
    /// an error.
    pub fn load(path: &str) -> Result<Settings> {
        let body = match std::fs::read_to_string(path) {
            Ok(body) => body,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
            Err(e) => bail!("read {path}: {e}"),
        };
        Settings::parse(&body).with_ctx(|| format!("parse {path}"))
    }

    pub fn parse(body: &str) -> Result<Settings> {
        let settings: Settings = toml::from_str(body).ctx("parse kube settings")?;
        settings.validate()?;
        Ok(settings)
    }

    /// Encode the settings the way [`SETTINGS_PATH`] holds them.
    pub fn encode(&self) -> Result<String> {
        self.validate()?;
        let body = toml::to_string(self).ctx("encode kube settings")?;
        Ok(format!(
            "# Written by `uc kube`; the usecode daemon reads it and runs k3s.\n\
             # Edit with `uc kube`, not by hand.\n\n{body}"
        ))
    }

    pub fn validate(&self) -> Result<()> {
        if self.cluster_init && !self.server.is_empty() {
            bail!("a controller either founds the cluster or joins one, not both");
        }
        if self.agent && self.cluster_init {
            bail!("a worker can't found a cluster; it joins one");
        }
        if self.agent && self.enabled && self.server.is_empty() {
            bail!("a worker needs the controller to join");
        }
        if !self.server.is_empty() && self.token.is_empty() {
            bail!("joining {} needs the cluster's token", self.server);
        }
        if !self.server.is_empty() && !self.server.starts_with("https://") {
            bail!("server {:?} must be an https:// URL", self.server);
        }
        for peer in &self.peers {
            if peer.parse::<IpAddr>().is_err() && parse_cidr(peer).is_err() {
                bail!("peer {peer:?} is not an IP address");
            }
        }
        if !self.version.is_empty() && !self.version.starts_with('v') {
            bail!("version {:?} should look like v1.31.4+k3s1", self.version);
        }
        Ok(())
    }
}

/// k3s's config file for `settings` on a host whose mesh config is `cfg`.
/// Kept pure so it can be tested without a host.
pub fn k3s_config(settings: &Settings, cfg: &Config) -> Result<String> {
    let mut map = serde_yaml::Mapping::new();
    let mut set = |k: &str, v: serde_yaml::Value| {
        map.insert(k.into(), v);
    };

    if !settings.agent {
        set("write-kubeconfig-mode", "0600".into());
    }
    if settings.cluster_init {
        set("cluster-init", true.into());
    }
    if !settings.server.is_empty() {
        set("server", settings.server.as_str().into());
    }
    if !settings.token.is_empty() {
        set("token", settings.token.as_str().into());
    }

    // A worker serves no API, so it has no certificate to name.
    let mut sans = if settings.agent {
        Vec::new()
    } else {
        settings.tls_san.clone()
    };
    if cfg.mesh_enabled() {
        let ip = cfg.address()?;
        if !settings.agent {
            sans.insert(0, ip.clone());
        }
        set("node-ip", ip.into());
        // Pod traffic between nodes rides the tunnel, which already
        // encrypts it.
        set("flannel-iface", cfg.interface.name.as_str().into());
    } else if !settings.agent {
        // The controllers pick the backend; workers follow it.
        set("flannel-backend", "wireguard-native".into());
    }
    sans.dedup();
    if !sans.is_empty() {
        set(
            "tls-san",
            serde_yaml::Value::Sequence(sans.into_iter().map(Into::into).collect()),
        );
    }

    let body = serde_yaml::to_string(&map).ctx("encode k3s config")?;
    Ok(format!(
        "# Written by the usecode daemon from {SETTINGS_PATH}; change it with\n\
         # `uc kube`, not by hand.\n{body}"
    ))
}

/// What the host firewall has to let in for a node, as rules for the
/// given family (`ipv4` true for iptables, false for ip6tables): the API
/// (on a controller), pods talking to their host, and the other nodes.
pub fn inbound_rules(settings: &Settings, ipv4: bool) -> Vec<Vec<String>> {
    if !settings.enabled {
        return Vec::new();
    }
    let mut rules = Vec::new();
    if !settings.agent {
        rules.push(words(&[
            "-p",
            "tcp",
            "--dport",
            &API_PORT.to_string(),
            "-j",
            "ACCEPT",
        ]));
    }
    for iface in POD_INTERFACES {
        rules.push(words(&["-i", iface, "-j", "ACCEPT"]));
    }
    for peer in &settings.peers {
        let addr = parse_cidr(peer)
            .map(|(ip, _)| ip)
            .or_else(|_| peer.parse::<IpAddr>());
        if addr.is_ok_and(|ip| ip.is_ipv4() == ipv4) {
            rules.push(words(&["-s", peer, "-j", "ACCEPT"]));
        }
    }
    rules
}

fn words(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

/// Put delivered settings in place on this host ([`SETTINGS_PATH`]),
/// reports whether anything changed. The daemon picks them up on reload.
pub fn accept(settings: &Settings) -> Result<bool> {
    let body = settings.encode()?;
    crate::daemon::host::put(SETTINGS_PATH, body.as_bytes(), 0o600)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Interface;

    fn mesh() -> Config {
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
    fn no_settings_means_kubernetes_is_off() {
        assert!(!Settings::default().enabled);
        assert!(inbound_rules(&Settings::default(), true).is_empty());
    }

    #[test]
    fn a_single_controller_on_the_mesh_uses_the_tunnel() {
        let settings = Settings {
            enabled: true,
            tls_san: vec!["edge.example.com".into()],
            ..Settings::default()
        };
        let yaml = k3s_config(&settings, &mesh()).unwrap();
        assert!(yaml.contains("node-ip: 10.10.0.2"), "{yaml}");
        assert!(yaml.contains("flannel-iface: wg-uc"), "{yaml}");
        assert!(yaml.contains("- 10.10.0.2"), "{yaml}");
        assert!(yaml.contains("- edge.example.com"), "{yaml}");
        assert!(!yaml.contains("cluster-init"), "{yaml}");
        assert!(!yaml.contains("flannel-backend"), "{yaml}");
    }

    #[test]
    fn without_the_mesh_pod_traffic_is_encrypted_by_flannel() {
        let settings = Settings {
            enabled: true,
            ..Settings::default()
        };
        let yaml = k3s_config(&settings, &Config::default()).unwrap();
        assert!(yaml.contains("flannel-backend: wireguard-native"), "{yaml}");
        assert!(!yaml.contains("node-ip"), "{yaml}");
    }

    #[test]
    fn an_ha_controller_joins_with_the_token() {
        let settings = Settings {
            enabled: true,
            server: "https://10.10.0.2:6443".into(),
            token: "secret".into(),
            ..Settings::default()
        };
        let yaml = k3s_config(&settings, &mesh()).unwrap();
        assert!(yaml.contains("server: https://10.10.0.2:6443"), "{yaml}");
        assert!(yaml.contains("token: secret"), "{yaml}");

        let founder = Settings {
            enabled: true,
            cluster_init: true,
            ..Settings::default()
        };
        assert!(
            k3s_config(&founder, &mesh())
                .unwrap()
                .contains("cluster-init: true")
        );
    }

    #[test]
    fn a_worker_joins_and_serves_no_api() {
        let settings = Settings {
            enabled: true,
            agent: true,
            server: "https://10.10.0.2:6443".into(),
            token: "secret".into(),
            tls_san: vec!["edge.example.com".into()],
            peers: vec!["10.10.0.2".into()],
            ..Settings::default()
        };
        let yaml = k3s_config(&settings, &mesh()).unwrap();
        assert!(yaml.contains("server: https://10.10.0.2:6443"), "{yaml}");
        assert!(yaml.contains("token: secret"), "{yaml}");
        assert!(yaml.contains("node-ip: 10.10.0.2"), "{yaml}");
        assert!(!yaml.contains("tls-san"), "{yaml}");
        assert!(!yaml.contains("write-kubeconfig-mode"), "{yaml}");

        let off_mesh = k3s_config(&settings, &Config::default()).unwrap();
        assert!(!off_mesh.contains("flannel-backend"), "{off_mesh}");

        let rules: Vec<String> = inbound_rules(&settings, true)
            .iter()
            .map(|r| r.join(" "))
            .collect();
        assert!(!rules.iter().any(|r| r.contains("6443")), "{rules:?}");
        assert!(rules.contains(&"-s 10.10.0.2 -j ACCEPT".to_string()));
    }

    #[test]
    fn bad_settings_are_refused() {
        let both = Settings {
            cluster_init: true,
            server: "https://a:6443".into(),
            token: "t".into(),
            ..Settings::default()
        };
        assert!(both.validate().is_err());
        let no_token = Settings {
            server: "https://a:6443".into(),
            ..Settings::default()
        };
        assert!(no_token.validate().is_err());
        let bad_peer = Settings {
            peers: vec!["not-an-ip".into()],
            ..Settings::default()
        };
        assert!(bad_peer.validate().is_err());
        let lonely_worker = Settings {
            enabled: true,
            agent: true,
            ..Settings::default()
        };
        assert!(lonely_worker.validate().is_err());
        let founding_worker = Settings {
            agent: true,
            cluster_init: true,
            ..Settings::default()
        };
        assert!(founding_worker.validate().is_err());
    }

    #[test]
    fn the_firewall_opens_the_api_pods_and_peers_by_family() {
        let settings = Settings {
            enabled: true,
            peers: vec!["203.0.113.7".into(), "2001:db8::7".into()],
            ..Settings::default()
        };
        let v4: Vec<String> = inbound_rules(&settings, true)
            .iter()
            .map(|r| r.join(" "))
            .collect();
        assert!(v4.contains(&"-p tcp --dport 6443 -j ACCEPT".to_string()));
        assert!(v4.contains(&"-i cni0 -j ACCEPT".to_string()));
        assert!(v4.contains(&"-s 203.0.113.7 -j ACCEPT".to_string()));
        assert!(!v4.iter().any(|r| r.contains("2001:db8::7")));

        let v6: Vec<String> = inbound_rules(&settings, false)
            .iter()
            .map(|r| r.join(" "))
            .collect();
        assert!(v6.contains(&"-s 2001:db8::7 -j ACCEPT".to_string()));
        assert!(!v6.iter().any(|r| r.contains("203.0.113.7")));
    }

    #[test]
    fn settings_round_trip_through_toml() {
        let settings = Settings {
            enabled: true,
            cluster_init: true,
            token: "t".into(),
            tls_san: vec!["edge".into()],
            peers: vec!["10.10.0.3".into()],
            version: "v1.31.4+k3s1".into(),
            ..Settings::default()
        };
        assert_eq!(
            Settings::parse(&settings.encode().unwrap()).unwrap(),
            settings
        );
    }
}
