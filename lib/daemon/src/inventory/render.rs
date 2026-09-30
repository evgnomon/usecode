// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Derives one host's config.toml from the whole topology. Nothing here
//! is decided per host: the address and key were recorded by
//! `uc net mesh add`, and every other mesh member becomes a `[[peer]]`,
//! so the two sides of a link are always built from the same facts.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::config::{Config, Interface, Peer, Service};
use crate::error::{Context, Result};
use crate::inventory::{Host, Inventory};

/// Fallbacks for the mesh-wide inputs an inventory may leave out.
const DEFAULT_INTERFACE: &str = "wg-uc";
const DEFAULT_LISTEN_PORT: i64 = 51820;
const DEFAULT_KEEPALIVE: i64 = 25;

/// One entry of `usecode_services` in host_vars, as written by hand. A
/// forward names the mesh member it targets (`peer`), and that is
/// resolved to its address here, so no address is ever repeated by hand;
/// `client_address` is for pointing somewhere outside the mesh.
#[derive(Debug, Deserialize)]
struct ServiceVars {
    name: String,
    #[serde(default)]
    protocol: String,
    local_port: i64,
    #[serde(default)]
    remote_bind: String,
    #[serde(default)]
    peer: String,
    #[serde(default)]
    client_address: String,
}

impl Inventory {
    /// The hosts with the mesh on - the ones that peer with each other.
    pub fn members(&self) -> impl Iterator<Item = &Host> {
        self.hosts.iter().filter(|h| h.mesh_enabled)
    }

    /// The WireGuard port every member listens on.
    pub fn listen_port(&self) -> i64 {
        match self.settings.listen_port {
            0 => DEFAULT_LISTEN_PORT,
            p => p,
        }
    }

    /// Refuse a topology in which two members claim the same tunnel
    /// address - the clash this whole arrangement exists to prevent, and
    /// one a hand-edited host_vars file could still introduce.
    pub fn check_addresses(&self) -> Result<()> {
        let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
        for h in self.members() {
            if let Some(other) = seen.insert(&h.address, &h.name) {
                bail!(
                    "{other} and {} both claim the tunnel address {}; fix host_vars before \
                     installing",
                    h.name,
                    h.address
                );
            }
        }
        Ok(())
    }

    /// The config.toml for mesh member `name`: its own interface, a
    /// `[[peer]]` for every other member, and its services.
    pub fn config_for(&self, name: &str) -> Result<Config> {
        let host = self
            .host(name)
            .ok_or_else(|| err!("{name} is not in the inventory"))?;
        if host.address.is_empty() || host.public_key.is_empty() {
            bail!(
                "{name} has no usecode_address/usecode_public_key. Hosts join the mesh with \
                 `uc net mesh add {name}`, which allocates the address and mints the keypair - \
                 don't write host_vars by hand."
            );
        }

        let keepalive = self
            .settings
            .persistent_keepalive
            .unwrap_or(DEFAULT_KEEPALIVE);
        let interface = match self.settings.interface.as_str() {
            "" => DEFAULT_INTERFACE,
            s => s,
        };

        let mut cfg = Config {
            interface: Interface {
                name: interface.to_string(),
                address: format!("{}/{}", host.address, self.prefix()?),
                listen_port: self.listen_port(),
                mtu: self.settings.mtu.max(0),
            },
            ..Config::default()
        };

        // Only a peer that publishes an endpoint can be dialled; towards
        // those, and only those, this host keeps its NAT mapping alive.
        for peer in self.members().filter(|p| p.name != name) {
            let dials = !peer.endpoint.is_empty();
            cfg.peers.push(Peer {
                name: peer.name.clone(),
                public_key: peer.public_key.clone(),
                endpoint: peer.endpoint.clone(),
                allowed_ips: vec![format!("{}/32", peer.address)],
                persistent_keepalive: if dials { keepalive } else { 0 },
                ..Peer::default()
            });
        }

        for (i, value) in host.services.iter().enumerate() {
            let svc: ServiceVars = serde_yaml::from_value(value.clone())
                .with_ctx(|| format!("{name}: usecode_services[{i}]"))?;
            cfg.services.push(self.service(name, svc)?);
        }

        cfg.validate().ctx(name)?;
        Ok(cfg)
    }

    fn service(&self, host: &str, svc: ServiceVars) -> Result<Service> {
        let client_address = match (svc.remote_bind.is_empty(), svc.client_address.is_empty()) {
            (true, _) => String::new(),
            (false, false) => svc.client_address,
            (false, true) => {
                let target = self
                    .host(&svc.peer)
                    .filter(|p| !p.address.is_empty())
                    .ok_or_else(|| {
                        err!(
                            "{host}: service {:?} forwards to peer {:?}, which is not a mesh \
                             member",
                            svc.name,
                            svc.peer
                        )
                    })?;
                target.address.clone()
            }
        };
        Ok(Service {
            name: svc.name,
            protocol: match svc.protocol.as_str() {
                "" => "tcp".to_string(),
                p => p.to_string(),
            },
            remote_bind: svc.remote_bind,
            client_address,
            local_port: svc.local_port,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::inventory::tests::new_inventory;
    use std::collections::BTreeMap;

    const SETTINGS: &str = "usecode_network: 10.10.0.0/24\nusecode_interface: wg-usecode\n";

    fn member(address: &str, extra: &str) -> String {
        format!(
            "usecode_mesh_enabled: true\nusecode_address: {address}\nusecode_public_key: \
             key-{address}\n{extra}"
        )
    }

    #[test]
    fn every_other_member_is_a_peer_and_only_endpoints_get_keepalive() {
        let vars = BTreeMap::from([
            (
                "edge",
                member("10.10.0.1", "usecode_endpoint: edge.example:51820\n"),
            ),
            ("laptop", member("10.10.0.2", "")),
            ("phone", member("10.10.0.3", "")),
            ("off", "ansible_user: root\n".to_string()),
        ]);
        let (_tmp, inv) = new_inventory(SETTINGS, &vars);

        let cfg = inv.config_for("laptop").unwrap();
        assert_eq!(cfg.interface.name, "wg-usecode");
        assert_eq!(cfg.interface.address, "10.10.0.2/24");
        let names: Vec<&str> = cfg.peers.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["edge", "phone"]);
        assert_eq!(cfg.peers[0].persistent_keepalive, 25);
        assert_eq!(cfg.peers[1].persistent_keepalive, 0);
        assert_eq!(cfg.peers[1].allowed_ips, ["10.10.0.3/32"]);
    }

    #[test]
    fn a_forward_resolves_its_peer_to_an_address() {
        let services = "usecode_services:\n  - name: web\n    remote_bind: \"0.0.0.0:80\"\n    \
                        peer: laptop\n    local_port: 8080\n";
        let vars = BTreeMap::from([
            ("edge", member("10.10.0.1", services)),
            ("laptop", member("10.10.0.2", "")),
        ]);
        let (_tmp, inv) = new_inventory(SETTINGS, &vars);

        let cfg = inv.config_for("edge").unwrap();
        assert_eq!(cfg.services[0].client_address, "10.10.0.2");
        assert_eq!(cfg.services[0].protocol, "tcp");
        assert_eq!(cfg.forwards().len(), 1);
    }

    #[test]
    fn a_forward_to_a_stranger_is_refused() {
        let services = "usecode_services:\n  - name: web\n    remote_bind: \"0.0.0.0:80\"\n    \
                        peer: nobody\n    local_port: 8080\n";
        let vars = BTreeMap::from([("edge", member("10.10.0.1", services))]);
        let (_tmp, inv) = new_inventory(SETTINGS, &vars);
        let err = inv.config_for("edge").unwrap_err().to_string();
        assert!(err.contains("not a mesh member"), "{err}");
    }

    #[test]
    fn two_members_on_one_address_are_refused() {
        let vars = BTreeMap::from([
            ("a", member("10.10.0.1", "")),
            ("b", member("10.10.0.1", "")),
        ]);
        let (_tmp, inv) = new_inventory(SETTINGS, &vars);
        assert!(inv.check_addresses().is_err());
    }
}
