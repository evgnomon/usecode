// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc daemon install`: puts the usecode daemon on hosts from the
//! control node.
//!
//! It only puts the daemon on the host - no mesh, no keys, no vault. For
//! each host it builds usecoded for the host's architecture, copies it
//! over and runs `usecoded setup` there, which installs the binary and
//! the systemd unit (see [`crate::setup`]). Whatever the mesh needs is
//! the daemon's own business once it runs.
//!
//! A new host goes into the inventory with only how to reach it over
//! ssh. Running it again for a host that is already installed just
//! reinstalls, and `--all` does that for every host - e.g. to roll out
//! a new version of the daemon.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Context, Result};
use crate::inventory::{self, Host, Inventory, NewHost};
use crate::remote::Target;

const USAGE: &str = "usage: uc daemon install NAME [[USER@]SSH_HOST]
       uc daemon install --all

Install the usecode daemon (binary and systemd service) on a host with
systemd. That is all it does: the daemon starts with the mesh off, and
sets the mesh up itself once `uc net mesh add NAME` and
`uc net mesh apply` have given it a config.

  NAME      the host's name (and its name in the inventory)
  SSH_HOST  where it is reached over ssh; USER defaults to root. Leave
            it out when NAME is already an ssh alias (e.g. one written
            by uc cloud / uc-agent-mcp)
  --all     reinstall every host in the inventory, e.g. to roll out a
            new version of the daemon

Run it from inside a usecode checkout. Running it again for a host that
is already installed reinstalls it.";

pub const SUMMARY: &str = "install the usecode daemon on a host with systemd";

pub fn run(args: &[String]) -> Result<()> {
    match args {
        [flag] if flag == "--summary" => {
            println!("{SUMMARY}");
            return Ok(());
        }
        [flag] if matches!(flag.as_str(), "-h" | "--help" | "help") => {
            println!("{USAGE}");
            return Ok(());
        }
        _ => {}
    }

    let mut inv = Inventory::load(&inventory::find()?)?;
    let names: Vec<String> = match args {
        [flag] if flag == "--all" => inv.hosts.iter().map(|h| h.name.clone()).collect(),
        [n] => vec![record(&mut inv, n, n)?],
        [n, t] => vec![record(&mut inv, n, t)?],
        _ => bail!("{USAGE}"),
    };

    let mut builds = Builds::default();
    for name in &names {
        let host = inv.host(name).expect("selected from the inventory");
        install(&inv.root, &mut builds, host).with_ctx(|| format!("install {name}"))?;
        if !host.mesh_enabled {
            println!("{name} has the daemon, with the mesh off. To add it: uc net mesh add {name}");
        }
    }
    Ok(())
}

/// Make sure `name` is in the inventory, adding it if it is new, and
/// return it. A new host is refused unless it answers over ssh and runs
/// systemd, before anything is written to the inventory.
fn record(inv: &mut Inventory, name: &str, target: &str) -> Result<String> {
    if inv.host(name).is_some() {
        println!("{name} is already in the inventory; reinstalling");
        return Ok(name.to_string());
    }
    let (user, ssh_host) = target.split_once('@').unwrap_or(("root", target));
    check_target(user, ssh_host)?;
    inv.add_host(NewHost {
        name: name.to_string(),
        // An ssh alias is resolved by ssh itself; repeating it as
        // ansible_host would only hide a later change to the alias.
        ansible_host: if ssh_host == name { "" } else { ssh_host }.to_string(),
        ansible_user: user.to_string(),
        ..Default::default()
    })?;
    println!("added {name} to the inventory");
    Ok(name.to_string())
}

fn check_target(user: &str, host: &str) -> Result<()> {
    let ok = Command::new("ssh")
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=10"])
        .arg(format!("{user}@{host}"))
        .arg("test -d /run/systemd/system")
        .status()
        .ctx("run ssh")?
        .success();
    if !ok {
        bail!("{user}@{host} is not reachable over ssh, or does not run systemd");
    }
    Ok(())
}

/// Copy usecoded to `host` and run setup there.
fn install(root: &Path, builds: &mut Builds, host: &Host) -> Result<()> {
    println!(
        "
== {} ==",
        host.name
    );
    let target = Target::for_host(host)?;

    let arch = target.output("uname -m")?;
    let binary = builds.get(root, &arch)?;

    let dir = target.stage(&[("usecoded", binary.as_path())])?;
    let result = target.run_as_root(&[format!("{dir}/usecoded"), "setup".to_string()]);
    target.remove_all(&dir);
    result
}

/// usecoded builds, one per target architecture, so installing on
/// twenty hosts doesn't mean twenty compiles.
#[derive(Default)]
struct Builds(BTreeMap<String, PathBuf>);

impl Builds {
    /// A static (musl) usecoded for `arch` as `uname -m` reports it,
    /// so it runs on a target whose libc is older than the control
    /// node's. Cargo's own incremental state in the per-architecture
    /// cache under ~/.cache makes a rebuild cheap across runs too.
    fn get(&mut self, root: &Path, arch: &str) -> Result<PathBuf> {
        if let Some(path) = self.0.get(arch) {
            return Ok(path.clone());
        }
        let triple = match arch {
            "x86_64" => "x86_64-unknown-linux-musl",
            "aarch64" => "aarch64-unknown-linux-musl",
            other => bail!("no build for a {other} host (x86_64 and aarch64 are supported)"),
        };
        let home = std::env::var_os("HOME").ok_or_else(|| err!("HOME is not set"))?;
        let cache = Path::new(&home).join(".cache/uc-daemon/build").join(arch);

        println!("building usecoded for {triple}");
        let status = Command::new("cargo")
            .args([
                "build",
                "--release",
                "--quiet",
                "-p",
                "uc-daemon",
                "--bin",
                "usecoded",
            ])
            .args(["--target", triple])
            .arg("--target-dir")
            .arg(&cache)
            .current_dir(root)
            .status()
            .ctx("run cargo")?;
        if !status.success() {
            bail!(
                "cargo build for {triple} failed ({status}); the target may need \
                 `rustup target add {triple}`"
            );
        }

        let path = cache.join(triple).join("release/usecoded");
        self.0.insert(arch.to_string(), path.clone());
        Ok(path)
    }
}
