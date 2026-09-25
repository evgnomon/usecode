// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `workd list`: list runs with optional filters.

use std::process::ExitCode;

use anyhow::Result;
use clap::Args;

use super::{dumps, query};
use crate::client::ApiClient;
use crate::util::{py_str, status_color, style};

#[derive(Args)]
pub struct ListArgs {
    /// Filter by tenant
    #[arg(short, long, default_value = "system")]
    tenant: String,
    /// Filter by namespace
    #[arg(short, long, default_value = "main")]
    namespace: String,
    /// Filter by job name
    #[arg(short, long)]
    job: Option<String>,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

pub fn run(client: &ApiClient, a: ListArgs) -> Result<ExitCode> {
    let q = query(&[
        ("tenant_name", Some(&a.tenant)),
        ("namespace", Some(&a.namespace)),
        ("job_name", a.job.as_deref()),
    ]);
    let result = client.get_json(&format!("/runs{q}"))?;

    if a.json {
        println!("{}", dumps(&result));
        return Ok(ExitCode::SUCCESS);
    }

    let runs = result.as_array().cloned().unwrap_or_default();
    if runs.is_empty() {
        println!("No runs found.");
        return Ok(ExitCode::SUCCESS);
    }

    println!("{:<40} {:<12} {:<6} CREATED", "RUN ID", "STATUS", "EXIT");
    println!("{}", "-".repeat(80));
    for run in &runs {
        let status = run["status"].as_str().unwrap_or_default();
        let pad = " ".repeat(12usize.saturating_sub(status.chars().count()));
        let exit_code = match run.get("exit_code") {
            None | Some(serde_json::Value::Null) => "-".to_string(),
            v => py_str(v),
        };
        println!(
            "{:<40} {}{} {:<6} {}",
            py_str(run.get("run_id")),
            style(status, status_color(status)),
            pad,
            exit_code,
            py_str(run.get("created_at"))
        );
    }
    Ok(ExitCode::SUCCESS)
}
