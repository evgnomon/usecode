// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `workd status`: show the status of a run.

use std::process::ExitCode;

use anyhow::Result;
use clap::Args;

use super::{Target, dumps, resolve_run_id};
use crate::client::ApiClient;
use crate::util::{py_str, status_color, style};

#[derive(Args)]
pub struct StatusArgs {
    #[command(flatten)]
    target: Target,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

pub fn run(client: &ApiClient, a: StatusArgs) -> Result<ExitCode> {
    let run_id = resolve_run_id(client, &a.target)?;
    let result = client.get_json(&format!("/runs/{run_id}"))?;

    if a.json {
        println!("{}", dumps(&result));
        return Ok(ExitCode::SUCCESS);
    }

    let status = result["status"].as_str().unwrap_or_default();
    let or_na = |k: &str| result.get(k).map_or("N/A".to_string(), |v| py_str(Some(v)));
    println!("Run ID:     {}", py_str(result.get("run_id")));
    println!("Tenant:     {}", py_str(result.get("tenant_name")));
    println!("Namespace:  {}", py_str(result.get("namespace")));
    println!("Job:        {}", py_str(result.get("job_name")));
    println!("Status:     {}", style(status, status_color(status)));
    println!("Exit Code:  {}", or_na("exit_code"));
    println!("Started:    {}", or_na("started_at"));
    println!("Finished:   {}", or_na("finished_at"));
    println!("Output Dir: {}", py_str(result.get("output_dir")));
    Ok(ExitCode::SUCCESS)
}
