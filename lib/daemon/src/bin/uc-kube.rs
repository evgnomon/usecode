// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc kube`: Kubernetes (k3s) controllers on your hosts, from wherever
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
use uc_daemon::kube::{self, Settings};
use uc_daemon::remote::{Target, inventory_host};
use uc_daemon::setup::BINARY_PATH;
use uc_daemon::{bail, err};

const SUMMARY: &str = "Kubernetes (k3s) controllers on your hosts, and kubectl for them";

const USAGE: &str = "usage: uc kube COMMAND [ARGS...]

Kubernetes on your hosts, run by the usecode daemon there as a k3s
controller. You say what you want; the daemon downloads k3s, starts it,
and keeps it running.

  enable HOST [HOST...]       make HOST a Kubernetes controller and add the
                              cluster to your kubeconfig. One host is a
                              cluster of its own; three or more are one HA
                              control plane (embedded etcd), founded by the
                              first. List the whole control plane every time
                              you grow it: uc kube enable a b c
      --version VERSION       run this k3s release (e.g. v1.31.4+k3s1)
                              instead of the current stable one
      --no-connect            don't touch your kubeconfig
  connect HOST                add HOST's cluster to your kubeconfig and
                              switch kubectl to it
      --name NAME             the context's name (default: HOST)
      --server ADDRESS        the address kubectl uses (default: the one
                              ssh reaches HOST by)
  status HOST                 the settings, k3s, and the nodes
  disable HOST                stop k3s on HOST (its data is kept)
      --purge                 uninstall k3s and delete its data

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
        other => bail!("{USAGE}\n\nunknown command {other:?}"),
    }
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

fn enable(args: &[String]) -> Result<()> {
    let usage = "usage: uc kube enable HOST [HOST...] [--version VERSION] [--no-connect]";
    let (names, flags) = parse(args, &["--version"], &["--no-connect"])?;
    if names.is_empty() {
        bail!("{usage}");
    }
    if names.len() == 2 {
        eprintln!(
            "note: an HA control plane needs three controllers to survive losing one; \
             two is less available than one. Add a third when you can."
        );
    }

    let mut nodes = Vec::new();
    for name in &names {
        let target = Target::reach(name)?;
        let settings = read_settings(&target)?;
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
        if let Some(v) = flag(&flags, "--version") {
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
            s.peers = cluster_addrs
                .iter()
                .filter(|a| **a != node.cluster)
                .filter(|a| a.parse::<IpAddr>().is_ok())
                .cloned()
                .collect();
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

    if flag(&flags, "--no-connect").is_some() {
        println!("\nWhen it's up: uc kube connect {}", nodes[0].name);
        return Ok(());
    }
    let first = &nodes[0];
    connect(&first.target, &first.name, &first.name, &first.public)
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
    if !settings.enabled {
        bail!("{name} is not a Kubernetes controller; `uc kube enable {name}` makes it one");
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
    if s.cluster_init {
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

    let k3s = target
        .output_as_root(&sh("systemctl is-active k3s 2>/dev/null || true"))
        .unwrap_or_else(|e| e.to_string());
    println!(
        "\nk3s: {}",
        if k3s.is_empty() {
            "not installed"
        } else {
            &k3s
        }
    );
    if k3s == "active" {
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
    fn ipv6_is_bracketed_in_urls() {
        assert_eq!(bracket("2001:db8::1"), "[2001:db8::1]");
        assert_eq!(bracket("10.0.0.1"), "10.0.0.1");
        assert_eq!(bracket("edge.example.com"), "edge.example.com");
    }
}
