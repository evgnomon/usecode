//! Manages the WireGuard interface using the ip(8) and wg(8)
//! command-line tools shipped by the wireguard-tools package.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::process::{Command, Stdio};

use tempfile::TempDir;

use crate::config::Config;
use crate::error::{Context, Result};
use crate::keys;

/// Bring up the WireGuard interface described by `cfg`: create the link
/// (if missing), assign the address, load the private key and peers, and
/// set the link up. It is idempotent; calling it on an already-up
/// interface reapplies the configuration.
pub fn up(cfg: &Config) -> Result<()> {
    let iface = &cfg.interface.name;

    if !link_exists(iface) {
        run("ip", &["link", "add", "dev", iface, "type", "wireguard"])
            .with_ctx(|| format!("create interface {iface}"))?;
    }

    configure_device(cfg)?;

    run(
        "ip",
        &["address", "replace", &cfg.interface.address, "dev", iface],
    )
    .ctx("assign address")?;

    if cfg.interface.mtu > 0 {
        run(
            "ip",
            &[
                "link",
                "set",
                "dev",
                iface,
                "mtu",
                &cfg.interface.mtu.to_string(),
            ],
        )
        .ctx("set mtu")?;
    }

    run("ip", &["link", "set", "dev", iface, "up"]).ctx("set interface up")?;

    Ok(())
}

/// Remove the WireGuard interface. A no-op if it does not exist.
pub fn down(iface: &str) -> Result<()> {
    if !link_exists(iface) {
        return Ok(());
    }
    run("ip", &["link", "del", "dev", iface]).with_ctx(|| format!("delete interface {iface}"))?;
    Ok(())
}

/// The output of `wg show <iface>`. The output is returned even when the
/// command fails, since that is what `portman status` wants to show.
pub fn status(iface: &str) -> (String, Result<()>) {
    match Command::new("wg").args(["show", iface]).output() {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            if out.status.success() {
                (text, Ok(()))
            } else {
                (text, Err(crate::err!("wg show {iface}: {}", out.status)))
            }
        }
        Err(e) => (String::new(), Err(crate::err!("wg show {iface}: {e}"))),
    }
}

fn configure_device(cfg: &Config) -> Result<()> {
    let priv_key = keys::ensure_private_key().ctx("load private key")?;

    // One directory for every key file this call needs; dropping it
    // wipes them all, whichever step failed.
    let dir = tempfile::Builder::new()
        .prefix("portman-key-")
        .tempdir()
        .ctx("create temp key directory")?;

    let mut args: Vec<String> = vec![
        "set".into(),
        cfg.interface.name.clone(),
        "private-key".into(),
        write_temp_key(&dir, "private", &priv_key)?,
    ];
    if cfg.interface.listen_port > 0 {
        args.push("listen-port".into());
        args.push(cfg.interface.listen_port.to_string());
    }

    for (i, p) in cfg.peers.iter().enumerate() {
        args.push("peer".into());
        args.push(p.public_key.clone());

        if !p.preshared_key.is_empty() {
            args.push("preshared-key".into());
            args.push(write_temp_key(&dir, &format!("psk-{i}"), &p.preshared_key)?);
        }
        if !p.allowed_ips.is_empty() {
            args.push("allowed-ips".into());
            args.push(p.allowed_ips.join(","));
        }
        if !p.endpoint.is_empty() {
            args.push("endpoint".into());
            args.push(p.endpoint.clone());
        }
        if p.persistent_keepalive > 0 {
            args.push("persistent-keepalive".into());
            args.push(p.persistent_keepalive.to_string());
        }
    }

    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    run("wg", &argv).ctx("configure wireguard device")?;
    Ok(())
}

/// Write key material to a 0600 file owned by the current process so
/// wg(8) can read it without the secret ever appearing in the process
/// argument list (visible via ps/proc to other users).
fn write_temp_key(dir: &TempDir, name: &str, key: &str) -> Result<String> {
    let path = dir.path().join(name);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .ctx("create temp key file")?;
    f.write_all(format!("{key}\n").as_bytes())
        .ctx("write temp key file")?;
    f.sync_all().ctx("write temp key file")?;
    Ok(path.to_string_lossy().into_owned())
}

fn link_exists(iface: &str) -> bool {
    Command::new("ip")
        .args(["link", "show", "dev", iface])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run a command, folding its combined output into the error message so
/// a failure says what the tool complained about.
pub fn run(name: &str, args: &[&str]) -> Result<()> {
    let out = Command::new(name)
        .args(args)
        .output()
        .with_ctx(|| format!("{name} {}", args.join(" ")))?;
    if out.status.success() {
        return Ok(());
    }
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    Err(crate::err!(
        "{name} {:?}: {}: {}",
        args,
        out.status,
        combined.trim()
    ))
}
