// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use std::fs;
use std::process::{Command, Output, exit};

use clap::Args;

use crate::config::CFG;

#[derive(Args)]
pub struct Opts {
    /// Container name
    #[arg(short, long, default_value = CFG.container.as_str())]
    pub name: String,
    /// Host port
    #[arg(short, long, default_value = CFG.port.as_str())]
    pub port: i64,
    /// MongoDB image
    #[arg(long, default_value = CFG.image.as_str())]
    pub image: String,
    /// Data directory
    #[arg(long, default_value = CFG.data_dir.as_str())]
    pub data_dir: String,
    /// Podman network
    #[arg(long, default_value = CFG.network.as_str())]
    pub network: String,
    /// Pull image before starting (default)
    #[arg(long, overrides_with = "no_pull")]
    pub pull: bool,
    /// Do not pull the image
    #[arg(long, overrides_with = "pull")]
    pub no_pull: bool,
    /// Remove existing container and start fresh
    #[arg(long)]
    pub force: bool,
}

fn podman(args: &[&str]) -> Output {
    Command::new("podman")
        .args(args)
        .output()
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to run podman: {e}");
            exit(1)
        })
}

fn ok(args: &[&str]) -> bool {
    podman(args).status.success()
}

fn container_state(name: &str) -> Option<String> {
    let filter = format!("name=^{name}$");
    let out = podman(&["ps", "-a", "--filter", &filter, "--format", "{{.State}}"]);
    let state = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!state.is_empty()).then_some(state)
}

fn ensure_network(name: &str) -> bool {
    if ok(&["network", "exists", name]) {
        return true;
    }
    eprintln!("Network '{name}' does not exist. Creating...");
    if ok(&["network", "create", name]) {
        eprintln!("Network '{name}' created successfully.");
        true
    } else {
        eprintln!("Error: Failed to create network '{name}'.");
        false
    }
}

pub fn run(o: &Opts) {
    let name = o.name.as_str();
    let state = container_state(name);

    if let Some(state) = &state
        && !o.force
    {
        if state == "running" {
            eprintln!("Container '{name}' is already running.");
            return;
        }
        eprintln!("Container '{name}' exists (state: {state}). Starting...");
        if ok(&["start", name]) {
            eprintln!("Container '{name}' started.");
        } else {
            eprintln!("Error: Failed to start container '{name}'.");
            exit(1);
        }
        return;
    }

    if state.is_some() {
        eprintln!("Removing existing container '{name}'...");
        ok(&["rm", "-f", name]);
    }

    if !ensure_network(&o.network) {
        exit(1);
    }

    if !o.no_pull {
        eprintln!("Pulling image {}...", o.image);
        ok(&["pull", &o.image]);
    }

    if let Err(e) = fs::create_dir_all(&o.data_dir) {
        eprintln!("Error: cannot create {}: {e}", o.data_dir);
        exit(1);
    }

    let port_map = format!("{}:27017", o.port);
    let volume = format!("{}:/data/db", o.data_dir);
    let user_env = format!("MONGO_INITDB_ROOT_USERNAME={}", CFG.user);
    let pass_env = format!("MONGO_INITDB_ROOT_PASSWORD={}", CFG.password);
    let args = [
        "run",
        "-d",
        "--name",
        name,
        "--hostname",
        name,
        "--network",
        &o.network,
        "--restart",
        "unless-stopped",
        "-p",
        &port_map,
        "-v",
        &volume,
        "-e",
        &user_env,
        "-e",
        &pass_env,
        &o.image,
    ];

    eprintln!("Starting MongoDB container '{name}' on port {}...", o.port);
    let out = podman(&args);
    if !out.status.success() {
        eprintln!(
            "Error starting container: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        exit(1);
    }

    eprintln!("MongoDB container '{name}' started successfully.");
    eprintln!("\nQuick commands:");
    eprintln!("  Logs   : podman logs -f {name}");
    eprintln!("  Stop   : podman stop {name}");
    eprintln!("  Remove : podman rm -f {name}");
    eprintln!(
        "  Connect: mongosh mongodb://{}:{}@localhost:{}",
        CFG.user, CFG.password, o.port
    );
}
