// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-push` — push local container images to the registry behind the
//! bastion.

use clap::Parser;
use std::process::ExitCode;
use uc::registry::{Config, Session};

const SUMMARY: &str = "push container images to the registry behind the bastion";

/// Push local container images to the registry deployed by
/// deploy/playbooks/registry.yaml.
///
/// Opens an SSH tunnel to the registry through the bastion, logs in, then
/// tags each image under the registry and pushes it.
#[derive(Parser)]
#[command(name = "uc push", bin_name = "uc push", version)]
struct Cli {
    #[command(flatten)]
    config: Config,

    /// Images to push, as <image>[:tag].
    #[arg(required = true, value_name = "IMAGE")]
    images: Vec<String>,
}

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("{SUMMARY}");
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    match Session::open(&cli.config).and_then(|s| s.push(&cli.images)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc-push: {err:#}");
            ExitCode::FAILURE
        }
    }
}
