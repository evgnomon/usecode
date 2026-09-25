// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! plat: per-host service deployment driver.
//!
//! Manages env files under $PLAT_HOME and dispatches actions to per-blueprint
//! scripts under $ROOT/lib/templates/lib/<service>/scripts/plat.
//!
//! The env file is the source of truth: template defaults are written on first
//! use and backfilled if missing, so per-invocation args stay minimal.

mod actions;
mod dotenv;
mod env;
mod paths;

use std::process::exit;

use clap::{Args, Parser, Subcommand};

use crate::env::Pod;
use crate::paths::Paths;

/// Driver for per-host service deployments.
#[derive(Parser)]
#[command(name = "plat")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Args)]
struct PodArgs {
    /// Tenant (default: sys)
    #[arg(
        short = 't',
        value_name = "TENANT",
        default_value = "sys",
        hide_default_value = true
    )]
    tenant: String,
    /// Namespace (default: main)
    #[arg(
        short = 'n',
        value_name = "NAMESPACE",
        default_value = "main",
        hide_default_value = true
    )]
    namespace: String,
    /// Pod name
    #[arg(
        short = 'p',
        value_name = "POD",
        default_value = "",
        hide_default_value = true
    )]
    pod: String,
}

impl PodArgs {
    fn pod(&self) -> Pod {
        Pod {
            tenant: self.tenant.clone(),
            namespace: self.namespace.clone(),
            pod: self.pod.clone(),
        }
    }
}

#[derive(Args)]
struct OptService {
    #[command(flatten)]
    pod: PodArgs,
    service: Option<String>,
}

#[derive(Args)]
struct Service {
    #[command(flatten)]
    pod: PodArgs,
    service: String,
}

#[derive(Subcommand)]
enum Cmd {
    /// Converge every service to its env file's STATUS.
    Apply,
    /// Show declared and actual state of every service under PLAT_HOME.
    Ps,
    /// Bring SERVICE up. With no SERVICE: bring everything up.
    Up(OptService),
    /// Bring SERVICE down. With no SERVICE: bring everything down.
    Down(OptService),
    /// Print resolved IP:port:port mappings for SERVICE.
    Port(Service),
    /// Remove SERVICE's env file.
    Undefine(Service),
    /// Run `destroy` on SERVICE.
    Destroy(Service),
    /// Run `restart` on SERVICE.
    Restart(Service),
    /// Run `logs` on SERVICE.
    Logs(Service),
    /// Run `backup` on SERVICE.
    Backup(Service),
    /// Run `restore` on SERVICE.
    Restore(Service),
    /// Run `push` on SERVICE.
    Push(Service),
    /// Run `pull` on SERVICE.
    Pull(Service),
}

fn up_or_down(paths: &Paths, action: &str, args: &OptService) {
    match args.service.as_deref().filter(|s| !s.is_empty()) {
        None => exit(actions::do_apply(paths, Some(action))),
        Some(service) => actions::dispatch(paths, &args.pod.pod(), service, &[action]),
    }
}

fn main() {
    let cli = Cli::parse();
    let paths = Paths::from_env();
    if let Err(e) = std::env::set_current_dir(&paths.root) {
        eprintln!("Error: {}: {e}", paths.root.display());
        exit(1);
    }

    let passthrough = |action: &str, s: &Service| -> ! {
        actions::dispatch(&paths, &s.pod.pod(), &s.service, &[action])
    };

    match &cli.command {
        Cmd::Apply => exit(actions::do_apply(&paths, None)),
        Cmd::Ps => actions::ps(&paths),
        Cmd::Up(a) => up_or_down(&paths, "up", a),
        Cmd::Down(a) => up_or_down(&paths, "down", a),
        Cmd::Port(s) => actions::port(&paths, &s.pod.pod(), &s.service),
        Cmd::Undefine(s) => actions::undefine(&paths, &s.pod.pod(), &s.service),
        Cmd::Destroy(s) => passthrough("destroy", s),
        Cmd::Restart(s) => passthrough("restart", s),
        Cmd::Logs(s) => passthrough("logs", s),
        Cmd::Backup(s) => passthrough("backup", s),
        Cmd::Restore(s) => passthrough("restore", s),
        Cmd::Push(s) => passthrough("push", s),
        Cmd::Pull(s) => passthrough("pull", s),
    }
}
