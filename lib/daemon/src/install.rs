// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc daemon install`: puts the usecode daemon on hosts from the
//! control node.
//!
//! It only puts the daemon on the host - no mesh, no keys, no vault. For
//! each host it gets usecoded for the host's architecture - downloaded
//! from the GitHub release of this uc, or built when run inside a usecode
//! checkout - copies it over and runs `usecoded setup` there, which installs the binary and
//! the systemd unit (see [`crate::setup`]). Whatever the mesh needs is
//! the daemon's own business once it runs.
//!
//! A new host goes into the inventory with only how to reach it over
//! ssh. Running it again for a host that is already installed just
//! reinstalls, and `--all` does that for every host - e.g. to roll out
//! a new version of the daemon.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use sha2::{Digest, Sha256};

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
            by uc vm / uc-agent-mcp)
  --all     reinstall every host in the inventory, e.g. to roll out a
            new version of the daemon

usecoded comes from the GitHub release of this uc, checked against the
release's SHA256SUMS. Run from inside a usecode checkout, it is built
from the checkout instead. Running it again for a host that is already
installed reinstalls it.";

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

    let dir = inventory::find()?;
    inventory::init(&dir)?;
    let mut inv = Inventory::load(&dir)?;
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

/// usecoded binaries, one per target architecture, so installing on
/// twenty hosts doesn't mean twenty downloads or compiles.
#[derive(Default)]
struct Builds(BTreeMap<String, PathBuf>);

impl Builds {
    /// A static (musl) usecoded for `arch` as `uname -m` reports it, so
    /// it runs on a target whose libc is older than the control node's:
    /// built from the checkout at `root`, or downloaded when `root` is
    /// empty.
    fn get(&mut self, root: &Path, arch: &str) -> Result<PathBuf> {
        if let Some(path) = self.0.get(arch) {
            return Ok(path.clone());
        }
        if !matches!(arch, "x86_64" | "aarch64") {
            bail!("no usecoded for a {arch} host (x86_64 and aarch64 are supported)");
        }
        let home = std::env::var_os("HOME").ok_or_else(|| err!("HOME is not set"))?;
        let cache = Path::new(&home).join(".cache/uc-daemon");
        let path = if root.as_os_str().is_empty() {
            download(&cache.join("release"), arch)?
        } else {
            build(root, &cache.join("build").join(arch), arch)?
        };
        self.0.insert(arch.to_string(), path.clone());
        Ok(path)
    }
}

/// Build usecoded for `arch` from the checkout at `root`. Cargo's own
/// incremental state in `cache` makes a rebuild cheap across runs.
fn build(root: &Path, cache: &Path, arch: &str) -> Result<PathBuf> {
    let triple = format!("{arch}-unknown-linux-musl");
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
        .args(["--target", &triple])
        .arg("--target-dir")
        .arg(cache)
        .current_dir(root)
        .status()
        .ctx("run cargo")?;
    if !status.success() {
        bail!(
            "cargo build for {triple} failed ({status}); the target may need \
             `rustup target add {triple}`"
        );
    }
    Ok(cache.join(&triple).join("release/usecoded"))
}

/// The release this uc came from, set by make (see /Makefile); without
/// one, the latest release.
const RELEASE: Option<&str> = option_env!("VERSION");

/// Download `usecoded-ARCH` from the GitHub release into `dir`, checked
/// against the release's SHA256SUMS.
fn download(dir: &Path, arch: &str) -> Result<PathBuf> {
    let (base, tag) = match RELEASE.filter(|v| !v.is_empty()) {
        Some(v) => (format!("{RELEASES}/download/v{v}"), format!("v{v}")),
        None => (format!("{RELEASES}/latest/download"), "latest".to_string()),
    };
    let name = format!("usecoded-{arch}");
    println!("downloading {name} from the {tag} release");

    // graviola is pure Rust, so the static builds stay free of C.
    let _ = rustls_graviola::default_provider().install_default();
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .build()
        .ctx("make an HTTP client")?;
    let get = |file: &str| -> Result<Vec<u8>> {
        let url = format!("{base}/{file}");
        let response = client
            .get(&url)
            .send()
            .and_then(|r| r.error_for_status())
            .with_ctx(|| format!("download {url}"))?;
        Ok(response
            .bytes()
            .with_ctx(|| format!("download {url}"))?
            .to_vec())
    };

    let sums = String::from_utf8_lossy(&get("SHA256SUMS")?).into_owned();
    let want = sums
        .lines()
        .find_map(|l| {
            let (sum, file) = l.split_once(char::is_whitespace)?;
            (file.trim_start_matches([' ', '*']) == name).then_some(sum)
        })
        .ok_or_else(|| err!("the {tag} release has no {name} in its SHA256SUMS"))?;
    let binary = get(&name)?;
    let got: String = Sha256::digest(&binary)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if got != want {
        bail!("{name} from the {tag} release does not match its SHA256SUMS");
    }

    fs::create_dir_all(dir).with_ctx(|| format!("create {}", dir.display()))?;
    let path = dir.join(&name);
    fs::write(&path, &binary).with_ctx(|| format!("write {}", path.display()))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .with_ctx(|| format!("chmod {}", path.display()))?;
    Ok(path)
}

const RELEASES: &str = "https://github.com/evgnomon/usecode/releases";
