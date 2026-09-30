// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! What the control node delivers to a host that the host can't work
//! out itself, one optional section per daemon module. `usecoded join`
//! hands each section to its module ([`crate::setup::join`]), and the
//! daemon takes it from there.
//!
//! Today only the mesh has one: a member's keypair and its config.toml,
//! which is derived from the whole topology, so it is rendered on the
//! control node, which has the inventory and the vault. `uc net mesh
//! apply` builds and delivers it.
//!
//! The file holds a private key: it only ever sits in 0700 directories,
//! and is removed again once join has run.

use std::collections::BTreeMap;
use std::fs;

use serde::{Deserialize, Serialize};

use crate::daemon::mesh;
use crate::error::{Context, Result};
use crate::inventory::{GROUP, Inventory};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Bundle {
    /// The host's inventory name, for messages.
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh: Option<mesh::Delivery>,
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
        })
    }

    pub fn encode(&self) -> Result<String> {
        toml::to_string(self).ctx("encode bundle")
    }

    pub fn load(path: &str) -> Result<Bundle> {
        let body = fs::read_to_string(path).with_ctx(|| format!("read {path}"))?;
        toml::from_str(&body).with_ctx(|| format!("parse {path}"))
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
