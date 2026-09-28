// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! uc-nats - run NATS server nodes (with JetStream) using Podman.

mod cmd;
mod health;
mod node;
mod podman;

use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

const DEFAULT_NETWORK: &str = "nats-cluster";
const DEFAULT_CLUSTER_NAME: &str = "c1";
const DEFAULT_IMAGE: &str = "nats:2-alpine";
const DEFAULT_NODE_COUNT: i64 = 3;

const EXAMPLES: &str = "Examples:
  uc nats cluster            Create a 3-node cluster
  uc nats cluster --nodes 5  Create a 5-node cluster
  uc nats node --node-id 1   Create/start a single node
  uc nats status             Show cluster status
  uc nats remove             Remove all nodes and volumes

Environment:
  NATS_NETWORK, CLUSTER_NAME, NATS_IMAGE, DATA_DIR, NATS_NODE_COUNT,
  NATS_NODE_ID, CLIENT_PORT, MONITORING_PORT provide option defaults.";

/// Run NATS JetStream nodes with Podman
#[derive(Parser)]
#[command(name = "uc-nats", after_help = EXAMPLES)]
struct Cli {
    /// Podman network name (default: nats-cluster)
    #[arg(long, global = true)]
    network: Option<String>,
    /// NATS cluster name (default: c1)
    #[arg(long, global = true)]
    cluster_name: Option<String>,
    /// Container image (default: nats:2-alpine)
    #[arg(long, global = true)]
    image: Option<String>,
    /// Host directory for JetStream storage
    #[arg(long, global = true)]
    data_dir: Option<String>,
    /// Number of nodes in cluster (default: 3)
    #[arg(long, global = true, allow_negative_numbers = true)]
    nodes: Option<i64>,
    /// Automatically answer yes to prompts
    #[arg(short, long, global = true)]
    yes: bool,
    /// Print commands instead of running them
    #[arg(long, global = true)]
    dry_run: bool,
    /// Do not pull image before starting
    #[arg(long, global = true)]
    no_pull: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Create a NATS cluster
    Cluster,
    /// Create/start a single node
    Node(NodeArgs),
    /// Show cluster status
    Status,
    /// Remove all nodes and volumes
    Remove,
}

#[derive(Args)]
struct NodeArgs {
    /// Node number 1–9 (default: 1)
    #[arg(long, allow_negative_numbers = true)]
    node_id: Option<i64>,
    /// Host port for NATS client connections
    #[arg(long, allow_negative_numbers = true)]
    client_port: Option<i64>,
    /// Host port for monitoring/health
    #[arg(long, allow_negative_numbers = true)]
    monitoring_port: Option<i64>,
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn int_opt(value: Option<i64>, env: &str, default: i64) -> Result<i64, String> {
    if let Some(v) = value {
        return Ok(v);
    }
    match std::env::var(env) {
        Ok(s) => s
            .trim()
            .parse()
            .map_err(|_| format!("invalid integer in {env}: {s:?}")),
        Err(_) => Ok(default),
    }
}

fn run(cli: Cli) -> Result<i32, String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "None".to_string());
    let globals = cmd::Globals {
        network: cli
            .network
            .unwrap_or_else(|| env_or("NATS_NETWORK", DEFAULT_NETWORK)),
        cluster_name: cli
            .cluster_name
            .unwrap_or_else(|| env_or("CLUSTER_NAME", DEFAULT_CLUSTER_NAME)),
        image: cli
            .image
            .unwrap_or_else(|| env_or("NATS_IMAGE", DEFAULT_IMAGE)),
        data_dir: cli
            .data_dir
            .unwrap_or_else(|| env_or("DATA_DIR", &format!("{home}/data/nats"))),
        nodes: int_opt(cli.nodes, "NATS_NODE_COUNT", DEFAULT_NODE_COUNT)?,
        yes: cli.yes,
        dry_run: cli.dry_run,
        no_pull: cli.no_pull,
    };

    Ok(match cli.command.unwrap_or(Command::Cluster) {
        Command::Cluster => cmd::cluster(&globals),
        Command::Node(a) => {
            let opts = cmd::NodeOpts {
                node_id: int_opt(a.node_id, "NATS_NODE_ID", 1)?,
                client_port: int_opt(a.client_port, "CLIENT_PORT", 0)?,
                monitoring_port: int_opt(a.monitoring_port, "MONITORING_PORT", 0)?,
            };
            cmd::node(&globals, &opts)
        }
        Command::Status => cmd::status(&globals),
        Command::Remove => cmd::remove(&globals),
    })
}

fn main() -> ExitCode {
    // One line for `uc help`, which lists this tool as `uc nats`.
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("run NATS JetStream nodes and clusters with Podman");
        return ExitCode::SUCCESS;
    }
    match run(Cli::parse()) {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}
