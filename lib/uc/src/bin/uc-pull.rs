// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-pull` — pull container images from the registry behind the bastion.

use clap::Parser;
use std::process::ExitCode;
use uc::registry::{Config, Session};

const SUMMARY: &str = "pull container images from the registry behind the bastion";

/// Pull container images from the registry deployed by
/// deploy/playbooks/registry.yaml.
///
/// Opens an SSH tunnel to the registry through the bastion, logs in, then
/// pulls each image.
#[derive(Parser)]
#[command(name = "uc pull", bin_name = "uc pull", version)]
struct Cli {
    /// Also tag each pulled image without the registry host prefix
    /// (localhost:5000/foo -> foo).
    #[arg(short, long)]
    strip_host: bool,

    #[command(flatten)]
    config: Config,

    /// Images to pull, as <image>[:tag].
    #[arg(required = true, value_name = "IMAGE")]
    images: Vec<String>,
}

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("{SUMMARY}");
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    match Session::open(&cli.config).and_then(|s| s.pull(&cli.images, cli.strip_host)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc-pull: {err:#}");
            ExitCode::FAILURE
        }
    }
}
