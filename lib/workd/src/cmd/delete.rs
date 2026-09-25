// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `workd delete` and `workd clear`.

use std::process::ExitCode;

use anyhow::Result;
use clap::Args;
use serde_json::json;

use super::{Target, job_runs, query, require_runs};
use crate::client::{ApiClient, ClientError};
use crate::util::{confirm_or_abort, py_str};

#[derive(Args)]
pub struct DeleteArgs {
    #[command(flatten)]
    target: Target,
    /// Skip confirmation
    #[arg(short, long)]
    yes: bool,
}

#[derive(Args)]
pub struct ClearArgs {
    job_name: Option<String>,
    /// Filter by tenant
    #[arg(short, long, default_value = "system")]
    tenant: String,
    /// Filter by namespace
    #[arg(short, long, default_value = "main")]
    namespace: String,
    /// Skip confirmation
    #[arg(short, long)]
    yes: bool,
}

pub fn delete(client: &ApiClient, a: DeleteArgs) -> Result<ExitCode> {
    if let Some(run_id) = &a.target.run_id {
        if !a.yes {
            confirm_or_abort(&format!("Delete run {run_id}?"));
        }
        let result = client.delete(&format!("/runs/{run_id}"))?;
        println!("Deleted: {}", py_str(result.get("run_id")));
        return Ok(ExitCode::SUCCESS);
    }

    let job_name = &a.target.job_name;
    let runs = job_runs(client, &a.target)?;
    require_runs(&runs, job_name);

    if !a.yes {
        println!("Found {} run(s) for job '{job_name}':", runs.len());
        for r in &runs {
            println!("  {}  {}", py_str(r.get("run_id")), py_str(r.get("status")));
        }
        confirm_or_abort("Delete all?");
    }

    let (mut deleted, mut skipped) = (0usize, 0usize);
    for r in &runs {
        let id = py_str(r.get("run_id"));
        match client.delete(&format!("/runs/{id}")) {
            Ok(_) => {
                deleted += 1;
                println!("Deleted: {id}");
            }
            Err(ClientError::Status(code, _)) => {
                skipped += 1;
                println!("Skipped: {id} ({code})");
            }
            Err(e) => return Err(e.into()),
        }
    }
    println!("\n{deleted} deleted, {skipped} skipped.");
    Ok(ExitCode::SUCCESS)
}

pub fn clear(client: &ApiClient, a: ClearArgs) -> Result<ExitCode> {
    let job_name = a.job_name.as_deref().filter(|j| !j.is_empty());
    if !a.yes {
        match job_name {
            Some(j) => {
                confirm_or_abort(&format!("Delete all runs except the latest for job '{j}'?"))
            }
            None => confirm_or_abort("Delete all runs except the latest for each job?"),
        }
    }

    let q = query(&[
        ("job_name", job_name),
        ("tenant_name", Some(&a.tenant)),
        ("namespace", Some(&a.namespace)),
    ]);
    let result = client.post(&format!("/clear{q}"), &json!({}))?;
    let list = |k: &str| result[k].as_array().cloned().unwrap_or_default();
    let (deleted, skipped) = (list("deleted"), list("skipped"));

    for rid in &deleted {
        println!("Deleted: {}", py_str(Some(rid)));
    }
    for rid in &skipped {
        println!("Skipped (running): {}", py_str(Some(rid)));
    }
    println!("\n{} deleted, {} skipped.", deleted.len(), skipped.len());
    Ok(ExitCode::SUCCESS)
}
