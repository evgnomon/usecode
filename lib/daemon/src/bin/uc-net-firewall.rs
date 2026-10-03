// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc net firewall`: sets a host's inbound firewall policy from the
//! control node. See [`uc_daemon::firewall`] for the policy itself.
//!
//! Every change is worked out against the host's current
//! /etc/uc/firewall.toml, delivered back as a [`Bundle`] the daemon there
//! joins (so it writes the file itself and reloads), and never touches a
//! rule the daemon does not own.

use std::fs;
use std::process::ExitCode;

use uc_daemon::bundle::Bundle;
use uc_daemon::error::{Context, Result};
use uc_daemon::firewall::{self, Settings};
use uc_daemon::kube;
use uc_daemon::remote::Target;
use uc_daemon::setup::BINARY_PATH;
use uc_daemon::{bail, err};

const USAGE: &str = "usage: uc net firewall COMMAND HOST [ARGS...]

Default-deny inbound firewall on a host, IPv4 and IPv6: everything but SSH
is closed unless allowed. Changes are written to /etc/uc/firewall.toml on
the host and the daemon there reloads.

  status HOST                 show the settings and the live rules
  allow HOST RULE...          add inbound exceptions; RULE is
                              \"<tcp|udp>:<port>[:<source>]\" (e.g. tcp:443,
                              udp:8000-8100:10.0.0.0/8, tcp:443:2001:db8::/32;
                              source defaults to any host, and an IPv4 or IPv6
                              source limits the rule to that family)
  unallow HOST RULE...        remove exceptions (matched as written)
  ssh-port HOST PORT          keep this port open for SSH (default: what
                              sshd reports, or 22)
  enable HOST                 turn the default-deny policy on
  disable HOST                turn it off (open the host)

HOST is an inventory name or any ssh destination ([USER@]HOST or an alias
from ~/.ssh/config).";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("uc net firewall: error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<()> {
    let (cmd, rest) = match args.split_first() {
        Some(pair) => pair,
        None => {
            eprintln!("{USAGE}");
            return Ok(());
        }
    };

    match cmd.as_str() {
        "help" | "-h" | "--help" => {
            println!("{USAGE}");
            Ok(())
        }
        "status" => status(rest),
        "allow" => edit_list(rest, true),
        "unallow" => edit_list(rest, false),
        "ssh-port" => ssh_port(rest),
        "enable" => set_enabled(rest, true),
        "disable" => set_enabled(rest, false),
        other => bail!("{USAGE}\n\nunknown command {other:?}"),
    }
}

/// The first argument plus the rest, or the usage error.
fn split<'a>(args: &'a [String], usage: &str) -> Result<(&'a str, &'a [String])> {
    match args.split_first() {
        Some((host, rest)) if !host.is_empty() => Ok((host, rest)),
        _ => bail!("{usage}"),
    }
}

fn status(args: &[String]) -> Result<()> {
    let (name, _) = split(args, "usage: uc net firewall status HOST")?;
    let target = reach(name)?;
    let settings = read_settings(&target)?;

    println!(
        "{name}: firewall {}",
        if settings.enabled { "on" } else { "off" }
    );
    println!("  ssh port  {}", firewall::ssh_port(&settings));
    if settings.allow.is_empty() {
        println!("  allow     (none)");
    } else {
        for rule in &settings.allow {
            println!("  allow     {rule}");
        }
    }
    // Opened by the daemon itself while `uc kube` has a node here.
    if let Some(kube) = read_kube(&target)
        && kube.enabled
    {
        let api = if kube.agent {
            String::new()
        } else {
            format!("tcp:{} (API), ", kube::API_PORT)
        };
        println!(
            "  kube      {api}pod interfaces, {} peer node(s)",
            kube.peers.len()
        );
    }

    for family in firewall::FAMILIES {
        let tool = family.command();
        match target.output_as_root(&[
            tool.to_string(),
            "-S".to_string(),
            firewall::CHAIN.to_string(),
        ]) {
            Ok(rules) => println!("\nlive rules ({tool}):\n{rules}"),
            Err(e) => println!("\nlive rules ({tool}): {e}"),
        }
    }
    Ok(())
}

fn edit_list(args: &[String], add: bool) -> Result<()> {
    let usage = if add {
        "usage: uc net firewall allow HOST RULE..."
    } else {
        "usage: uc net firewall unallow HOST RULE..."
    };
    let (name, specs) = split(args, usage)?;
    if specs.is_empty() {
        bail!("{usage}");
    }
    // Check every spec before touching the host.
    for spec in specs {
        firewall::parse_allow(spec)?;
    }

    update(name, |settings| {
        for spec in specs {
            if add {
                if !settings.allow.contains(spec) {
                    settings.allow.push(spec.clone());
                }
            } else if let Some(i) = settings.allow.iter().position(|s| s == spec) {
                settings.allow.remove(i);
            }
        }
        settings.allow.sort();
        Ok(())
    })
}

fn ssh_port(args: &[String]) -> Result<()> {
    let (name, rest) = split(args, "usage: uc net firewall ssh-port HOST PORT")?;
    let port = rest
        .first()
        .ok_or_else(|| err!("usage: uc net firewall ssh-port HOST PORT"))?;
    let port: u16 = port
        .parse()
        .map_err(|_| err!("{port:?} is not a port number"))?;
    if port == 0 {
        bail!("{port:?} is not a port number");
    }

    update(name, |settings| {
        settings.ssh_port = Some(port);
        Ok(())
    })
}

fn set_enabled(args: &[String], enabled: bool) -> Result<()> {
    let name = if enabled {
        split(args, "usage: uc net firewall enable HOST")?.0
    } else {
        split(args, "usage: uc net firewall disable HOST")?.0
    };
    update(name, |settings| {
        settings.enabled = enabled;
        Ok(())
    })
}

/// Read-modify-write the host's settings, then have the daemon pick them
/// up. The host is how `uc daemon reload` reaches one: through the
/// inventory if it is there, as an ssh destination if not.
fn update(name: &str, f: impl FnOnce(&mut Settings) -> Result<()>) -> Result<()> {
    let target = reach(name)?;
    let mut settings = read_settings(&target)?;
    f(&mut settings)?;
    deliver(&target, name, &settings)?;
    println!("{name}: firewall settings updated");
    Ok(())
}

fn reach(name: &str) -> Result<Target> {
    Target::reach(name)
}

/// The host's current settings; an empty or missing file is the default
/// (default-deny, SSH only).
fn read_settings(target: &Target) -> Result<Settings> {
    let body = target.output(&format!(
        "cat {} 2>/dev/null || true",
        firewall::SETTINGS_PATH
    ))?;
    if body.trim().is_empty() {
        return Ok(Settings::default());
    }
    Settings::parse(&body)
}

/// The host's Kubernetes settings, if it has any and they can be read
/// (the file is root's; status works without them).
fn read_kube(target: &Target) -> Option<kube::Settings> {
    let body = target
        .output_as_root(&[
            "sh".into(),
            "-c".into(),
            format!("cat {} 2>/dev/null || true", kube::SETTINGS_PATH),
        ])
        .ok()?;
    kube::Settings::parse(&body).ok()
}

/// Hand the settings to the host's daemon through `usecoded join`, the
/// same path `uc net mesh apply` uses, so the daemon writes the file and
/// reloads with an answer.
fn deliver(target: &Target, name: &str, settings: &Settings) -> Result<()> {
    let bundle = Bundle {
        host: name.to_string(),
        firewall: Some(settings.clone()),
        ..Bundle::default()
    };

    let local = tempfile::Builder::new()
        .prefix("uc-net-firewall-")
        .tempdir()
        .ctx("create a staging directory")?;
    let path = local.path().join("bundle.toml");
    fs::write(&path, bundle.encode()?).ctx("write the bundle")?;

    let dir = target.stage(&[("bundle.toml", path.as_path())])?;
    let result = target
        .run_as_root(&[
            BINARY_PATH.to_string(),
            "join".to_string(),
            format!("{dir}/bundle.toml"),
        ])
        .with_ctx(|| {
            format!(
                "join (if the daemon is missing or older than this, `uc daemon install {name}` \
                 first)"
            )
        });
    target.remove_all(&dir);
    result
}
