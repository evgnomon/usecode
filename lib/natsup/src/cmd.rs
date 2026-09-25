// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Subcommand implementations.

use std::io::{BufRead, Write};
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::health;
use crate::node::{self, Spec};
use crate::podman;

const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(2);
const MAX_NODE_ID: i64 = 9;

/// Resolved global options.
pub struct Globals {
    pub network: String,
    pub cluster_name: String,
    pub image: String,
    pub data_dir: String,
    pub nodes: i64,
    pub yes: bool,
    pub dry_run: bool,
    pub no_pull: bool,
}

impl Globals {
    fn spec(&self) -> Spec<'_> {
        Spec {
            node_count: self.nodes,
            network: &self.network,
            cluster_name: &self.cluster_name,
            image: &self.image,
            data_dir: &self.data_dir,
        }
    }
}

/// Options of the `node` subcommand.
pub struct NodeOpts {
    pub node_id: i64,
    pub client_port: i64,
    pub monitoring_port: i64,
}

fn flush() {
    let _ = std::io::stdout().flush();
}

fn prompt_yes_no(message: &str, default: bool) -> bool {
    let suffix = if default { " [Y/n]: " } else { " [y/N]: " };
    print!("{message}{suffix}");
    flush();
    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => {
            println!();
            false
        }
        Ok(_) => {
            let answer = line.trim().to_lowercase();
            if answer.is_empty() {
                default
            } else {
                answer == "y" || answer == "yes"
            }
        }
    }
}

fn ensure_network(name: &str) -> bool {
    if podman::network_exists(name) {
        return true;
    }
    println!("Network '{name}' does not exist. Creating...");
    if podman::create_network(name) {
        println!("Network '{name}' created successfully.");
        true
    } else {
        eprintln!("Error: Failed to create network '{name}'");
        false
    }
}

fn remove_volume(path: &str) -> bool {
    if !Path::new(path).exists() {
        return true;
    }
    match std::fs::remove_dir_all(path) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("Error removing volume '{path}': {e}");
            false
        }
    }
}

/// Existing nats1..nats9 containers as (node id, state).
fn existing_nodes() -> Vec<(i64, String)> {
    (1..=MAX_NODE_ID)
        .filter_map(|i| podman::container_state(&node::nats_name(i)).map(|s| (i, s)))
        .collect()
}

fn remove_nodes(existing: &[(i64, String)], data_dir: &str) {
    for (node_id, _) in existing {
        let name = node::nats_name(*node_id);
        print!("  Removing {name}... ");
        flush();
        if podman::remove_container(&name) {
            print!("container ✓ ");
        } else {
            print!("container ✗ ");
        }
        if remove_volume(&node::volume_path(data_dir, *node_id)) {
            println!("volume ✓");
        } else {
            println!("volume ✗");
        }
    }
}

fn pull(image: &str) -> Result<(), i32> {
    if podman::pull(image) {
        Ok(())
    } else {
        eprintln!("Error: podman pull {image} failed");
        Err(1)
    }
}

fn wait_for_cluster_health(node_count: i64) -> bool {
    println!(
        "\nWaiting for cluster health (timeout: {}s)...",
        HEALTH_CHECK_TIMEOUT.as_secs()
    );
    let start = Instant::now();
    while start.elapsed() < HEALTH_CHECK_TIMEOUT {
        let mut all_healthy = true;
        let mut all_connected = true;
        for node_id in 1..=node_count {
            let port = node::monitoring_port(node_id, 0);
            if !health::node_healthy(port) {
                all_healthy = false;
                break;
            }
            let info = health::route_info(port);
            let routes = health::num_routes(info.as_ref());
            if info.is_some() && routes.as_f64().unwrap_or(0.0) < (node_count - 1) as f64 {
                all_connected = false;
            }
        }
        if all_healthy && all_connected {
            println!("✓ All nodes healthy and connected!");
            return true;
        }
        sleep(HEALTH_CHECK_INTERVAL);
        print!(".");
        flush();
    }
    println!("\n✗ Timeout waiting for cluster health");
    false
}

fn print_cluster_status(node_count: i64) {
    println!("\nCluster Status:");
    println!("{}", "-".repeat(60));
    for node_id in 1..=node_count {
        let name = node::nats_name(node_id);
        let port = node::monitoring_port(node_id, 0);
        let Some(state) = podman::container_state(&name) else {
            println!("  {name}: not found");
            continue;
        };
        let healthy = health::node_healthy(port);
        let routes = health::num_routes(health::route_info(port).as_ref());
        let icon = if healthy { "✓" } else { "✗" };
        println!(
            "  {name}: {state:<10} health: {icon}  routes: {routes}/{}",
            node_count - 1
        );
    }
    println!("{}", "-".repeat(60));
}

pub fn cluster(g: &Globals) -> i32 {
    let node_count = g.nodes;
    println!("Creating {node_count}-node NATS cluster");
    println!("  Network      : {}", g.network);
    println!("  Cluster name : {}", g.cluster_name);
    println!("  Data dir     : {}", g.data_dir);
    println!();

    let existing = existing_nodes();
    if !existing.is_empty() {
        println!("Found existing NATS containers:");
        for (node_id, state) in &existing {
            println!("  {}: {state}", node::nats_name(*node_id));
        }
        if g.dry_run {
            println!("\nDRY RUN — would remove existing containers and their volumes");
        } else if g.yes || prompt_yes_no("\nRemove existing containers and their volumes?", true) {
            remove_nodes(&existing, &g.data_dir);
        } else {
            println!("Aborted.");
            return 1;
        }
    }

    if g.dry_run {
        println!(
            "\nDRY RUN — would ensure network {} and pull {}",
            g.network, g.image
        );
    } else if !ensure_network(&g.network) {
        return 1;
    }

    if !g.no_pull && !g.dry_run {
        println!("\nPulling image {}...", g.image);
        if let Err(code) = pull(&g.image) {
            return code;
        }
    }

    println!("\nCreating {node_count} nodes...");
    let spec = g.spec();
    for node_id in 1..=node_count {
        if !node::create(&spec, node_id, 0, 0, g.dry_run, true) {
            println!("\nFailed to create node {node_id}. Cluster may be incomplete.");
            return 1;
        }
    }

    if g.dry_run {
        println!("\nDry run complete.");
        return 0;
    }

    if !wait_for_cluster_health(node_count) {
        println!("\nCluster started but may not be fully healthy.");
        print_cluster_status(node_count);
        return 1;
    }

    print_cluster_status(node_count);

    println!("\nQuick commands:");
    println!("  Logs (node 1) : podman logs -f nats1");
    println!("  Health        : curl http://localhost:8222/healthz");
    println!("  Routes        : curl http://localhost:8222/routez");
    println!("  Stop all      : podman stop nats{{1..{node_count}}}");
    println!("  Remove all    : podman rm -f nats{{1..{node_count}}}");
    0
}

pub fn node(g: &Globals, o: &NodeOpts) -> i32 {
    let node_id = o.node_id;
    if !(1..=MAX_NODE_ID).contains(&node_id) {
        eprintln!("Error: --node-id must be between 1 and 9");
        return 1;
    }
    let name = node::nats_name(node_id);

    if !g.dry_run && !ensure_network(&g.network) {
        return 1;
    }

    if let Some(state) = podman::container_state(&name) {
        println!("Container '{name}' already exists (state: {state})");
        let monitoring = node::monitoring_port(node_id, o.monitoring_port);

        if state == "running" {
            println!("Container is already running.");
            println!("\nQuick checks:");
            println!("  Logs   : podman logs -f {name}");
            println!("  Health : curl http://localhost:{monitoring}/healthz");
            println!("  Stop   : podman stop {name}");
            return 0;
        }

        if g.dry_run {
            println!("\nDRY RUN — would remove or start existing container '{name}'");
            return 0;
        }

        let prompt = format!("Remove existing container '{name}' and its volume?");
        if g.yes || prompt_yes_no(&prompt, true) {
            let volume = node::volume_path(&g.data_dir, node_id);
            print!("Removing container '{name}'... ");
            flush();
            if !podman::remove_container(&name) {
                println!("✗");
                return 1;
            }
            println!("✓");

            print!("Removing volume '{volume}'... ");
            flush();
            if remove_volume(&volume) {
                println!("✓");
            } else {
                println!("✗ (continuing anyway)");
            }
        } else {
            println!("Starting existing container '{name}'...");
            if podman::start_container(&name) {
                println!("✓ Started successfully.");
                println!("\nQuick checks:");
                println!("  Logs   : podman logs -f {name}");
                println!("  Health : curl http://localhost:{monitoring}/healthz");
                return 0;
            }
            println!("✗ Failed to start");
            return 1;
        }
    }

    if !g.no_pull && !g.dry_run {
        println!("Pulling image {}...", g.image);
        if let Err(code) = pull(&g.image) {
            return code;
        }
    }

    let client = node::client_port(node_id, o.client_port);
    let monitoring = node::monitoring_port(node_id, o.monitoring_port);

    println!("\nStarting NATS node {name}");
    println!("  Network         : {}", g.network);
    println!("  Client port     : {client} → 4222");
    println!("  Monitoring port : {monitoring} → 8222");
    println!(
        "  Data directory  : {}",
        node::volume_path(&g.data_dir, node_id)
    );
    println!("  Routes          : {}", node::routes(g.nodes));

    if g.dry_run {
        println!("\nDRY RUN — no container created");
        return 0;
    }

    let spec = g.spec();
    if !node::create(
        &spec,
        node_id,
        o.client_port,
        o.monitoring_port,
        g.dry_run,
        false,
    ) {
        return 1;
    }

    println!("✓ Container started");

    print!("\nWaiting for node health... ");
    flush();
    let mut healthy = false;
    for _ in 0..15 {
        if health::node_healthy(monitoring) {
            println!("✓");
            healthy = true;
            break;
        }
        sleep(Duration::from_secs(1));
        print!(".");
        flush();
    }
    if !healthy {
        println!("✗ (timeout)");
    }

    println!("\nQuick checks:");
    println!("  Logs        : podman logs -f {name}");
    println!("  Health      : curl http://localhost:{monitoring}/healthz");
    println!("  Routes      : curl http://localhost:{monitoring}/routez");
    println!("  Stop        : podman stop {name}");
    println!("  Remove      : podman rm -f {name}");
    0
}

pub fn status(g: &Globals) -> i32 {
    print_cluster_status(g.nodes);
    0
}

pub fn remove(g: &Globals) -> i32 {
    let existing = existing_nodes();
    if existing.is_empty() {
        println!("No NATS containers found.");
        return 0;
    }

    println!("Found NATS containers:");
    for (node_id, state) in &existing {
        let volume = node::volume_path(&g.data_dir, *node_id);
        let exists = if Path::new(&volume).exists() {
            "yes"
        } else {
            "no"
        };
        println!("  {}: {state}, volume: {exists}", node::nats_name(*node_id));
    }

    if !(g.yes || prompt_yes_no("\nRemove all containers and volumes?", true)) {
        println!("Aborted.");
        return 1;
    }

    remove_nodes(&existing, &g.data_dir);
    println!("\nDone.");
    0
}
