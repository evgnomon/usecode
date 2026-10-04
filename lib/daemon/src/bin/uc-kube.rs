// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc kube`: Kubernetes (k3s) controllers and workers on your hosts, from wherever
//! you run `uc`. See [`uc_daemon::kube`] for what a host does with it.
//!
//! Like `uc net firewall`, every change is worked out against the host's
//! current /etc/uc/kube.toml and delivered back as a [`Bundle`] the
//! daemon there joins - it writes the file, reloads, and takes it from
//! there: downloads k3s, starts it, joins the others. `connect` then
//! puts the cluster in your kubeconfig.

use std::fs;
use std::io::Write;
use std::net::{IpAddr, ToSocketAddrs};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use serde_yaml::{Mapping, Value};
use uc_daemon::bundle::Bundle;
use uc_daemon::error::{Context, Result};
use uc_daemon::inventory::{self, Inventory};
use uc_daemon::kube::{self, Settings};
use uc_daemon::remote::{Target, inventory_host};
use uc_daemon::setup::BINARY_PATH;
use uc_daemon::{bail, err};

const SUMMARY: &str = "Kubernetes (k3s) on your hosts, and kubectl for them";

const USAGE: &str = "usage: uc kube COMMAND [ARGS...]

Kubernetes on your hosts, run by the usecode daemon there as a k3s
controller or worker. You say what you want; the daemon downloads k3s,
starts it, and keeps it running.

  enable HOST [HOST...] --to CONTROLLER
                              make HOST a worker of CONTROLLER's cluster:
                              it runs your pods, the controllers run the
                              cluster
      --version VERSION       run this k3s release (default: the
                              controller's)
  enable HOST [HOST...] --control
                              make HOST a controller and add the cluster to
                              your kubeconfig. One host is a cluster of its
                              own; three or more are one HA control plane
                              (embedded etcd), founded by the first. List
                              the whole control plane every time you grow
                              it: uc kube enable a b c --control
      --version VERSION       run this k3s release (e.g. v1.31.4+k3s1)
                              instead of the current stable one
      --no-connect            don't touch your kubeconfig
  connect HOST                add HOST's cluster to your kubeconfig and
                              switch kubectl to it
      --name NAME             the context's name (default: HOST)
      --server ADDRESS        the address kubectl uses (default: the one
                              ssh reaches HOST by)
  replace OLD NEW             swap OLD for NEW in its cluster, in the same
                              role: NEW joins at the release the cluster
                              runs, and once it's ready OLD is removed
                              (below). The control plane keeps its
                              quorum all the way
      --version VERSION       run this k3s release on NEW instead
      --via CONTROLLER        work through this controller (when OLD
                              can't be reached any more)
      --no-drain              don't wait for OLD's pods to move first
      --no-connect            don't point kubectl at NEW (controllers)
  remove HOST                 take HOST out of its cluster for good: drain
                              it, delete its node (and etcd member),
                              uninstall k3s there, and tell the rest -
                              forget its address, and join through
                              another controller if they joined through it
      --via CONTROLLER        work through this controller (when HOST
                              can't be reached any more)
      --no-drain              don't wait for HOST's pods to move first
  status HOST                 the settings, k3s, and the nodes
  disable HOST                stop k3s on HOST (its data is kept)
      --purge                 uninstall k3s and delete its data; a worker
                              stays listed until you kubectl delete node
  configure [OPTIONS]         set up the cluster kubectl points at, like
                              `uc configure` does this machine: Hetzner
                              Cloud Volumes for your PVCs, for a start.
                              `uc kube configure -h` for the options

HOST is an inventory name or any ssh destination ([USER@]HOST or an alias
from ~/.ssh/config). The daemon has to be on it: `uc daemon install HOST`.";

/// How long `connect` waits for a freshly started k3s to write its
/// kubeconfig. The first start downloads images, so be generous.
const READY_TIMEOUT: Duration = Duration::from_secs(300);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("uc kube: error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<()> {
    let Some((cmd, rest)) = args.split_first() else {
        eprintln!("{USAGE}");
        return Ok(());
    };
    match cmd.as_str() {
        "--summary" => {
            println!("{SUMMARY}");
            Ok(())
        }
        "help" | "-h" | "--help" => {
            println!("{USAGE}");
            Ok(())
        }
        "enable" => enable(rest),
        "connect" => connect_cmd(rest),
        "status" => status(rest),
        "disable" => disable(rest),
        "remove" => remove(rest),
        "replace" => replace(rest),
        "configure" => configure(rest),
        other => bail!("{USAGE}\n\nunknown command {other:?}"),
    }
}

/// Hands over to `uc-kube-configure`, found next to this executable or on
/// PATH. It only returns when that cannot be run.
fn configure(args: &[String]) -> Result<()> {
    use std::os::unix::process::CommandExt;
    const PROGRAM: &str = "uc-kube-configure";
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join(PROGRAM)))
        .filter(|p| p.is_file());
    let mut cmd = match beside {
        Some(path) => Command::new(path),
        None => Command::new(PROGRAM),
    };
    let err = cmd.arg0(PROGRAM).args(args).exec();
    bail!("running {PROGRAM}: {err}")
}

/// Positional words and `(--flag, value)` pairs, as [`parse`] splits them.
type Parsed<'a> = (Vec<&'a str>, Vec<(&'a str, &'a str)>);

/// Split `args` into positional words and `--flag [value]` options, the
/// ones in `valued` taking a value.
fn parse<'a>(args: &'a [String], valued: &[&str], bare: &[&str]) -> Result<Parsed<'a>> {
    let mut words = Vec::new();
    let mut flags = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let arg = arg.as_str();
        if valued.contains(&arg) {
            let value = it.next().ok_or_else(|| err!("{arg} needs a value"))?;
            flags.push((arg, value.as_str()));
        } else if bare.contains(&arg) {
            flags.push((arg, ""));
        } else if arg.starts_with("--") {
            bail!("unknown option {arg}");
        } else {
            words.push(arg);
        }
    }
    Ok((words, flags))
}

fn flag<'a>(flags: &[(&str, &'a str)], name: &str) -> Option<&'a str> {
    flags.iter().find(|(f, _)| *f == name).map(|(_, v)| *v)
}

/// One host of the control plane being set up.
struct Node {
    name: String,
    target: Target,
    settings: Settings,
    /// Where kubectl (and the certificate) reaches it.
    public: String,
    /// Where the other controllers reach it: its mesh address when it is
    /// in the mesh, its public one otherwise.
    cluster: String,
}

const ENABLE_USAGE: &str =
    "usage: uc kube enable HOST [HOST...] --to CONTROLLER [--version VERSION]
       uc kube enable HOST [HOST...] --control [--version VERSION] [--no-connect]";

/// Workers by default; controllers with `--control`.
fn enable(args: &[String]) -> Result<()> {
    let (names, flags) = parse(args, &["--to", "--version"], &["--control", "--no-connect"])?;
    if names.is_empty() {
        bail!("{ENABLE_USAGE}");
    }
    match (flag(&flags, "--control"), flag(&flags, "--to")) {
        (Some(_), None) => enable_control(&names, &flags),
        (None, Some(ctrl)) if flag(&flags, "--no-connect").is_none() => {
            enable_workers(&names, ctrl, &flags)
        }
        (None, Some(_)) => bail!("--no-connect is for controllers (--control)"),
        (Some(_), Some(_)) => bail!(
            "--control and --to don't go together: a host is a controller or a worker\n\n{ENABLE_USAGE}"
        ),
        (None, None) => bail!(
            "say which cluster the worker(s) join with --to CONTROLLER, or make them \
             controllers with --control\n\n{ENABLE_USAGE}"
        ),
    }
}

/// Make `names` the control plane: one host alone, or the first founding
/// an HA one the rest join.
fn enable_control(names: &[&str], flags: &[(&str, &str)]) -> Result<()> {
    if names.len() == 2 {
        eprintln!(
            "note: an HA control plane needs three controllers to survive losing one; \
             two is less available than one. Add a third when you can."
        );
    }

    let mut nodes = Vec::new();
    for name in names {
        let target = Target::reach(name)?;
        let settings = read_settings(&target)?;
        if settings.agent && settings.enabled {
            bail!(
                "{name} is a worker; `uc kube disable {name} --purge` first to make it a controller"
            );
        }
        let public = public_address(name)?;
        let cluster = cluster_address(name, &public)?;
        nodes.push(Node {
            name: name.to_string(),
            target,
            settings,
            public,
            cluster,
        });
    }

    let ha = nodes.len() > 1;
    // The token every controller shares: the one the founder already has
    // (it may have run alone until now), or a new one.
    let token = if ha {
        let founder = &nodes[0];
        if !founder.settings.token.is_empty() {
            founder.settings.token.clone()
        } else {
            match read_token(&founder.target)? {
                Some(t) => t,
                None => new_token()?,
            }
        }
    } else {
        String::new()
    };
    let server = format!("https://{}:{}", bracket(&nodes[0].cluster), kube::API_PORT);
    let cluster_addrs: Vec<String> = nodes.iter().map(|n| n.cluster.clone()).collect();

    for (i, node) in nodes.iter_mut().enumerate() {
        let s = &mut node.settings;
        s.enabled = true;
        s.purge = false;
        s.agent = false;
        if let Some(v) = flag(flags, "--version") {
            s.version = v.to_string();
        }
        if !s.tls_san.contains(&node.public) {
            s.tls_san.push(node.public.clone());
        }
        if ha {
            s.cluster_init = i == 0;
            s.server = if i == 0 {
                String::new()
            } else {
                server.clone()
            };
            s.token = token.clone();
            // Keep the peers it has (its workers) and add the others.
            add_peers(s, &cluster_addrs, &node.cluster);
        }
        // A single host keeps whatever cluster it was part of: enabling it
        // again after a disable shouldn't make it leave its control plane.
        s.validate()?;
    }

    // The founder first, so the others have someone to join.
    for node in &nodes {
        deliver(&node.target, &node.name, &node.settings)?;
        println!(
            "{}: Kubernetes controller{}",
            node.name,
            match (ha, node.settings.cluster_init) {
                (false, _) => String::new(),
                (true, true) => " (founds the HA control plane)".into(),
                (true, false) => format!(" (joins {})", nodes[0].name),
            }
        );
    }

    if flag(flags, "--no-connect").is_some() {
        println!("\nWhen it's up: uc kube connect {}", nodes[0].name);
        return Ok(());
    }
    let first = &nodes[0];
    connect(&first.target, &first.name, &first.name, &first.public)
}

/// Make `names` workers of the cluster `ctrl_name` is a controller of.
fn enable_workers(names: &[&str], ctrl_name: &str, flags: &[(&str, &str)]) -> Result<()> {
    let ctrl = Target::reach(ctrl_name)?;
    let ctrl_settings = read_settings(&ctrl)?;
    if !ctrl_settings.enabled || ctrl_settings.agent {
        bail!(
            "{ctrl_name} is not a Kubernetes controller; `uc kube enable {ctrl_name} --control` makes it one"
        );
    }
    let token = match ctrl_settings.token.as_str() {
        "" => read_token(&ctrl)?.ok_or_else(|| {
            err!("k3s on {ctrl_name} isn't up yet; `uc kube status {ctrl_name}` shows when it is")
        })?,
        t => t.to_string(),
    };
    let ctrl_cluster = cluster_address(ctrl_name, &public_address(ctrl_name)?)?;
    let server = format!("https://{}:{}", bracket(&ctrl_cluster), kube::API_PORT);
    let version = flag(flags, "--version").unwrap_or(&ctrl_settings.version);

    let mut workers = Vec::new();
    for name in names {
        let target = Target::reach(name)?;
        let settings = read_settings(&target)?;
        if settings.enabled && !settings.agent {
            bail!(
                "{name} is a controller; `uc kube disable {name} --purge` first to make it a worker"
            );
        }
        let public = public_address(name)?;
        let cluster = cluster_address(name, &public)?;
        workers.push(Node {
            name: name.to_string(),
            target,
            settings,
            public,
            cluster,
        });
    }
    let worker_addrs: Vec<String> = workers.iter().map(|w| w.cluster.clone()).collect();

    // Every node already in the cluster lets the new workers in (it only
    // matters off the mesh, where the firewall goes by address). The
    // controller knows them all as its peers.
    let mut known = vec![ctrl_cluster.clone()];
    known.extend(ctrl_settings.peers.iter().cloned());
    let mut missed = Vec::new();
    for addr in &known {
        if worker_addrs.contains(addr) {
            continue;
        }
        let Some(name) = host_at(addr, ctrl_name, &ctrl_cluster) else {
            missed.push(addr.clone());
            continue;
        };
        let target = Target::reach(&name)?;
        let mut settings = read_settings(&target)?;
        if !settings.enabled {
            continue;
        }
        let before = settings.peers.len();
        add_peers(&mut settings, &worker_addrs, addr);
        if settings.peers.len() != before {
            deliver(&target, &name, &settings)?;
            println!("{name}: lets the new worker(s) in");
        }
    }

    for w in &mut workers {
        let s = &mut w.settings;
        *s = Settings {
            enabled: true,
            agent: true,
            server: server.clone(),
            token: token.clone(),
            version: version.to_string(),
            peers: s.peers.clone(),
            ..Settings::default()
        };
        add_peers(s, &known, &w.cluster);
        add_peers(s, &worker_addrs, &w.cluster);
        s.validate()?;
        deliver(&w.target, &w.name, s)?;
        println!("{}: Kubernetes worker (joins {ctrl_name})", w.name);
    }

    if !missed.is_empty() {
        eprintln!(
            "note: no inventory host has the address {}; if it's off the mesh, its \
             firewall may not let the new workers in yet",
            missed.join(", ")
        );
    }
    println!("\nThey show up in a minute or so: kubectl get nodes");
    Ok(())
}

/// Add `addrs` to the peers of `settings`, all but the node's own
/// address `own`, and only addresses (the firewall can't use names).
fn add_peers(settings: &mut Settings, addrs: &[String], own: &str) {
    for a in addrs {
        if a != own && a.parse::<IpAddr>().is_ok() && !settings.peers.contains(a) {
            settings.peers.push(a.clone());
        }
    }
}

/// The inventory host the cluster reaches at `addr`, if there is one.
/// The controller itself is known by the name it was given.
fn host_at(addr: &str, ctrl_name: &str, ctrl_cluster: &str) -> Option<String> {
    if addr == ctrl_cluster {
        return Some(ctrl_name.to_string());
    }
    inventory_name_at(addr)
}

/// The inventory host the cluster reaches at `addr`, if there is one.
fn inventory_name_at(addr: &str) -> Option<String> {
    let inv = inventory::find()
        .ok()
        .and_then(|dir| Inventory::load(&dir).ok())?;
    inv.hosts
        .iter()
        .find(|h| {
            public_address(&h.name)
                .and_then(|p| cluster_address(&h.name, &p))
                .is_ok_and(|a| a == addr)
        })
        .map(|h| h.name.clone())
}

fn connect_cmd(args: &[String]) -> Result<()> {
    let usage = "usage: uc kube connect HOST [--name NAME] [--server ADDRESS]";
    let (words, flags) = parse(args, &["--name", "--server"], &[])?;
    let [name] = words.as_slice() else {
        bail!("{usage}");
    };
    let target = Target::reach(name)?;
    let address = match flag(&flags, "--server") {
        Some(a) => a.to_string(),
        None => public_address(name)?,
    };

    // The API certificate has to be valid for the address kubectl uses.
    let mut settings = read_settings(&target)?;
    if !settings.enabled || settings.agent {
        bail!(
            "{name} is not a Kubernetes controller; `uc kube enable {name} --control` makes it one"
        );
    }
    if !settings.tls_san.contains(&address) {
        settings.tls_san.push(address.clone());
        deliver(&target, name, &settings)?;
    }

    connect(
        &target,
        name,
        flag(&flags, "--name").unwrap_or(name),
        &address,
    )
}

/// Wait for k3s on `target` to write its admin kubeconfig, then merge it
/// into the local kubeconfig as context `context`, pointing at
/// `address`, and switch to it.
fn connect(target: &Target, host: &str, context: &str, address: &str) -> Result<()> {
    let remote = wait_for_kubeconfig(target, host)?;
    let mut incoming: Value = serde_yaml::from_str(&remote).ctx("parse the host's kubeconfig")?;
    let server = format!(
        "https://{}:{}",
        bracket(address.trim_start_matches("https://")),
        kube::API_PORT
    );
    rename_and_point(&mut incoming, context, &server)?;

    let path = local_kubeconfig()?;
    let mut local: Value = match fs::read_to_string(&path) {
        Ok(body) if !body.trim().is_empty() => {
            serde_yaml::from_str(&body).with_ctx(|| format!("parse {}", path.display()))?
        }
        Ok(_) => Value::Null,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Value::Null,
        Err(e) => bail!("read {}: {e}", path.display()),
    };
    merge(&mut local, &incoming, context)?;
    write_private(
        &path,
        &serde_yaml::to_string(&local).ctx("encode kubeconfig")?,
    )?;

    println!(
        "\nkubectl now talks to {context} ({server}), saved in {}.\nTry: kubectl get nodes",
        path.display()
    );
    Ok(())
}

fn wait_for_kubeconfig(target: &Target, host: &str) -> Result<String> {
    let started = Instant::now();
    let mut said = false;
    loop {
        let body = target
            .output_as_root(&sh(&format!(
                "cat {} 2>/dev/null || true",
                kube::KUBECONFIG_PATH
            )))
            .ctx("read the kubeconfig (root, through sudo -n)")?;
        if !body.trim().is_empty() {
            return Ok(body);
        }
        if started.elapsed() > READY_TIMEOUT {
            bail!(
                "k3s on {host} hasn't come up in {}s; `uc kube status {host}` shows what it's \
                 doing, and `uc kube connect {host}` picks up from here",
                READY_TIMEOUT.as_secs()
            );
        }
        if !said {
            println!("waiting for k3s on {host} to come up (the first start takes a minute)...");
            said = true;
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}

/// k3s calls everything in its kubeconfig "default" and points it at
/// 127.0.0.1. Rename the cluster, user and context to `name` and point
/// the cluster at `server`.
fn rename_and_point(config: &mut Value, name: &str, server: &str) -> Result<()> {
    for (list, inner) in [
        ("clusters", "cluster"),
        ("users", "user"),
        ("contexts", "context"),
    ] {
        let entries = config
            .get_mut(list)
            .and_then(Value::as_sequence_mut)
            .ok_or_else(|| err!("the host's kubeconfig has no {list}"))?;
        for entry in entries {
            entry["name"] = name.into();
            match list {
                "clusters" => entry[inner]["server"] = server.into(),
                "contexts" => {
                    entry[inner]["cluster"] = name.into();
                    entry[inner]["user"] = name.into();
                }
                _ => {}
            }
        }
    }
    config["current-context"] = name.into();
    Ok(())
}

/// Put `incoming`'s clusters, users and contexts into `local`, replacing
/// entries of the same name, and make `context` the current one.
fn merge(local: &mut Value, incoming: &Value, context: &str) -> Result<()> {
    if !local.is_mapping() {
        let mut m = Mapping::new();
        m.insert("apiVersion".into(), "v1".into());
        m.insert("kind".into(), "Config".into());
        m.insert("preferences".into(), Value::Mapping(Mapping::new()));
        *local = Value::Mapping(m);
    }
    for list in ["clusters", "users", "contexts"] {
        let new = incoming
            .get(list)
            .and_then(Value::as_sequence)
            .cloned()
            .unwrap_or_default();
        let slot = &mut local[list];
        if !slot.is_sequence() {
            *slot = Value::Sequence(Vec::new());
        }
        let entries = slot.as_sequence_mut().expect("just made a sequence");
        for entry in new {
            let name = entry.get("name").cloned();
            entries.retain(|e| e.get("name") != name.as_ref());
            entries.push(entry);
        }
    }
    local["current-context"] = context.into();
    Ok(())
}

/// The kubeconfig kubectl reads: the first file in $KUBECONFIG, or
/// ~/.kube/config.
fn local_kubeconfig() -> Result<PathBuf> {
    if let Some(first) = std::env::var_os("KUBECONFIG")
        .as_ref()
        .and_then(|v| std::env::split_paths(v).find(|p| !p.as_os_str().is_empty()))
    {
        return Ok(first);
    }
    let home = std::env::var_os("HOME").ok_or_else(|| err!("HOME is not set"))?;
    Ok(PathBuf::from(home).join(".kube").join("config"))
}

/// Write `body` to `path`, readable by you alone: it holds the cluster's
/// admin credentials.
fn write_private(path: &PathBuf, body: &str) -> Result<()> {
    if let Some(dir) = path.parent()
        && !dir.exists()
    {
        fs::create_dir_all(dir).with_ctx(|| format!("create {}", dir.display()))?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .with_ctx(|| format!("chmod {}", dir.display()))?;
    }
    let tmp = path.with_extension("uc-tmp");
    let write = || -> std::io::Result<()> {
        let _ = fs::remove_file(&tmp);
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(body.as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    };
    write().map_err(|e| {
        let _ = fs::remove_file(&tmp);
        err!("write {}: {e}", path.display())
    })
}

fn status(args: &[String]) -> Result<()> {
    let [name] = args else {
        bail!("usage: uc kube status HOST");
    };
    let target = Target::reach(name)?;
    let s = read_settings(&target)?;

    println!(
        "{name}: Kubernetes {}",
        if s.enabled { "on" } else { "off" }
    );
    if s.agent {
        println!("  role      worker, joined to {}", s.server);
    } else if s.cluster_init {
        println!("  role      first controller of an HA control plane");
    } else if !s.server.is_empty() {
        println!("  role      controller, joined to {}", s.server);
    } else if s.enabled {
        println!("  role      single controller");
    }
    println!(
        "  version   {}",
        if s.version.is_empty() {
            "stable"
        } else {
            &s.version
        }
    );
    for san in &s.tls_san {
        println!("  api name  {san}");
    }
    for peer in &s.peers {
        println!("  peer      {peer}");
    }

    let unit = if s.agent { "k3s-agent" } else { "k3s" };
    let k3s = target
        .output_as_root(&sh(&format!(
            "systemctl is-active {unit} 2>/dev/null || true"
        )))
        .unwrap_or_else(|e| e.to_string());
    println!(
        "\nk3s: {}",
        if k3s.is_empty() {
            "not installed"
        } else {
            &k3s
        }
    );
    if k3s == "active" && !s.agent {
        match target.output_as_root(&sh("k3s kubectl get nodes -o wide 2>&1")) {
            Ok(nodes) => println!("\n{nodes}"),
            Err(e) => println!("\nnodes: {e}"),
        }
    }
    Ok(())
}

fn disable(args: &[String]) -> Result<()> {
    let (words, flags) = parse(args, &[], &["--purge"])?;
    let [name] = words.as_slice() else {
        bail!("usage: uc kube disable HOST [--purge]");
    };
    let target = Target::reach(name)?;
    let mut settings = read_settings(&target)?;
    settings.enabled = false;
    settings.purge = flag(&flags, "--purge").is_some();
    if settings.purge {
        // Nothing of the old cluster is left to rejoin.
        settings = Settings {
            purge: true,
            ..Settings::default()
        };
    }
    deliver(&target, name, &settings)?;
    println!(
        "{name}: Kubernetes {}",
        if settings.purge {
            "uninstalled"
        } else {
            "stopped (its data is kept)"
        }
    );
    Ok(())
}

/// How long `replace` waits for the new node to join and be ready. It
/// downloads k3s and its images first.
const JOIN_TIMEOUT: Duration = Duration::from_secs(600);

const REMOVE_USAGE: &str = "usage: uc kube remove HOST [--via CONTROLLER] [--no-drain]";

const REPLACE_USAGE: &str = "usage: uc kube replace OLD NEW [--version VERSION] \
     [--via CONTROLLER] [--no-drain] [--no-connect]";

fn remove(args: &[String]) -> Result<()> {
    let (words, flags) = parse(args, &["--via"], &["--no-drain"])?;
    let [name] = words.as_slice() else {
        bail!("{REMOVE_USAGE}");
    };
    let old = Leaving::find(name)?;
    let mut rest = cluster_without(&old, flag(&flags, "--via"))?;
    if take_out(&old, &mut rest, flag(&flags, "--no-drain").is_none())? {
        let successor = rest
            .iter_mut()
            .filter(|n| !n.settings.agent)
            .max_by_key(|n| n.settings.cluster_init)
            .expect("take_out leaves a controller");
        retire_kubeconfig(&old, successor)?;
    }
    Ok(())
}

fn replace(args: &[String]) -> Result<()> {
    let (words, flags) = parse(
        args,
        &["--via", "--version"],
        &["--no-drain", "--no-connect"],
    )?;
    let [old_name, new_name] = words.as_slice() else {
        bail!("{REPLACE_USAGE}");
    };
    let mut old = Leaving::find(old_name)?;
    let mut new = node_for(new_name)?;
    if new.cluster == old.cluster {
        bail!("{old_name} and {new_name} are the same host");
    }
    if new.settings.enabled {
        bail!(
            "{new_name} already runs Kubernetes; `uc kube disable {new_name} --purge` first \
             to start it fresh"
        );
    }
    let mut rest = cluster_without(&old, flag(&flags, "--via"))?;

    // Who the new node joins through: a controller staying in the
    // cluster, or the old one when it is the only one.
    let lone = !rest.iter().any(|n| !n.settings.agent);
    if lone {
        let Some(node) = old.node.as_mut().filter(|n| !n.settings.agent) else {
            bail!(
                "found no controller of {old_name}'s cluster to work through; name one with \
                 --via CONTROLLER"
            );
        };
        found_control_plane(node)?;
    }
    let via_at = rest.iter().position(|n| !n.settings.agent);
    let via = match via_at {
        Some(i) => &rest[i],
        None => old.node.as_ref().expect("checked above"),
    };

    let control = match &old.node {
        Some(n) if n.settings.enabled => !n.settings.agent,
        _ => {
            let nodes = kube_nodes(&via.target)?;
            find_node(&nodes, &old.addresses(), &old.names())
                .ok_or_else(|| err!("{} has no node for {old_name}", via.name))?
                .control
        }
    };
    let version = match (flag(&flags, "--version"), &old.node) {
        (Some(v), _) => v.to_string(),
        (None, Some(n)) if !n.settings.version.is_empty() => n.settings.version.clone(),
        (None, _) => running_version(&via.target)
            .with_ctx(|| format!("ask {} which k3s it runs", via.name))?,
    };
    let token = match via.settings.token.as_str() {
        "" => read_token(&via.target)?.ok_or_else(|| {
            err!(
                "k3s on {} isn't up; `uc kube status {}`",
                via.name,
                via.name
            )
        })?,
        t => t.to_string(),
    };

    let mut everyone: Vec<String> = rest.iter().map(|n| n.cluster.clone()).collect();
    everyone.push(old.cluster.clone());
    new.settings = Settings {
        enabled: true,
        agent: !control,
        server: api_url(&via.cluster),
        token,
        version,
        tls_san: if control {
            vec![new.public.clone()]
        } else {
            Vec::new()
        },
        ..Settings::default()
    };
    add_peers(&mut new.settings, &everyone, &new.cluster);
    new.settings.validate()?;
    let via_name = via.name.clone();

    // Everyone lets the new node in before it knocks.
    for node in rest.iter_mut().chain(old.node.as_mut()) {
        let before = node.settings.clone();
        add_peers(
            &mut node.settings,
            std::slice::from_ref(&new.cluster),
            &node.cluster,
        );
        redeliver(node, &before)?;
    }
    deliver(&new.target, &new.name, &new.settings)?;
    let role = if control { "controller" } else { "worker" };
    println!(
        "{}: Kubernetes {role} (joins {}, k3s {})",
        new.name, via_name, new.settings.version
    );
    let via = match via_at {
        Some(i) => &rest[i],
        None => old.node.as_ref().expect("checked above"),
    };
    wait_for_node(via, &new)?;
    println!("{}: ready", new.name);

    rest.push(new);
    take_out(&old, &mut rest, flag(&flags, "--no-drain").is_none())?;

    let new = rest.last_mut().expect("just pushed");
    if !control {
        return Ok(());
    }
    retire_kubeconfig(&old, new)?;
    if flag(&flags, "--no-connect").is_some() {
        println!("Point kubectl at it: uc kube connect {}", new.name);
        return Ok(());
    }
    connect(&new.target, &new.name, &new.name, &new.public)
}

/// The host leaving its cluster, as far as it can still be reached.
struct Leaving {
    name: String,
    public: String,
    cluster: String,
    /// The host itself, when it can be reached and runs the daemon.
    node: Option<Node>,
    /// Its host name, which k3s names the node by.
    hostname: Option<String>,
}

impl Leaving {
    /// Find `name`, carrying on without it when it's gone: its addresses
    /// come from the inventory or ssh's config either way.
    fn find(name: &str) -> Result<Leaving> {
        let public = public_address(name)?;
        // A gone host may be known by its node name only.
        let cluster = cluster_address(name, &public).unwrap_or_else(|_| public.clone());
        let node = match Target::reach(name).and_then(|target| {
            let settings = read_settings(&target)?;
            Ok((target, settings))
        }) {
            Ok((target, settings)) => Some(Node {
                name: name.to_string(),
                target,
                settings,
                public: public.clone(),
                cluster: cluster.clone(),
            }),
            Err(e) => {
                eprintln!("note: {name} can't be reached ({e}); going on without it");
                None
            }
        };
        let hostname = node.as_ref().and_then(|n| n.target.output("hostname").ok());
        Ok(Leaving {
            name: name.to_string(),
            public,
            cluster,
            node,
            hostname,
        })
    }

    fn addresses(&self) -> Vec<&str> {
        vec![&self.cluster, &self.public]
    }

    fn names(&self) -> Vec<&str> {
        let mut names = vec![self.name.as_str()];
        names.extend(self.hostname.as_deref());
        names
    }
}

/// `name` as a member of a cluster.
fn node_for(name: &str) -> Result<Node> {
    let target = Target::reach(name)?;
    let settings = read_settings(&target)?;
    let public = public_address(name)?;
    let cluster = cluster_address(name, &public)?;
    Ok(Node {
        name: name.to_string(),
        target,
        settings,
        public,
        cluster,
    })
}

/// The rest of `old`'s cluster: every node the cluster lists, found by
/// ssh, `via` (a controller named on the command line) first. Nodes that
/// are down are left out; a node that is up but can't be found by ssh
/// stops it here, before anything has changed.
fn cluster_without(old: &Leaving, via: Option<&str>) -> Result<Vec<Node>> {
    let mut rest = Vec::new();
    if let Some(v) = via {
        let node = node_for(v)?;
        if !node.settings.enabled || node.settings.agent {
            bail!("{v} is not a Kubernetes controller");
        }
        rest.push(node);
    }
    // Who lists the nodes: `via`, or the old host while it still runs
    // as a controller, or the controller a worker joined through.
    let lister = match (rest.first(), &old.node) {
        (Some(v), _) => &v.target,
        (None, Some(n)) if n.settings.enabled && !n.settings.agent => &n.target,
        (None, Some(n)) if n.settings.enabled => {
            let addr = server_address(&n.settings.server).unwrap_or_default();
            let name = destination_for(None, std::slice::from_ref(&addr))
                .ok_or_else(|| err!("can't reach {addr}, the controller {} joined", old.name))?;
            rest.push(node_for(&name)?);
            &rest[0].target
        }
        _ => bail!(
            "can't tell which cluster {} is in; name one of its controllers with --via CONTROLLER",
            old.name
        ),
    };
    let nodes = kube_nodes(lister)
        .ctx("list the cluster's nodes (name a running controller with --via CONTROLLER)")?;
    if old.node.is_none() && find_node(&nodes, &old.addresses(), &old.names()).is_none() {
        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        bail!(
            "the cluster has no node named {} or at {}; nothing was changed. Its nodes are: \
             {}. Name the one to take out by its node name",
            old.name,
            old.cluster,
            names.join(", ")
        );
    }

    let mut lost = Vec::new();
    for kn in &nodes {
        let is = |n: &Node| kn.addresses.contains(&n.cluster) || kn.name == n.name;
        if find_node(std::slice::from_ref(kn), &old.addresses(), &old.names()).is_some()
            || rest.iter().any(is)
        {
            continue;
        }
        match destination_for(Some(&kn.name), &kn.addresses).map(|d| node_for(&d)) {
            Some(Ok(mut node)) if node.settings.enabled => {
                // The cluster knows it by its node address (its mesh
                // address, for one ssh reaches by another).
                if !kn.addresses.contains(&node.cluster)
                    && let Some(a) = kn.addresses.first()
                {
                    node.cluster = a.clone();
                }
                rest.push(node);
            }
            _ if !kn.ready => {
                eprintln!("note: {} is down; it's left as it is", kn.name)
            }
            _ => lost.push(format!("{} ({})", kn.name, kn.addresses.join(", "))),
        }
    }
    if !lost.is_empty() {
        bail!(
            "can't reach {} by ssh, so it couldn't be told; nothing was changed. Add it to \
             the inventory or ~/.ssh/config (and `uc daemon install` it), then run this again",
            lost.join(", ")
        );
    }
    Ok(rest)
}

/// An ssh destination for the node `name` at `addresses`: the inventory
/// host at one of them, or else the first of its name and addresses ssh
/// gets into without asking, that says it is that node.
fn destination_for(name: Option<&str>, addresses: &[String]) -> Option<String> {
    if let Some(host) = addresses.iter().find_map(|a| inventory_name_at(a)) {
        return Some(host);
    }
    name.into_iter()
        .chain(addresses.iter().map(String::as_str))
        .find(|dest| {
            Command::new("ssh")
                .args([
                    "-o",
                    "BatchMode=yes",
                    "-o",
                    "ConnectTimeout=5",
                    dest,
                    "hostname",
                ])
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .is_ok_and(|out| {
                    let host = String::from_utf8_lossy(&out.stdout).trim().to_lowercase();
                    out.status.success() && name.is_none_or(|n| n.eq_ignore_ascii_case(&host))
                })
        })
        .map(str::to_string)
}

/// The address in an API URL like `https://[2001:db8::1]:6443`.
fn server_address(url: &str) -> Option<String> {
    let (host, _port) = url.strip_prefix("https://")?.rsplit_once(':')?;
    Some(
        host.trim_start_matches('[')
            .trim_end_matches(']')
            .to_string(),
    )
}

fn api_url(addr: &str) -> String {
    format!("https://{}:{}", bracket(addr), kube::API_PORT)
}

/// Take `old` out of the cluster that is left as `rest`: drain and delete
/// its node (k3s drops its etcd member with it), uninstall k3s there, and
/// tell the rest. Controllers that have to restart for it do so one at a
/// time, so the control plane keeps its quorum.
fn take_out(old: &Leaving, rest: &mut [Node], drain: bool) -> Result<bool> {
    let Some(via) = rest.iter().find(|n| !n.settings.agent) else {
        bail!(
            "found no other controller in {name}'s cluster. Name one with --via CONTROLLER; or, \
             if {name} is the only one, `uc kube disable {name} --purge` takes the cluster down \
             with it",
            name = old.name
        );
    };

    let nodes = kube_nodes(&via.target).with_ctx(|| format!("list the nodes on {}", via.name))?;
    let was_control = match find_node(&nodes, &old.addresses(), &old.names()) {
        Some(node) => {
            if drain && old.node.is_some() {
                println!(
                    "{}: draining (its pods move to the other nodes)...",
                    old.name
                );
                kubectl(
                    &via.target,
                    &[
                        "drain",
                        &node.name,
                        "--ignore-daemonsets",
                        "--delete-emptydir-data",
                        "--timeout=300s",
                    ],
                )
                .map_err(|e| {
                    err!(
                        "drain {}: {e}. `kubectl drain {}` shows what's holding it up, and \
                         --no-drain skips this",
                        old.name,
                        node.name
                    )
                })?;
            }
            kubectl(&via.target, &["delete", "node", &node.name])?;
            println!("{}: node {} deleted", old.name, node.name);
            node.control
        }
        None => {
            eprintln!(
                "note: the cluster has no node for {}; nothing to delete",
                old.name
            );
            old.node.as_ref().is_some_and(|n| !n.settings.agent)
        }
    };
    if was_control {
        etcd_forget(via, old)?;
    }

    match &old.node {
        Some(node) => match deliver(
            &node.target,
            &old.name,
            &Settings {
                purge: true,
                ..Settings::default()
            },
        ) {
            Ok(()) => println!("{}: k3s uninstalled", old.name),
            Err(e) => eprintln!("note: uninstalling k3s on {}: {e}", old.name),
        },
        None => eprintln!(
            "note: k3s is still installed on {name}; `uc kube disable {name} --purge` if it \
             comes back",
            name = old.name
        ),
    }

    let before: Vec<Settings> = rest.iter().map(|n| n.settings.clone()).collect();
    let mut members: Vec<(String, Settings)> = rest
        .iter()
        .map(|n| (n.cluster.clone(), n.settings.clone()))
        .collect();
    repoint(&mut members, &old.cluster);
    for (node, (_, settings)) in rest.iter_mut().zip(members) {
        node.settings = settings;
    }
    // Controllers first, one by one, then the workers.
    let order = (0..rest.len())
        .filter(|&i| !rest[i].settings.agent)
        .chain((0..rest.len()).filter(|&i| rest[i].settings.agent));
    for i in order {
        let (node, was) = (&rest[i], &before[i]);
        if node.settings.cluster_init && !was.cluster_init {
            println!(
                "{}: leads the control plane now (restarting k3s)...",
                node.name
            );
        } else if node.settings.server != was.server {
            println!(
                "{}: joins through {} now",
                node.name,
                server_address(&node.settings.server).unwrap_or_default()
            );
        }
        redeliver(node, was)?;
    }

    println!("\n{} is out of the cluster.", old.name);
    let controllers = rest.iter().filter(|n| !n.settings.agent).count();
    if was_control && controllers == 2 {
        eprintln!(
            "note: two controllers can't lose one; add a third: uc kube enable ... --control"
        );
    }
    Ok(was_control)
}

/// Where k3s keeps the certificates its own etcd client uses.
const ETCD_TLS: &str = "/var/lib/rancher/k3s/server/tls/etcd";

/// How long k3s gets to drop a deleted controller's etcd member itself.
const ETCD_GRACE: Duration = Duration::from_secs(30);

/// Make sure `old` is out of etcd. k3s drops the member when its node is
/// deleted; if it hasn't in a while, or the node was gone already, the
/// member is removed here, through `via`.
fn etcd_forget(via: &Node, old: &Leaving) -> Result<()> {
    if !via.settings.cluster_init && via.settings.server.is_empty() {
        return Ok(()); // a lone controller: no etcd to leave
    }
    let started = Instant::now();
    let mut removed = false;
    loop {
        let members = etcd(&via.target, "list", "{}")?;
        let Some(id) = etcd_member(&members, &old.addresses(), &old.names()) else {
            if removed {
                println!("{}: etcd member removed", old.name);
            }
            return Ok(());
        };
        if !removed && started.elapsed() > ETCD_GRACE {
            etcd(&via.target, "remove", &format!(r#"{{"ID":"{id}"}}"#))?;
            removed = true;
        } else if started.elapsed() > ETCD_GRACE * 2 {
            bail!(
                "{}'s etcd member {id} is still there after removing it; the control plane \
                 may not have quorum: `uc kube status {}`",
                old.name,
                via.name
            );
        }
        std::thread::sleep(Duration::from_secs(3));
    }
}

/// Call the etcd cluster API on the controller `via` (`list`, `remove`),
/// as k3s's own client.
fn etcd(via: &Target, call: &str, body: &str) -> Result<Value> {
    let argv: Vec<String> = [
        "curl",
        "-sSf",
        "--cacert",
        &format!("{ETCD_TLS}/server-ca.crt"),
        "--cert",
        &format!("{ETCD_TLS}/client.crt"),
        "--key",
        &format!("{ETCD_TLS}/client.key"),
        "-X",
        "POST",
        &format!("https://127.0.0.1:2379/v3/cluster/member/{call}"),
        "-d",
        body,
    ]
    .iter()
    .map(|a| a.to_string())
    .collect();
    let out = via
        .output_as_root(&argv)
        .with_ctx(|| format!("etcd member {call}"))?;
    serde_yaml::from_str(&out).ctx("parse etcd's answer")
}

/// The ID of the etcd member, in a member list, of the node at one of
/// `addresses` or named one of `names` (k3s names members
/// `<node>-<random>`).
fn etcd_member(list: &Value, addresses: &[&str], names: &[&str]) -> Option<String> {
    let member = list.get("members")?.as_sequence()?.iter().find(|m| {
        let peer = m
            .get("peerURLs")
            .and_then(Value::as_sequence)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(server_address)
            .any(|a| addresses.contains(&a.as_str()));
        let named = m
            .get("name")
            .and_then(Value::as_str)
            .and_then(|n| n.rsplit_once('-'))
            .is_some_and(|(node, _)| names.iter().any(|w| w.eq_ignore_ascii_case(node)));
        peer || named
    })?;
    match member.get("ID")? {
        Value::String(id) => Some(id.clone()),
        Value::Number(id) => Some(id.to_string()),
        _ => None,
    }
}

/// Point what in the local kubeconfig talked to `old` at `successor`, a
/// controller of the same cluster (same CA, same credentials); entries
/// named after `old` take `successor`'s name.
fn retire_kubeconfig(old: &Leaving, successor: &mut Node) -> Result<()> {
    let path = local_kubeconfig()?;
    let Ok(body) = fs::read_to_string(&path) else {
        return Ok(());
    };
    let Ok(mut config) = serde_yaml::from_str::<Value>(&body) else {
        return Ok(());
    };
    let mut ids = old.addresses();
    ids.extend(old.names());
    let touched = point_clusters(&mut config, &ids, &api_url(&successor.public));
    if touched.is_empty() {
        return Ok(());
    }
    if touched.contains(&old.name) {
        rename_entries(&mut config, &old.name, &successor.name);
    }
    // Its certificate has to name the address kubectl now uses.
    if !successor.settings.tls_san.contains(&successor.public) {
        let before = successor.settings.clone();
        successor.settings.tls_san.push(successor.public.clone());
        redeliver(successor, &before)?;
    }
    write_private(
        &path,
        &serde_yaml::to_string(&config).ctx("encode kubeconfig")?,
    )?;
    println!(
        "kubectl: {} talks to {} now",
        touched.join(", "),
        successor.name
    );
    Ok(())
}

/// Point the clusters whose server is one of `ids` at `server`; their
/// names.
fn point_clusters(config: &mut Value, ids: &[&str], server: &str) -> Vec<String> {
    let mut touched = Vec::new();
    let Some(clusters) = config.get_mut("clusters").and_then(Value::as_sequence_mut) else {
        return touched;
    };
    for entry in clusters {
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let Some(slot) = entry.get_mut("cluster").and_then(|c| c.get_mut("server")) else {
            continue;
        };
        if slot
            .as_str()
            .and_then(server_address)
            .is_some_and(|h| ids.contains(&h.as_str()))
        {
            *slot = server.into();
            touched.push(name);
        }
    }
    touched
}

/// Rename the cluster, user and context `from` to `to`, or drop them
/// where `to` is there already, and follow along in the references.
fn rename_entries(config: &mut Value, from: &str, to: &str) {
    for list in ["clusters", "users", "contexts"] {
        let Some(entries) = config.get_mut(list).and_then(Value::as_sequence_mut) else {
            continue;
        };
        let named = |e: &Value, n: &str| e.get("name").and_then(Value::as_str) == Some(n);
        if entries.iter().any(|e| named(e, to)) {
            entries.retain(|e| !named(e, from));
        } else if let Some(e) = entries.iter_mut().find(|e| named(e, from)) {
            e["name"] = to.into();
        }
    }
    if let Some(contexts) = config.get_mut("contexts").and_then(Value::as_sequence_mut) {
        for ctx in contexts.iter_mut().filter_map(|c| c.get_mut("context")) {
            for key in ["cluster", "user"] {
                if ctx.get(key).and_then(Value::as_str) == Some(from) {
                    ctx[key] = to.into();
                }
            }
        }
    }
    if config.get("current-context").and_then(Value::as_str) == Some(from) {
        config["current-context"] = to.into();
    }
}

/// What the rest of a cluster, as `(address, settings)`, changes once the
/// node at `gone` has left: nobody lets it in any more, and whoever joined
/// through it joins through the founder - one of the rest, if `gone` was
/// the founder.
fn repoint(members: &mut [(String, Settings)], gone: &str) {
    for (_, s) in members.iter_mut() {
        s.peers.retain(|p| p != gone);
    }
    let control = |s: &Settings| s.enabled && !s.agent;
    let Some(founder) = members
        .iter()
        .position(|(_, s)| control(s) && s.cluster_init)
        .or_else(|| members.iter().position(|(_, s)| control(s)))
    else {
        return;
    };
    let gone_url = api_url(gone);
    let (addr, s) = &mut members[founder];
    if s.server == gone_url {
        s.cluster_init = true;
        s.server.clear();
    }
    let to = api_url(addr);
    for (i, (_, s)) in members.iter_mut().enumerate() {
        if i != founder && s.server == gone_url {
            s.server = to.clone();
        }
    }
}

/// Make a lone controller the founder of an HA control plane, so another
/// can join it: k3s moves its data over to etcd when it restarts.
fn found_control_plane(node: &mut Node) -> Result<()> {
    if node.settings.cluster_init || !node.settings.server.is_empty() {
        return Ok(());
    }
    let before = node.settings.clone();
    node.settings.cluster_init = true;
    if node.settings.token.is_empty() {
        node.settings.token = match read_token(&node.target)? {
            Some(t) => t,
            None => new_token()?,
        };
    }
    println!(
        "{}: moving to embedded etcd so another controller can join (restarting k3s)...",
        node.name
    );
    redeliver(node, &before)
}

/// Hand `node` its changed settings. A controller whose k3s restarts for
/// them is waited for until it serves again.
fn redeliver(node: &Node, before: &Settings) -> Result<()> {
    if node.settings == *before {
        return Ok(());
    }
    let without_peers = |s: &Settings| Settings {
        peers: Vec::new(),
        ..s.clone()
    };
    let restarts = !node.settings.agent && without_peers(&node.settings) != without_peers(before);
    let pid = if restarts {
        main_pid(&node.target)
    } else {
        String::new()
    };
    deliver(&node.target, &node.name, &node.settings)?;
    if restarts && pid != "0" && !pid.is_empty() {
        wait_restarted(node, &pid)?;
    }
    Ok(())
}

fn main_pid(target: &Target) -> String {
    target
        .output("systemctl show -p MainPID --value k3s")
        .unwrap_or_default()
}

/// Wait for k3s on `node` to run as a new process (not `pid`) and serve
/// the API.
fn wait_restarted(node: &Node, pid: &str) -> Result<()> {
    let started = Instant::now();
    loop {
        std::thread::sleep(Duration::from_secs(3));
        let now = main_pid(&node.target);
        // The daemon restarts k3s only when its config file changed; a
        // process that is still the same well after is one that didn't
        // have to.
        let settled = now != pid || started.elapsed() > Duration::from_secs(30);
        if settled && now != "0" && !now.is_empty() {
            let ready = node
                .target
                .output_as_root(&sh("k3s kubectl get --raw=/readyz 2>/dev/null || true"))
                .unwrap_or_default();
            if ready == "ok" {
                return Ok(());
            }
        }
        if started.elapsed() > READY_TIMEOUT {
            bail!(
                "k3s on {name} hasn't come back in {}s; `uc kube status {name}` shows what \
                 it's doing. Nothing else was touched after it",
                READY_TIMEOUT.as_secs(),
                name = node.name
            );
        }
    }
}

/// Wait for `new` to show up Ready in the nodes `via` lists.
fn wait_for_node(via: &Node, new: &Node) -> Result<()> {
    println!(
        "waiting for {} to join (the first start takes a few minutes)...",
        new.name
    );
    let hostname = new.target.output("hostname").ok();
    let mut names = vec![new.name.as_str()];
    names.extend(hostname.as_deref());
    let started = Instant::now();
    loop {
        let nodes = kube_nodes(&via.target).unwrap_or_default();
        if find_node(&nodes, &[&new.cluster, &new.public], &names).is_some_and(|n| n.ready) {
            return Ok(());
        }
        if started.elapsed() > JOIN_TIMEOUT {
            bail!(
                "{name} hasn't joined in {}s, so nothing was removed; `uc kube status {name}` \
                 shows what it's doing, and running the same replace again picks up from here",
                JOIN_TIMEOUT.as_secs(),
                name = new.name
            );
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}

/// The k3s release `target` runs, from `k3s --version`.
fn running_version(target: &Target) -> Result<String> {
    let out = target.output("k3s --version")?;
    out.split_whitespace()
        .skip_while(|w| *w != "version")
        .nth(1)
        .map(str::to_string)
        .ok_or_else(|| err!("unexpected `k3s --version`: {out}"))
}

/// A node as the cluster lists it.
#[derive(Debug, PartialEq)]
struct KubeNode {
    name: String,
    addresses: Vec<String>,
    ready: bool,
    control: bool,
}

/// The cluster's nodes, as the controller `via` lists them.
fn kube_nodes(via: &Target) -> Result<Vec<KubeNode>> {
    let out = kubectl(
        via,
        &[
            "get",
            "nodes",
            "-o",
            r#"jsonpath={range .items[*]}{.metadata.name}{"\t"}{.status.addresses[?(@.type=="InternalIP")].address}{"\t"}{.status.conditions[?(@.type=="Ready")].status}{"\t"}{.metadata.labels.node-role\.kubernetes\.io/control-plane}{"\n"}{end}"#,
        ],
    )?;
    Ok(parse_nodes(&out))
}

fn parse_nodes(out: &str) -> Vec<KubeNode> {
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let mut fields = line.split('\t');
            let mut next = || fields.next().unwrap_or("").trim().to_string();
            KubeNode {
                name: next(),
                addresses: next().split_whitespace().map(str::to_string).collect(),
                ready: next() == "True",
                control: next() == "true",
            }
        })
        .collect()
}

/// The node with one of `addresses`, or else one of `names`.
fn find_node<'a>(
    nodes: &'a [KubeNode],
    addresses: &[&str],
    names: &[&str],
) -> Option<&'a KubeNode> {
    nodes
        .iter()
        .find(|n| n.addresses.iter().any(|a| addresses.contains(&a.as_str())))
        .or_else(|| nodes.iter().find(|n| names.contains(&n.name.as_str())))
}

/// Run kubectl on the controller `via`, as root (k3s's kubeconfig is).
fn kubectl(via: &Target, args: &[&str]) -> Result<String> {
    let mut argv = vec!["k3s".to_string(), "kubectl".to_string()];
    argv.extend(args.iter().map(|a| a.to_string()));
    via.output_as_root(&argv)
}

fn sh(script: &str) -> Vec<String> {
    vec!["sh".into(), "-c".into(), script.into()]
}

/// The host's current settings; no file is Kubernetes off. The file is
/// root's (it holds the token), so it's read through sudo.
fn read_settings(target: &Target) -> Result<Settings> {
    let body = target
        .output_as_root(&sh(&format!(
            "cat {} 2>/dev/null || true",
            kube::SETTINGS_PATH
        )))
        .ctx("read the kube settings (root, through sudo -n)")?;
    if body.trim().is_empty() {
        return Ok(Settings::default());
    }
    Settings::parse(&body)
}

/// The join token of a k3s server that is already running, if it is.
fn read_token(target: &Target) -> Result<Option<String>> {
    let token = target.output_as_root(&sh(&format!(
        "cat {} 2>/dev/null || true",
        kube::TOKEN_PATH
    )))?;
    Ok((!token.is_empty()).then_some(token))
}

fn new_token() -> Result<String> {
    let mut raw = [0u8; 32];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut raw))
        .ctx("generate a token")?;
    Ok(raw.iter().map(|b| format!("{b:02x}")).collect())
}

/// The address `name` is reached by from here: the inventory's
/// `ansible_host`, or the host name ssh resolves it to (so aliases from
/// ~/.ssh/config work).
fn public_address(name: &str) -> Result<String> {
    if let Some(host) = inventory_host(name) {
        if host.ansible_connection == "local" {
            return Ok("127.0.0.1".into());
        }
        if !host.ansible_host.is_empty() {
            return Ok(host.ansible_host);
        }
    }
    let dest = name.rsplit('@').next().unwrap_or(name);
    let out = Command::new("ssh")
        .args(["-G", name])
        .output()
        .ctx("run ssh -G")?;
    let resolved = String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("hostname ").map(str::to_string));
    Ok(resolved.unwrap_or_else(|| dest.to_string()))
}

/// The address the other controllers reach `name` by: its mesh address
/// when it is in the mesh, else its public address as an IP.
fn cluster_address(name: &str, public: &str) -> Result<String> {
    if let Some(host) = inventory_host(name)
        && host.mesh_enabled
        && !host.address.is_empty()
    {
        return Ok(host.address);
    }
    if public.parse::<IpAddr>().is_ok() {
        return Ok(public.to_string());
    }
    (public, 0)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.next())
        .map(|a| a.ip().to_string())
        .ok_or_else(|| err!("can't resolve {public} ({name}) to an address"))
}

/// An IPv6 address in brackets, as a URL wants it.
fn bracket(addr: &str) -> String {
    match addr.parse::<IpAddr>() {
        Ok(IpAddr::V6(_)) => format!("[{addr}]"),
        _ => addr.to_string(),
    }
}

/// Hand the settings to the host's daemon through `usecoded join`, the
/// same way `uc net firewall` does, so the daemon writes the file and
/// reloads with an answer.
fn deliver(target: &Target, name: &str, settings: &Settings) -> Result<()> {
    let bundle = Bundle {
        host: name.to_string(),
        kube: Some(settings.clone()),
        ..Bundle::default()
    };

    let local = tempfile::Builder::new()
        .prefix("uc-kube-")
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

#[cfg(test)]
mod tests {
    use super::*;

    const K3S_YAML: &str = "apiVersion: v1
clusters:
- cluster:
    certificate-authority-data: Q0E=
    server: https://127.0.0.1:6443
  name: default
contexts:
- context:
    cluster: default
    user: default
  name: default
current-context: default
kind: Config
preferences: {}
users:
- name: default
  user:
    client-certificate-data: Q0VSVA==
    client-key-data: S0VZ
";

    #[test]
    fn the_hosts_kubeconfig_is_renamed_and_pointed_at_the_host() {
        let mut config: Value = serde_yaml::from_str(K3S_YAML).unwrap();
        rename_and_point(&mut config, "edge", "https://203.0.113.7:6443").unwrap();
        assert_eq!(config["clusters"][0]["name"], "edge");
        assert_eq!(
            config["clusters"][0]["cluster"]["server"],
            "https://203.0.113.7:6443"
        );
        assert_eq!(config["contexts"][0]["context"]["cluster"], "edge");
        assert_eq!(config["contexts"][0]["context"]["user"], "edge");
        assert_eq!(config["users"][0]["name"], "edge");
    }

    #[test]
    fn merging_replaces_the_same_name_and_keeps_the_rest() {
        let mut incoming: Value = serde_yaml::from_str(K3S_YAML).unwrap();
        rename_and_point(&mut incoming, "edge", "https://a:6443").unwrap();

        let mut local: Value = serde_yaml::from_str(
            "apiVersion: v1\nkind: Config\ncurrent-context: work\n\
             clusters:\n- name: work\n  cluster: {server: https://w}\n\
             - name: edge\n  cluster: {server: https://old}\n\
             contexts:\n- name: work\n  context: {cluster: work, user: work}\n\
             users:\n- name: work\n  user: {token: t}\n",
        )
        .unwrap();
        merge(&mut local, &incoming, "edge").unwrap();

        let clusters = local["clusters"].as_sequence().unwrap();
        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[1]["cluster"]["server"], "https://a:6443");
        assert_eq!(local["contexts"].as_sequence().unwrap().len(), 2);
        assert_eq!(local["current-context"], "edge");
    }

    #[test]
    fn merging_into_nothing_makes_a_kubeconfig() {
        let incoming: Value = serde_yaml::from_str(K3S_YAML).unwrap();
        let mut local = Value::Null;
        merge(&mut local, &incoming, "default").unwrap();
        assert_eq!(local["kind"], "Config");
        assert_eq!(local["clusters"].as_sequence().unwrap().len(), 1);
    }

    #[test]
    fn options_and_hosts_are_told_apart() {
        let args: Vec<String> = ["a", "--version", "v1.31.4+k3s1", "b", "--no-connect"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (words, flags) = parse(&args, &["--version"], &["--no-connect"]).unwrap();
        assert_eq!(words, vec!["a", "b"]);
        assert_eq!(flag(&flags, "--version"), Some("v1.31.4+k3s1"));
        assert!(flag(&flags, "--no-connect").is_some());
        assert!(parse(&args, &[], &[]).is_err());
    }

    #[test]
    fn peers_are_added_once_without_the_node_itself() {
        let mut s = Settings {
            peers: vec!["10.10.0.5".into()],
            ..Settings::default()
        };
        let addrs: Vec<String> = ["10.10.0.2", "10.10.0.5", "10.10.0.9", "edge.example.com"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        add_peers(&mut s, &addrs, "10.10.0.9");
        assert_eq!(s.peers, vec!["10.10.0.5", "10.10.0.2"]);
    }

    fn controller(server: &str, cluster_init: bool, peers: &[&str]) -> Settings {
        Settings {
            enabled: true,
            cluster_init,
            server: server.into(),
            token: "t".into(),
            peers: peers.iter().map(|p| p.to_string()).collect(),
            ..Settings::default()
        }
    }

    #[test]
    fn losing_the_founder_makes_the_next_controller_lead() {
        let gone = "https://10.10.0.2:6443";
        let mut members = vec![
            (
                "10.10.0.3".to_string(),
                controller(gone, false, &["10.10.0.2", "10.10.0.4"]),
            ),
            (
                "10.10.0.4".to_string(),
                controller(gone, false, &["10.10.0.2", "10.10.0.3"]),
            ),
            (
                "10.10.0.9".to_string(),
                Settings {
                    agent: true,
                    ..controller(gone, false, &["10.10.0.2"])
                },
            ),
        ];
        repoint(&mut members, "10.10.0.2");
        let [(_, a), (_, b), (_, w)] = &members[..] else {
            unreachable!()
        };
        assert!(a.cluster_init && a.server.is_empty(), "{a:?}");
        assert_eq!(a.peers, vec!["10.10.0.4"]);
        assert_eq!(b.server, "https://10.10.0.3:6443");
        assert!(!b.cluster_init);
        assert_eq!(w.server, "https://10.10.0.3:6443");
        assert!(w.peers.is_empty());
    }

    #[test]
    fn losing_another_controller_only_drops_its_address() {
        let founder = "https://10.10.0.2:6443";
        let mut members = vec![
            (
                "10.10.0.2".to_string(),
                controller("", true, &["10.10.0.3", "10.10.0.4"]),
            ),
            (
                "10.10.0.4".to_string(),
                controller(founder, false, &["10.10.0.2", "10.10.0.3"]),
            ),
        ];
        let before = members.clone();
        repoint(&mut members, "10.10.0.3");
        assert_eq!(members[0].1.peers, vec!["10.10.0.4"]);
        assert_eq!(members[1].1.peers, vec!["10.10.0.2"]);
        assert_eq!(members[0].1.cluster_init, before[0].1.cluster_init);
        assert_eq!(members[1].1.server, founder);
    }

    #[test]
    fn nodes_are_read_from_kubectl_and_found_by_address_or_name() {
        let out = "edge\t10.10.0.2\tTrue\ttrue\ncave\t10.10.0.3 fd00::3\tFalse\ttrue\nbox-1\t10.10.0.9\tTrue";
        let nodes = parse_nodes(out);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[1].addresses, vec!["10.10.0.3", "fd00::3"]);
        assert!(!nodes[1].ready && nodes[1].control);
        assert!(nodes[2].ready && !nodes[2].control);
        assert_eq!(find_node(&nodes, &["fd00::3"], &[]).unwrap().name, "cave");
        assert_eq!(
            find_node(&nodes, &["1.2.3.4"], &["box-1"]).unwrap().name,
            "box-1"
        );
        assert!(find_node(&nodes, &["1.2.3.4"], &["attic"]).is_none());
    }

    #[test]
    fn the_address_is_taken_out_of_an_api_url() {
        assert_eq!(
            server_address("https://10.10.0.2:6443").as_deref(),
            Some("10.10.0.2")
        );
        assert_eq!(
            server_address("https://[2001:db8::1]:6443").as_deref(),
            Some("2001:db8::1")
        );
        assert_eq!(server_address(""), None);
        assert_eq!(api_url("2001:db8::1"), "https://[2001:db8::1]:6443");
    }

    #[test]
    fn the_etcd_member_is_found_by_peer_address_or_node_name() {
        let list: Value = serde_yaml::from_str(
            r#"{"header":{},"members":[
                {"ID":"11","name":"edge-1a2b3c4d","peerURLs":["https://10.10.0.2:2380"]},
                {"ID":"12","name":"cave-5e6f7a8b","peerURLs":["https://10.10.0.3:2380"]}]}"#,
        )
        .unwrap();
        assert_eq!(
            etcd_member(&list, &["10.10.0.3"], &[]).as_deref(),
            Some("12")
        );
        assert_eq!(
            etcd_member(&list, &["1.2.3.4"], &["EDGE"]).as_deref(),
            Some("11")
        );
        assert_eq!(etcd_member(&list, &["1.2.3.4"], &["attic"]), None);
    }

    #[test]
    fn kubectl_follows_the_cluster_to_the_successor() {
        let mut config: Value = serde_yaml::from_str(
            "clusters:\n\
             - {name: cave, cluster: {server: 'https://203.0.113.3:6443'}}\n\
             - {name: prod, cluster: {server: 'https://203.0.113.3:6443'}}\n\
             - {name: work, cluster: {server: 'https://w:6443'}}\n\
             users:\n- {name: cave, user: {}}\n- {name: work, user: {}}\n\
             contexts:\n- {name: cave, context: {cluster: cave, user: cave}}\n\
             - {name: work, context: {cluster: work, user: work}}\n\
             current-context: cave\n",
        )
        .unwrap();
        let touched = point_clusters(&mut config, &["203.0.113.3", "cave"], "https://den:6443");
        assert_eq!(touched, vec!["cave", "prod"]);
        rename_entries(&mut config, "cave", "den");
        assert_eq!(config["clusters"][0]["name"], "den");
        assert_eq!(
            config["clusters"][0]["cluster"]["server"],
            "https://den:6443"
        );
        assert_eq!(
            config["clusters"][1]["cluster"]["server"],
            "https://den:6443"
        );
        assert_eq!(config["clusters"][2]["cluster"]["server"], "https://w:6443");
        assert_eq!(config["users"][0]["name"], "den");
        assert_eq!(config["contexts"][0]["context"]["cluster"], "den");
        assert_eq!(config["current-context"], "den");

        // Where the successor has entries already, the old ones go.
        rename_entries(&mut config, "work", "den");
        assert_eq!(config["contexts"].as_sequence().unwrap().len(), 1);
    }

    #[test]
    fn ipv6_is_bracketed_in_urls() {
        assert_eq!(bracket("2001:db8::1"), "[2001:db8::1]");
        assert_eq!(bracket("10.0.0.1"), "10.0.0.1");
        assert_eq!(bracket("edge.example.com"), "edge.example.com");
    }
}
