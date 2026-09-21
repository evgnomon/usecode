//! portman sets up a WireGuard mesh and configures DNAT rules so that
//! traffic to a public address lands on a port running elsewhere in the
//! mesh. There is no client/server distinction at the command level:
//! every host runs the same commands, and what a host actually does
//! (accept inbound connections, forward traffic) follows from its
//! config.

#[macro_use]
mod error;

mod app;
mod config;
mod flags;
mod inventory;
mod iptables;
mod keys;
mod net;
mod remote;
mod wg;

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use config::{Config, Descriptor, Peer, Service};
use error::{Context, Result};
use flags::FlagSet;
use inventory::{Inventory, NewHost, Vault};
use net::join_host_port;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("portman: error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<()> {
    let Some((cmd, rest)) = args.split_first() else {
        usage();
        return Ok(());
    };

    match cmd.as_str() {
        "up" => with_config(rest, REQUIRE_ROOT, app::up),
        "down" => with_config(rest, REQUIRE_ROOT, app::down),
        "reload" => with_config(rest, REQUIRE_ROOT, app::reload),
        "status" => with_config(rest, NO_ROOT_REQUIRED, |cfg| {
            let (out, res) = app::status(cfg);
            print!("{out}");
            res
        }),
        "validate" => with_config(rest, NO_ROOT_REQUIRED, |cfg| {
            println!(
                "{}: OK (interface={}, peers={}, services={})",
                config_path(rest),
                cfg.interface.name,
                cfg.peers.len(),
                cfg.services.len()
            );
            Ok(())
        }),
        "add" => add(rest),
        "export" => export(rest),
        "import" => import_peer(rest),
        "forward" => forward(rest),
        "unforward" => unforward(rest),
        "pubkey" => pubkey(),
        "genkey" => genkey(rest),
        "version" | "-v" | "--version" => {
            println!("portman {VERSION}");
            Ok(())
        }
        "help" | "-h" | "--help" => {
            usage();
            Ok(())
        }
        other => {
            usage();
            bail!("unknown command {other:?}")
        }
    }
}

fn usage() {
    eprint!(
        r#"portman - WireGuard mesh + DNAT port forwarding manager

Every host runs the same commands. What a host does (accept inbound
connections, forward traffic to a peer) follows from its config, not
from a mode you pick up front.

Fleet (run on the control node, in a checkout of this repo):
  portman add NAME [-endpoint HOST:PORT] [-ansible-host HOST]
                                     put a host into the mesh topology: allocate
                                     the next free tunnel address, mint its
                                     keypair, record it in the Ansible inventory
                                     (private key into the vault), then run
                                     ansible-playbook to converge every host

Setup (run on the host itself, for a mesh you drive by hand instead):
  portman export  [-out FILE] [-address CIDR] [-endpoint HOST:PORT]
                                     generate this host's keypair if needed and
                                     write its descriptor - hand FILE to any host
                                     that should peer with this one
  portman import  DESCRIPTOR_FILE   add the host behind a descriptor as a peer
  portman forward NAME PROTO PORT
  portman forward NAME PROTO [BIND:]PORT:PEER:PEER_PORT
                                     declare a service: with just a PORT, "I run
                                     this here"; with :PEER:PEER_PORT, "forward
                                     PORT to that peer's PEER_PORT"
  portman unforward NAME            remove a forward/service declaration

Apply:
  portman up       [-config PATH]   bring up the tunnel, and DNAT rules for any
                                     forward declarations in this host's config
  portman down     [-config PATH]   tear down the tunnel and any DNAT rules
  portman reload   [-config PATH]   reapply the config to a running tunnel
  portman status   [-config PATH]   show WireGuard and DNAT state
  portman validate [-config PATH]   check the config file without applying it

Low-level:
  portman pubkey                    print this host's WireGuard public key
  portman genkey [-force]           (re)generate this host's WireGuard keypair
  portman version                   print the portman version

portman manages its own WireGuard private key; it is never read from or
written to the config file. Keys live root-only under {keys_dir}.

Config defaults to {config_path} and must be mode 0600, owned by root.
"#,
        keys_dir = keys::DIR,
        config_path = config::DEFAULT_PATH,
    );
}

fn config_path(args: &[String]) -> String {
    let mut fs = FlagSet::new("portman");
    fs.string("config", config::DEFAULT_PATH);
    let _ = fs.parse(args);
    fs.get_str("config")
}

const REQUIRE_ROOT: bool = true;
const NO_ROOT_REQUIRED: bool = false;

fn with_config(
    args: &[String],
    must_be_root: bool,
    f: impl FnOnce(&Config) -> Result<()>,
) -> Result<()> {
    let mut fs = FlagSet::new("portman");
    fs.string("config", config::DEFAULT_PATH);
    fs.parse(args)?;

    if must_be_root {
        require_root()?;
    }

    let cfg = Config::load(&fs.get_str("config"))?;
    f(&cfg)
}

fn require_root() -> Result<()> {
    // SAFETY: geteuid is always safe to call and cannot fail.
    if unsafe { libc::geteuid() } != 0 {
        bail!("this command must be run as root");
    }
    Ok(())
}

/// Puts a host into the mesh topology. Addresses are the reason this
/// exists: they cannot be picked safely on the host being added, because
/// "free" is a fact about every other host. So the topology lives in one
/// place - the Ansible inventory - and add is the thing that reads all
/// of it, allocates the lowest unused address, and records the new
/// member. Nothing is configured on the host here; `ansible-playbook`
/// applies the topology afterwards.
fn add(args: &[String]) -> Result<()> {
    let mut fs = FlagSet::new("add");
    fs.string("inventory", "")
        .string("endpoint", "")
        .string("address", "")
        .string("ansible-host", "")
        .string("ansible-user", "root")
        .string("vault-password-file", "");

    // `portman add NAME -endpoint ...` is how anyone would write this,
    // but parsing stops at the first non-flag argument - so lift the
    // name off the front ourselves when it leads, and accept it trailing
    // the flags too.
    let (name, rest, leading) = match args.first() {
        Some(first) if !first.starts_with('-') => (first.clone(), &args[1..], true),
        _ => (String::new(), args, false),
    };
    fs.parse(rest)?;

    let name = if leading { name } else { fs.arg(0) };
    let leftover = fs.nargs();
    if (leading && leftover > 0) || (!leading && leftover != 1) {
        bail!("usage: portman add NAME [flags]");
    }

    inventory::check_available()?;

    let dir = match fs.get_str("inventory") {
        s if s.is_empty() => find_inventory()?,
        s => PathBuf::from(s),
    };

    let mut inv = Inventory::load(&dir)?;
    let vault = Vault::new(&inv, &fs.get_str("vault-password-file"));

    let host = inv.add(
        NewHost {
            name,
            address: fs.get_str("address"),
            endpoint: fs.get_str("endpoint"),
            ansible_host: fs.get_str("ansible-host"),
            ansible_user: fs.get_str("ansible-user"),
        },
        &vault,
    )?;

    let prefix = inv.prefix()?;

    println!("added {} to the mesh", host.name);
    println!(
        "  address    {}/{prefix} (allocated from {})",
        host.address, inv.settings.network
    );
    println!("  public key {}", host.public_key);
    if host.endpoint.is_empty() {
        println!("  endpoint   none - dials out only");
    } else {
        println!("  endpoint   {}", host.endpoint);
    }
    println!(
        "\nApply the topology to every host (from {}):",
        inv.root.display()
    );
    println!("  ansible-playbook deploy/playbooks/portman.yml");
    Ok(())
}

/// Looks for the inventory in the current directory and its parents, so
/// `portman add` works anywhere inside a checkout.
fn find_inventory() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().ctx("determine working directory")?;
    loop {
        let candidate = dir.join(inventory::DEFAULT_DIR);
        if candidate.join("hosts.yml").exists() {
            return Ok(candidate);
        }
        if !dir.pop() {
            bail!(
                "no {} found in this directory or any parent; run `portman add` from a portman \
                 checkout, or pass -inventory DIR",
                inventory::DEFAULT_DIR
            );
        }
    }
}

/// Generates this host's persistent keypair if needed and writes a
/// small, non-secret descriptor (name, public key, address, endpoint)
/// that another host can hand to `portman import` to add this host as a
/// peer. If no config exists yet, -address creates a minimal one first.
fn export(args: &[String]) -> Result<()> {
    let mut fs = FlagSet::new("export");
    fs.string("config", config::DEFAULT_PATH)
        .string("out", "")
        .string("name", "")
        .string("address", "")
        .string("endpoint", "")
        .string("interface", "wg-portman")
        .int("listen-port", 51820);
    fs.parse(args)?;

    require_root()?;

    let path = fs.get_str("config");
    let cfg = if Path::new(&path).exists() {
        Config::load(&path)?
    } else {
        let address = fs.get_str("address");
        if address.is_empty() {
            bail!(
                "no config at {path} yet; pass -address (e.g. -address 10.10.0.2/24) to create one"
            );
        }
        Config::new_at(
            &path,
            &fs.get_str("interface"),
            &address,
            fs.get_int("listen-port"),
        )?
    };

    let public_key = keys::public_key()?;
    let address = cfg.address()?;

    let hostname = match fs.get_str("name") {
        s if !s.is_empty() => s,
        _ => this_hostname()?,
    };

    let desc = Descriptor {
        name: hostname.clone(),
        public_key,
        address,
        endpoint: fs.get_str("endpoint"),
    };
    let body = toml::to_string(&desc).ctx("encode descriptor")?;

    let out = fs.get_str("out");
    if out.is_empty() {
        print!("{body}");
    } else {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .open(&out)
            .with_ctx(|| format!("write {out}"))?;
        f.write_all(body.as_bytes())
            .with_ctx(|| format!("write {out}"))?;
        eprintln!("wrote {out} - hand it to any host that should peer with {hostname:?}");
    }
    Ok(())
}

fn this_hostname() -> Result<String> {
    let name =
        fs::read_to_string("/proc/sys/kernel/hostname").ctx("determine hostname (pass -name)")?;
    let name = name.trim().to_string();
    if name.is_empty() {
        bail!("determine hostname (pass -name): the system reports no hostname");
    }
    Ok(name)
}

/// Reads a descriptor written by `portman export` on another host and
/// adds (or updates) it as a `[[peer]]` in this host's config.
fn import_peer(args: &[String]) -> Result<()> {
    let mut fs = FlagSet::new("import");
    fs.string("config", config::DEFAULT_PATH)
        .string("allowed-ips", "")
        .int("persistent-keepalive", 25)
        .string("preshared-key", "");
    fs.parse(args)?;
    if fs.nargs() != 1 {
        bail!("usage: portman import DESCRIPTOR_FILE");
    }
    let desc_path = fs.arg(0);

    require_root()?;

    let body = std::fs::read_to_string(&desc_path).with_ctx(|| format!("parse {desc_path}"))?;
    let desc: Descriptor = toml::from_str(&body).with_ctx(|| format!("parse {desc_path}"))?;
    if desc.name.is_empty() || desc.public_key.is_empty() || desc.address.is_empty() {
        bail!("{desc_path} is not a valid portman descriptor (missing name/public_key/address)");
    }

    let path = fs.get_str("config");
    let mut cfg = Config::load(&path)?;

    let allowed_ips = fs.get_str("allowed-ips");
    let ips: Vec<String> = if allowed_ips.is_empty() {
        vec![format!("{}/32", desc.address)]
    } else {
        allowed_ips.split(',').map(str::to_string).collect()
    };

    cfg.add_or_replace_peer(Peer {
        name: desc.name.clone(),
        public_key: desc.public_key,
        preshared_key: fs.get_str("preshared-key"),
        endpoint: desc.endpoint,
        allowed_ips: ips,
        persistent_keepalive: fs.get_int("persistent-keepalive"),
    });
    cfg.save(&path)?;

    println!(
        "imported peer {:?} ({}) into {path}",
        desc.name, desc.address
    );
    println!("run `sudo portman up` (or `reload` if already running) to apply");
    Ok(())
}

/// Adds or replaces a `[[service]]` declaration: a bare port is a local
/// declaration ("I run this here"); PORT:PEER:PEER_PORT (optionally
/// prefixed with BIND:) is a forward rule to an already-imported peer.
fn forward(args: &[String]) -> Result<()> {
    let mut fs = FlagSet::new("forward");
    fs.string("config", config::DEFAULT_PATH);
    fs.parse(args)?;
    if fs.nargs() != 3 {
        bail!(
            "usage: portman forward NAME PROTO PORT | portman forward NAME PROTO \
             [BIND:]PORT:PEER:PEER_PORT"
        );
    }
    let (name, proto, spec) = (fs.arg(0), fs.arg(1), fs.arg(2));

    require_root()?;

    let path = fs.get_str("config");
    let mut cfg = Config::load(&path)?;

    let svc = build_service(&cfg, &name, &proto, &spec)?;
    let is_forward = svc.is_forward();
    let (remote_bind, target, local_port) = (
        svc.remote_bind.clone(),
        join_host_port(&svc.client_address, svc.local_port as u16),
        svc.local_port,
    );

    cfg.add_or_replace_service(svc);
    cfg.save(&path)?;

    if is_forward {
        println!("saved service {name:?}: forward {remote_bind} -> {target}");
    } else {
        println!("saved service {name:?}: local port {local_port}");
    }
    println!("run `sudo portman up` (or `reload` if already running) to apply");
    Ok(())
}

fn build_service(cfg: &Config, name: &str, proto: &str, spec: &str) -> Result<Service> {
    let parts: Vec<&str> = spec.split(':').collect();

    match parts.len() {
        1 => {
            let port: i64 = parts[0].parse().unwrap_or(0);
            if port <= 0 {
                bail!("invalid port {:?}", parts[0]);
            }
            Ok(Service {
                name: name.to_string(),
                protocol: proto.to_string(),
                local_port: port,
                ..Service::default()
            })
        }

        3 | 4 => {
            let (bind_host, bind_port, peer_name, peer_port) = if parts.len() == 4 {
                (parts[0], parts[1], parts[2], parts[3])
            } else {
                ("0.0.0.0", parts[0], parts[1], parts[2])
            };
            if bind_port.parse::<u16>().is_err() {
                bail!("invalid port {bind_port:?}");
            }
            let peer_port: i64 = peer_port.parse().unwrap_or(0);
            if peer_port <= 0 {
                bail!("invalid peer port {:?}", parts[parts.len() - 1]);
            }

            let peer = cfg
                .peer_by_name(peer_name)
                .ok_or_else(|| err!("no peer named {peer_name:?}; run `portman import` first"))?;
            let addr = peer_address(peer).with_ctx(|| format!("peer {peer_name:?}"))?;

            Ok(Service {
                name: name.to_string(),
                protocol: proto.to_string(),
                remote_bind: format!("{bind_host}:{bind_port}"),
                client_address: addr,
                local_port: peer_port,
            })
        }

        _ => bail!(
            "invalid forward spec {spec:?}: want PORT, or PORT:PEER:PEER_PORT, or \
             BIND:PORT:PEER:PEER_PORT"
        ),
    }
}

/// Extracts a single host address from a peer's allowed_ips, as set by
/// `portman import` (descriptor address + "/32").
fn peer_address(p: &Peer) -> Result<String> {
    if p.allowed_ips.len() != 1 {
        bail!(
            "has {} allowed_ips, not a single address; set client_address by editing the config \
             directly",
            p.allowed_ips.len()
        );
    }
    let entry = &p.allowed_ips[0];
    if let Ok((ip, _)) = net::parse_cidr(entry) {
        return Ok(ip.to_string());
    }
    if let Ok(ip) = entry.parse::<std::net::IpAddr>() {
        return Ok(ip.to_string());
    }
    bail!("allowed_ips {entry:?} is not a single host address")
}

/// Removes a service declaration by name.
fn unforward(args: &[String]) -> Result<()> {
    let mut fs = FlagSet::new("unforward");
    fs.string("config", config::DEFAULT_PATH);
    fs.parse(args)?;
    if fs.nargs() != 1 {
        bail!("usage: portman unforward NAME");
    }
    let name = fs.arg(0);

    require_root()?;

    let path = fs.get_str("config");
    let mut cfg = Config::load(&path)?;
    if !cfg.remove_service(&name) {
        bail!("no service named {name:?}");
    }
    cfg.save(&path)?;

    println!("removed service {name:?}");
    println!("run `sudo portman up` (or `reload` if already running) to apply");
    Ok(())
}

/// Prints this host's persistent WireGuard public key, generating a
/// keypair on first use if none exists yet. The private key never leaves
/// the host; only this value needs to be shared with a peer.
fn pubkey() -> Result<()> {
    require_root()?;
    println!("{}", keys::public_key()?);
    Ok(())
}

/// (Re)generates this host's WireGuard keypair. Regenerating an existing
/// keypair breaks the tunnel until every peer's config is updated with
/// the new public key, so it requires -force.
fn genkey(args: &[String]) -> Result<()> {
    require_root()?;

    let mut fs = FlagSet::new("genkey");
    fs.bool("force", false);
    fs.parse(args)?;

    if Path::new(keys::PRIVATE_KEY_PATH).exists() && !fs.get_bool("force") {
        bail!(
            "a keypair already exists at {}; pass -force to replace it (you'll need to update \
             the public key on every peer)",
            keys::PRIVATE_KEY_PATH
        );
    }

    let (_, public_key) = keys::generate()?;
    println!("{public_key}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::Interface;

    fn cfg_with_peer() -> Config {
        let mut cfg = Config {
            interface: Interface {
                name: "wg-portman".into(),
                address: "10.10.0.2/24".into(),
                listen_port: 51820,
                mtu: 0,
            },
            ..Config::default()
        };
        cfg.add_or_replace_peer(Peer {
            name: "shadow".into(),
            public_key: "abc=".into(),
            allowed_ips: vec!["10.10.0.1/32".into()],
            ..Peer::default()
        });
        cfg
    }

    #[test]
    fn a_bare_port_is_a_local_declaration() {
        let svc = build_service(&cfg_with_peer(), "web", "tcp", "8080").unwrap();
        assert!(!svc.is_forward());
        assert_eq!(svc.local_port, 8080);
    }

    #[test]
    fn three_fields_forward_from_every_address() {
        let svc = build_service(&cfg_with_peer(), "web", "tcp", "443:shadow:8443").unwrap();
        assert_eq!(svc.remote_bind, "0.0.0.0:443");
        assert_eq!(svc.client_address, "10.10.0.1");
        assert_eq!(svc.local_port, 8443);
    }

    #[test]
    fn four_fields_pin_the_bind_address() {
        let svc = build_service(
            &cfg_with_peer(),
            "web",
            "tcp",
            "203.0.113.9:443:shadow:8443",
        )
        .unwrap();
        assert_eq!(svc.remote_bind, "203.0.113.9:443");
    }

    #[test]
    fn an_unknown_peer_is_refused() {
        let err = build_service(&cfg_with_peer(), "web", "tcp", "443:nope:8443")
            .unwrap_err()
            .to_string();
        assert!(err.contains("no peer named"), "{err}");
    }

    #[test]
    fn a_nonsense_spec_is_refused() {
        for spec in ["", "notaport", "1:2:3:4:5", "443:shadow:0"] {
            assert!(
                build_service(&cfg_with_peer(), "web", "tcp", spec).is_err(),
                "{spec:?} was accepted"
            );
        }
    }

    #[test]
    fn a_peer_address_comes_out_of_its_single_allowed_ip() {
        assert_eq!(
            peer_address(&Peer {
                allowed_ips: vec!["10.10.0.1/32".into()],
                ..Peer::default()
            })
            .unwrap(),
            "10.10.0.1"
        );
        assert!(peer_address(&Peer {
            allowed_ips: vec!["10.10.0.0/24".into(), "10.20.0.0/24".into()],
            ..Peer::default()
        })
        .is_err());
    }
}
