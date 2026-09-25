// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Naming, port and path conventions plus single-node creation.

use std::io::Write;

use crate::podman;

pub fn nats_name(node_id: i64) -> String {
    format!("nats{node_id}")
}

/// `--routes` value assuming nodes are named nats1, nats2, ...
pub fn routes(node_count: i64) -> String {
    (1..=node_count)
        .map(|i| format!("nats://nats{i}:6222"))
        .collect::<Vec<_>>()
        .join(",")
}

pub fn client_port(node_id: i64, over: i64) -> i64 {
    if over != 0 { over } else { 4222 + node_id - 1 }
}

pub fn monitoring_port(node_id: i64, over: i64) -> i64 {
    if over != 0 { over } else { 8222 + node_id - 1 }
}

pub fn volume_path(data_dir: &str, node_id: i64) -> String {
    format!("{data_dir}/js-{node_id}")
}

/// Settings shared by every node.
pub struct Spec<'a> {
    pub node_count: i64,
    pub network: &'a str,
    pub cluster_name: &'a str,
    pub image: &'a str,
    pub data_dir: &'a str,
}

/// Full `podman run` argument list for one node.
pub fn run_args(spec: &Spec, node_id: i64, client: i64, monitoring: i64) -> Vec<String> {
    let name = nats_name(node_id);
    let volume = format!("{}:/data/js", volume_path(spec.data_dir, node_id));
    [
        "podman",
        "run",
        "-d",
        "--name",
        &name,
        "--hostname",
        &name,
        "--network",
        spec.network,
        "--restart",
        "unless-stopped",
        "-p",
        &format!("{client}:4222"),
        "-p",
        &format!("{monitoring}:8222"),
        "-v",
        &volume,
        spec.image,
        // NATS server flags
        "--name",
        &name,
        "--cluster_name",
        spec.cluster_name,
        "--jetstream",
        "--store_dir",
        "/data/js",
        "--cluster",
        "nats://0.0.0.0:6222",
        "--routes",
        &routes(spec.node_count),
        "--http_port",
        "8222",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Create and start a single NATS node. Returns true on success.
pub fn create(
    spec: &Spec,
    node_id: i64,
    client_override: i64,
    monitoring_override: i64,
    dry_run: bool,
    verbose: bool,
) -> bool {
    let name = nats_name(node_id);
    let client = client_port(node_id, client_override);
    let monitoring = monitoring_port(node_id, monitoring_override);
    let args = run_args(spec, node_id, client, monitoring);

    if verbose {
        println!("  Starting {name}...");
        println!("    Client: {client}, Monitor: {monitoring}");
    }

    if dry_run {
        println!("    DRY RUN — command:");
        println!("    {} ...", args[..15].join(" "));
        return true;
    }

    let volume = volume_path(spec.data_dir, node_id);
    if let Err(e) = std::fs::create_dir_all(&volume) {
        eprintln!("    ✗ Failed to create {volume}: {e}");
        return false;
    }

    let _ = std::io::stdout().flush();
    let argv: Vec<&str> = args[1..].iter().map(String::as_str).collect();
    let out = podman::capture(&argv);
    if !out.status.success() {
        eprintln!("    ✗ Failed to start {name}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stderr = stderr.trim();
        if !stderr.is_empty() {
            eprintln!("    stderr: {stderr}");
        }
        return false;
    }

    if verbose {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let id: String = stdout.trim().chars().take(12).collect();
        println!("    ✓ Started ({id})");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conventions() {
        assert_eq!(nats_name(3), "nats3");
        assert_eq!(
            routes(3),
            "nats://nats1:6222,nats://nats2:6222,nats://nats3:6222"
        );
        assert_eq!(routes(0), "");
        assert_eq!(client_port(2, 0), 4223);
        assert_eq!(client_port(2, 5000), 5000);
        assert_eq!(monitoring_port(3, 0), 8224);
        assert_eq!(volume_path("/d", 1), "/d/js-1");
    }

    #[test]
    fn podman_run_args() {
        let spec = Spec {
            node_count: 2,
            network: "net",
            cluster_name: "c1",
            image: "nats:2-alpine",
            data_dir: "/data",
        };
        let args = run_args(&spec, 1, 4222, 8222);
        assert_eq!(
            args[..15].join(" "),
            "podman run -d --name nats1 --hostname nats1 --network net --restart unless-stopped -p 4222:4222 -p 8222:8222"
        );
        assert_eq!(
            args[15..].join(" "),
            "-v /data/js-1:/data/js nats:2-alpine --name nats1 --cluster_name c1 --jetstream \
             --store_dir /data/js --cluster nats://0.0.0.0:6222 \
             --routes nats://nats1:6222,nats://nats2:6222 --http_port 8222"
        );
    }
}
