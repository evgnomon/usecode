// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! What the control node delivers to a host that the host can't work
//! out itself, one optional section per daemon module. `usecoded join`
//! hands each section to its module ([`crate::setup::join`]), and the
//! daemon takes it from there.
//!
//! The mesh has one: a member's keypair and its config.toml, which is
//! derived from the whole topology, so it is rendered on the control
//! node, which has the inventory and the vault. `uc net mesh apply`
//! builds and delivers it.
//!
//! The firewall has one too: the host's [`crate::firewall::Settings`],
//! edited with `uc net firewall` and delivered so the daemon on the host
//! picks the policy up.
//!
//! And Kubernetes: the host's [`crate::kube::Settings`], written by
//! `uc kube` so the daemon there runs it as a k3s controller.
//!
//! And calling home: the [`crate::callhome::Settings`] `uc daemon register`
//! hands a host, a one-time token to register with usecode.dev.
//!
//! The file can hold a private key: it only ever sits in 0700
//! directories, and is removed again once join has run.

use std::collections::BTreeMap;
use std::fs;

use serde::{Deserialize, Serialize};

use crate::callhome;
use crate::daemon::mesh;
use crate::error::{Context, Result};
use crate::firewall::Settings;
use crate::inventory::{GROUP, Inventory};
use crate::kube;
use crate::remote::Target;
use crate::setup::BINARY_PATH;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Bundle {
    /// The host's inventory name, for messages.
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh: Option<mesh::Delivery>,
    /// The host's firewall settings, when `uc net firewall` delivered a
    /// new policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firewall: Option<Settings>,
    /// The host's Kubernetes settings, when `uc kube` delivered them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kube: Option<kube::Settings>,
    /// Where to call home and the token to register with, when
    /// `uc daemon register` delivered them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callhome: Option<callhome::Settings>,
}

impl Bundle {
    /// The mesh delivery for member `name`. `private_keys` is the
    /// vault's content.
    pub fn mesh_for_host(
        inv: &Inventory,
        private_keys: &BTreeMap<String, String>,
        name: &str,
    ) -> Result<Bundle> {
        let host = inv
            .host(name)
            .ok_or_else(|| err!("{name} is not in the inventory"))?;
        if !host.mesh_enabled {
            bail!("{name} is not in the mesh; `uc net mesh add {name}` puts it there");
        }

        let private_key = private_keys.get(name).cloned().unwrap_or_default();
        if private_key.is_empty() {
            bail!(
                "no private key for {name} in group_vars/{GROUP}/secrets.yml. It is written \
                 there by `uc net mesh add`; if the host predates that, re-add it or insert \
                 the key with `ansible-vault edit`."
            );
        }

        let cfg = inv.config_for(name)?;
        let body = toml::to_string(&cfg).with_ctx(|| format!("encode config for {name}"))?;
        Ok(Bundle {
            host: name.to_string(),
            mesh: Some(mesh::Delivery {
                private_key,
                public_key: host.public_key.clone(),
                config: format!("{}{body}", config_header(name)),
            }),
            ..Bundle::default()
        })
    }

    pub fn encode(&self) -> Result<String> {
        toml::to_string(self).ctx("encode bundle")
    }

    pub fn load(path: &str) -> Result<Bundle> {
        let body = fs::read_to_string(path).with_ctx(|| format!("read {path}"))?;
        toml::from_str(&body).with_ctx(|| format!("parse {path}"))
    }

    /// Hand the bundle to `usecoded join` on `target`. It may hold a
    /// secret, so it only ever sits in 0700 directories, here and there,
    /// and both go away afterwards.
    pub fn deliver(&self, target: &Target) -> Result<()> {
        let local = tempfile::Builder::new()
            .prefix("uc-bundle-")
            .tempdir()
            .ctx("create a staging directory")?;
        let path = local.path().join("bundle.toml");
        fs::write(&path, self.encode()?).ctx("write the bundle")?;

        let dir = target.stage(&[("bundle.toml", path.as_path())])?;
        let result = target
            .run_as_root(&[
                BINARY_PATH.to_string(),
                "join".to_string(),
                format!("{dir}/bundle.toml"),
            ])
            .with_ctx(|| {
                format!(
                    "join (if the daemon is missing or older than this, `uc daemon install {}` \
                     first)",
                    self.host
                )
            });
        target.remove_all(&dir);
        result
    }
}

fn config_header(name: &str) -> String {
    format!(
        "# Written by `uc net mesh apply` for {name}.\n\
         #\n\
         # Derived from the mesh topology in the inventory, not written by hand:\n\
         # the address below was allocated by `uc net mesh add`, and every [[peer]]\n\
         # is another member of the {GROUP} group. There is no private_key here by\n\
         # design - it lives root-only in {dir}/private.key.\n\n",
        dir = crate::keys::DIR,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mesh_bundle_without_a_firewall_section_still_decodes() {
        let text = "host = \"edge\"\n\
                    [mesh]\n\
                    private_key = \"k\"\n\
                    public_key = \"p\"\n\
                    config = \"\"\n";
        let bundle: Bundle = toml::from_str(text).unwrap();
        assert!(bundle.firewall.is_none());
        assert!(bundle.kube.is_none());
        assert!(bundle.mesh.is_some());
    }

    #[test]
    fn a_firewall_section_round_trips() {
        let bundle = Bundle {
            host: "edge".into(),
            mesh: None,
            firewall: Some(Settings {
                ssh_port: Some(2657),
                allow: vec!["tcp:443".into()],
                ..Settings::default()
            }),
            ..Bundle::default()
        };

        let back: Bundle = toml::from_str(&bundle.encode().unwrap()).unwrap();
        let settings = back.firewall.expect("firewall section");
        assert_eq!(settings.ssh_port, Some(2657));
        assert_eq!(settings.allow, vec!["tcp:443".to_string()]);
        assert!(settings.enabled);
    }

    #[test]
    fn a_kube_section_round_trips() {
        let bundle = Bundle {
            host: "edge".into(),
            kube: Some(kube::Settings {
                enabled: true,
                token: "t".into(),
                ..kube::Settings::default()
            }),
            ..Bundle::default()
        };
        let back: Bundle = toml::from_str(&bundle.encode().unwrap()).unwrap();
        assert_eq!(back.kube, bundle.kube);
    }

    #[test]
    fn a_callhome_section_round_trips() {
        let bundle = Bundle {
            host: "edge".into(),
            callhome: Some(callhome::Settings {
                endpoint: callhome::DEFAULT_ENDPOINT.into(),
                token: "1a2b.t".into(),
                name: "edge".into(),
            }),
            ..Bundle::default()
        };
        let back: Bundle = toml::from_str(&bundle.encode().unwrap()).unwrap();
        assert_eq!(back.callhome, bundle.callhome);
        assert!(back.mesh.is_none());
    }
}
