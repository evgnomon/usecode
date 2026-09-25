// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `workd stdout`, `workd stderr` and `workd logs`.

use std::process::ExitCode;

use anyhow::Result;
use clap::Args;

use super::{Target, resolve_run_id};
use crate::client::ApiClient;
use crate::util::style;

#[derive(Args)]
pub struct LogsArgs {
    #[command(flatten)]
    target: Target,
    /// Show only stdout
    #[arg(long)]
    stdout_only: bool,
    /// Show only stderr
    #[arg(long)]
    stderr_only: bool,
}

/// Print one output stream (`stdout` or `stderr`) of a run.
pub fn stream(client: &ApiClient, t: Target, which: &str) -> Result<ExitCode> {
    let run_id = resolve_run_id(client, &t)?;
    println!("{}", client.get_text(&format!("/runs/{run_id}/{which}"))?);
    Ok(ExitCode::SUCCESS)
}

pub fn logs(client: &ApiClient, a: LogsArgs) -> Result<ExitCode> {
    let run_id = resolve_run_id(client, &a.target)?;
    let sections = [
        (!a.stderr_only, "stdout", "=== STDOUT ===", "green"),
        (!a.stdout_only, "stderr", "=== STDERR ===", "red"),
    ];
    for (_, which, header, color) in sections.iter().filter(|s| s.0) {
        let text = client.get_text(&format!("/runs/{run_id}/{which}"))?;
        if !text.trim().is_empty() {
            println!("{}", style(header, color));
            println!("{text}");
        }
    }
    Ok(ExitCode::SUCCESS)
}
