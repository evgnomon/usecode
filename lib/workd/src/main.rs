// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Process Runner - HTTPS (mutual TLS) server and CLI client.
//!
//! Run arbitrary commands asynchronously, save stdout/stderr to an organized
//! file structure, list and filter runs by tenant, namespace and job name, and
//! retrieve output files for each run.

mod client;
mod cmd;
mod server;
mod util;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let cli = cmd::Cli::parse();
    match cmd::dispatch(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("Error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
