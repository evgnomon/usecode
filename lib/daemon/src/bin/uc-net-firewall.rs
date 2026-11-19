// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc net firewall`: sets a host's inbound firewall policy from the
//! control node. See [`uc_daemon::firewall`] for the policy itself.
//!
//! Every change is worked out against the host's current
//! /etc/uc/firewall.toml, delivered back as a [`Bundle`] the daemon there
//! joins (so it writes the file itself and reloads), and never touches a
//! rule the daemon does not own.

use std::collections::HashMap;
use std::fs;
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

use uc_daemon::bundle::Bundle;
use uc_daemon::config::{self, Config};
use uc_daemon::error::{Context, Result};
use uc_daemon::firewall::{self, Settings, report};
use uc_daemon::flags::{usage_of, wants_help};
use uc_daemon::inventory::{self, Inventory};
use uc_daemon::kube;
use uc_daemon::remote::Target;
use uc_daemon::setup::BINARY_PATH;
use uc_daemon::{bail, err};

const USAGE: &str = "usage: uc net firewall COMMAND HOST [ARGS...]

Default-deny inbound firewall on a host, IPv4 and IPv6: everything but SSH
is closed unless allowed. Changes are written to /etc/uc/firewall.toml on
the host and the daemon there reloads.

  status [HOST...] [--raw]    what the live rules let in, from where and why;
                              without HOST, for every host of the inventory,
                              what they all share first and then what is
                              particular to some (--raw adds the iptables
                              rules themselves)
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
        "status" | "allow" | "unallow" | "ssh-port" | "enable" | "disable" if wants_help(rest) => {
            let section = usage_of(USAGE, &format!("  {cmd} "));
            let host = USAGE.rsplit("\n\n").next().unwrap_or_default();
            println!("usage: uc net firewall {cmd} ...\n\n{section}\n{host}");
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

/// Marks the sections of what [`gather`] reads on a host in one go.
const SECTION: &str = "@@uc-section ";

/// How long the cluster view waits for a host to let it in before it
/// counts it as unreachable.
const DEADLINE: Duration = Duration::from_secs(30);

fn status(args: &[String]) -> Result<()> {
    let raw = args.iter().any(|arg| arg == "--raw");
    let hosts: Vec<&str> = args
        .iter()
        .filter(|arg| *arg != "--raw")
        .map(String::as_str)
        .collect();
    if let Some(flag) = hosts.iter().find(|host| host.starts_with('-')) {
        bail!("usage: uc net firewall status [HOST...] [--raw]\n\nunknown option {flag:?}");
    }
    let names = inventory_names();

    // One host: its own report, with a table header.
    if let [name] = hosts.as_slice() {
        let target = reach(name)?;
        let found = gather(&target, name, &names)?;
        print!("{}", report::render(&found.host, &found.live));
        if raw {
            print!("{}", found.raw);
        }
        return Ok(());
    }

    // Many: every host of the inventory unless named, asked side by side.
    let hosts: Vec<String> = if hosts.is_empty() {
        let dir = inventory::find()
            .map_err(|e| err!("{e} (or name the hosts: uc net firewall status HOST...)"))?;
        Inventory::load(&dir)?
            .hosts
            .into_iter()
            .map(|host| host.name)
            .collect()
    } else {
        hosts.iter().map(|host| host.to_string()).collect()
    };
    let results: Vec<(String, Result<Gathered>)> = std::thread::scope(|scope| {
        let jobs: Vec<_> = hosts
            .iter()
            .map(|name| {
                let names = &names;
                scope.spawn(move || {
                    let found = Target::reach_unattended(name, DEADLINE)
                        .and_then(|target| gather(&target, name, names));
                    (name.clone(), found)
                })
            })
            .collect();
        jobs.into_iter()
            .map(|job| job.join().expect("a status thread panicked"))
            .collect()
    });
    let views: Vec<report::View> = results
        .iter()
        .map(|(name, found)| match found {
            Ok(found) => report::view(&found.host, &found.live),
            Err(e) => report::View::unreachable(name, e.to_string()),
        })
        .collect();
    print!("{}", report::render_cluster(&views));
    if raw {
        for (name, found) in &results {
            if let Ok(found) = found {
                print!("\n{name}:{}", found.raw);
            }
        }
    }
    Ok(())
}

/// What [`gather`] learned about a host.
struct Gathered {
    host: report::Host,
    live: Vec<report::Live>,
    /// The chain as iptables prints it, for `--raw`.
    raw: String,
}

/// Read a host's firewall in one trip, as root: the settings, the live
/// rules of both families and whether INPUT jumps to them, the port sshd
/// listens on and the mesh and Kubernetes settings.
fn gather(target: &Target, name: &str, names: &HashMap<String, String>) -> Result<Gathered> {
    let mut script = format!(
        "echo '{SECTION}settings'; cat {} 2>/dev/null; ",
        firewall::SETTINGS_PATH
    );
    for family in firewall::FAMILIES {
        let tool = family.command();
        let chain = firewall::CHAIN;
        script.push_str(&format!(
            "echo '{SECTION}{tool}'; \
             if out=$({tool} -S {chain} 2>&1); then printf '%s\\n' \"$out\"; \
             else printf '{SECTION}error %s\\n' \"$out\"; fi; \
             if {tool} -C INPUT -j {chain} 2>/dev/null; then echo '{SECTION}hooked'; fi; "
        ));
    }
    script.push_str(&format!(
        "echo '{SECTION}sshd'; sshd -T 2>/dev/null | awk '$1==\"port\"{{print $2; exit}}'; \
         echo '{SECTION}mesh'; cat {} 2>/dev/null; \
         echo '{SECTION}kube'; cat {} 2>/dev/null; true",
        config::DEFAULT_PATH,
        kube::SETTINGS_PATH
    ));
    let out = target.output_as_root(&["sh".into(), "-c".into(), script])?;
    let sections = sections(&out);
    let section = |key: &str| sections.get(key).map(String::as_str).unwrap_or_default();

    // An empty or missing settings file is the default policy.
    let settings = match section("settings").trim() {
        "" => Settings::default(),
        body => Settings::parse(body)?,
    };
    let mesh: Option<Config> = toml::from_str(section("mesh")).ok();
    let kube = kube::Settings::parse(section("kube")).unwrap_or_default();
    let ssh_port = settings
        .ssh_port
        .or_else(|| section("sshd").trim().parse().ok())
        .unwrap_or(22);

    let mut raw = String::new();
    let live: Vec<report::Live> = firewall::FAMILIES
        .iter()
        .map(|family| {
            let body = section(family.command());
            raw.push_str(&format!("\n{} -S {}:\n", family.command(), firewall::CHAIN));
            for line in body.lines().filter(|line| !line.starts_with(SECTION)) {
                raw.push_str(line);
                raw.push('\n');
            }
            let rules = match body
                .lines()
                .find_map(|line| line.strip_prefix(&format!("{SECTION}error ")))
            {
                Some(error) => Err(error.trim().to_string()),
                None => Ok(report::parse_chain(body)),
            };
            report::Live {
                family: *family,
                rules,
                hooked: body.contains(&format!("{SECTION}hooked")),
            }
        })
        .collect();

    let mut names = names.clone();
    if let Some(cfg) = &mesh {
        names.extend(mesh_names(cfg));
    }
    let host = report::Host {
        name: name.to_string(),
        enabled: settings.enabled,
        ssh_port,
        allow: settings.allow.clone(),
        kube_api: kube.enabled && !kube.agent,
        kube_peers: if kube.enabled {
            kube.peers.clone()
        } else {
            Vec::new()
        },
        mesh: mesh.as_ref().filter(|cfg| cfg.mesh_enabled()).map(|cfg| {
            let port = u16::try_from(cfg.interface.listen_port).unwrap_or(0);
            (cfg.interface.name.clone(), port)
        }),
        names,
    };
    Ok(Gathered { host, live, raw })
}

/// What a script printed, split at the [`SECTION`] markers. The family
/// sections keep their own `error` and `hooked` markers.
fn sections(out: &str) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();
    let mut current = String::new();
    for line in out.lines() {
        if let Some(key) = line.strip_prefix(SECTION)
            && !key.starts_with("error ")
            && key != "hooked"
        {
            current = key.to_string();
            map.entry(current.clone()).or_default();
            continue;
        }
        let body = map.entry(current.clone()).or_default();
        body.push_str(line);
        body.push('\n');
    }
    map
}

fn bare(addr: &str) -> String {
    addr.split('/').next().unwrap_or_default().to_string()
}

/// A host's mesh peers by name, from their endpoints and mesh addresses.
fn mesh_names(cfg: &Config) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for peer in &cfg.peers {
        if let Some((host, _)) = peer.endpoint.rsplit_once(':') {
            let host = host.trim_start_matches('[').trim_end_matches(']');
            names.insert(host.to_string(), peer.name.clone());
        }
        for ip in &peer.allowed_ips {
            names.insert(bare(ip), peer.name.clone());
        }
    }
    names
}

/// Host names by address, to name the sources of rules: the inventory's
/// hosts, by what they are reached at (their ssh_config `HostName` too)
/// and their mesh address.
fn inventory_names() -> HashMap<String, String> {
    let mut names = HashMap::new();
    let hosts = inventory::find()
        .ok()
        .and_then(|dir| Inventory::load(&dir).ok())
        .map(|inv| inv.hosts)
        .unwrap_or_default();
    for host in hosts {
        let reached = match host.ansible_host.as_str() {
            "" => host.name.as_str(),
            addr => addr,
        };
        if let Some(addr) = ssh_hostname(reached) {
            names.insert(addr, host.name.clone());
        }
        for addr in [&host.ansible_host, &host.address] {
            if !addr.is_empty() {
                names.insert(bare(addr), host.name.clone());
            }
        }
    }
    names
}

/// The `HostName` ssh_config gives `host`, as `ssh -G` works it out
/// (without connecting).
fn ssh_hostname(host: &str) -> Option<String> {
    let out = Command::new("ssh")
        .args(["-G", host])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("hostname "))
        .map(|addr| addr.trim().to_string())
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
