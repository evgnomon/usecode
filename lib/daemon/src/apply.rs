// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc net mesh apply [NAME...]`: hands every mesh member (or just the
//! ones named) what the topology says it should have - its keypair and a
//! config.toml with every other member as a peer - by running
//! `usecoded join` on it. The daemon there reloads and its mesh module
//! sets the mesh up; this only delivers.
//!
//! A member that can't be reached doesn't stop the others: it is
//! reported at the end, and picks the topology up on the next apply.

use crate::bundle::Bundle;
use crate::error::Result;
use crate::inventory::{self, Inventory, Vault};
use crate::remote::Target;

pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a.starts_with('-')) {
        bail!("usage: uc net mesh apply [NAME...]");
    }
    let inv = Inventory::load(&inventory::find()?)?;
    inv.check_addresses()?;

    let names: Vec<String> = if args.is_empty() {
        inv.members().map(|h| h.name.clone()).collect()
    } else {
        args.to_vec()
    };
    if names.is_empty() {
        println!("no host has the mesh on yet; `uc net mesh add NAME` adds one");
        return Ok(());
    }

    let private_keys = Vault::new(&inv, "").private_keys()?;
    // Every bundle is worked out before any host is touched, so a
    // topology problem stops the run before half the mesh has it.
    let bundles = names
        .iter()
        .map(|n| Bundle::mesh_for_host(&inv, &private_keys, n))
        .collect::<Result<Vec<_>>>()?;

    let mut failed = Vec::new();
    for bundle in &bundles {
        println!("\n== {} ==", bundle.host);
        if let Err(e) = deliver(&inv, bundle) {
            eprintln!("{}: {e}", bundle.host);
            failed.push(bundle.host.as_str());
        }
    }
    if !failed.is_empty() {
        bail!(
            "not applied on {}; the others have the new topology. Run `uc net mesh apply {}` \
             again once they are reachable",
            failed.join(", "),
            failed.join(" ")
        );
    }
    Ok(())
}

fn deliver(inv: &Inventory, bundle: &Bundle) -> Result<()> {
    let host = inv
        .host(&bundle.host)
        .expect("bundles are for inventory hosts");
    bundle.deliver(&Target::for_host(host)?)
}
