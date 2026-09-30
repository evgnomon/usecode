// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc daemon reload`: have the daemon on hosts converge every module
//! now, from the control node.
//!
//! It runs `usecoded reload` on each host over ssh, which asks the
//! running daemon through its control socket (see
//! [`crate::daemon::control`]) and prints how each module did. A name
//! in the inventory is reached the way `uc daemon install` reaches it;
//! any other name is taken as an ssh destination, so this works outside
//! a checkout too.

use crate::error::{Context, Result};
use crate::inventory::{self, Inventory};
use crate::remote::Target;
use crate::setup;

const USAGE: &str = "usage: uc daemon reload NODE...
       uc daemon reload --all

Have the usecode daemon on each NODE converge every module now, and
print how each one did - the same as `systemctl reload usecode` there,
with an answer.

  NODE   a host in the inventory, or any ssh destination ([USER@]HOST
         or an alias from ~/.ssh/config)
  --all  every host in the inventory";

pub const SUMMARY: &str = "have the daemon on hosts converge every module now";

pub fn run(args: &[String]) -> Result<()> {
    let names: Vec<String> = match args {
        [] => bail!("{USAGE}"),
        [flag] if flag == "--summary" => {
            println!("{SUMMARY}");
            return Ok(());
        }
        [flag] if matches!(flag.as_str(), "-h" | "--help" | "help") => {
            println!("{USAGE}");
            return Ok(());
        }
        [flag] if flag == "--all" => load_inventory()
            .ctx("--all needs the inventory (run it inside a usecode checkout)")?
            .hosts
            .into_iter()
            .map(|h| h.name)
            .collect(),
        names => names.to_vec(),
    };

    let inv = load_inventory().ok();
    let mut failed = Vec::new();
    for name in &names {
        println!("== {name} ==");
        if let Err(e) = reload(inv.as_ref(), name) {
            eprintln!("{name}: {e}");
            failed.push(name.as_str());
        }
    }
    if !failed.is_empty() {
        bail!("reload failed on {}", failed.join(", "));
    }
    Ok(())
}

fn load_inventory() -> Result<Inventory> {
    Inventory::load(&inventory::find()?)
}

/// Reach `name` - through the inventory if it is there, as an ssh
/// destination if not - and reload the daemon on it.
fn reload(inv: Option<&Inventory>, name: &str) -> Result<()> {
    let target = match inv.and_then(|inv| inv.host(name)) {
        Some(host) => Target::for_host(host)?,
        None => Target::ssh(name)?,
    };
    target.run_as_root(&[setup::BINARY_PATH.to_string(), "reload".to_string()])
}
