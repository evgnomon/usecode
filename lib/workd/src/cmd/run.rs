// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `workd run`: start a command on the server.

use std::collections::BTreeMap;
use std::io::Write;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::Result;
use clap::{Args, CommandFactory};
use serde_json::{Value, json};

use super::Cli;
use crate::client::ApiClient;
use crate::util::{py_str, style};

#[derive(Args)]
pub struct RunArgs {
    /// Tenant name
    #[arg(short, long, default_value = "system")]
    tenant: String,
    /// Namespace
    #[arg(short, long, default_value = "main")]
    namespace: String,
    /// Job name
    #[arg(short, long)]
    job: String,
    /// Environment variables (KEY=VALUE)
    #[arg(short, long)]
    env: Vec<String>,
    /// Working directory
    #[arg(short, long)]
    workdir: Option<String>,
    /// Wait for the process to complete
    #[arg(long)]
    wait: bool,
    /// Wait and stream stdout when done
    #[arg(short, long)]
    pub follow: bool,
    /// Command and its arguments
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 0..)]
    pub cmd_args: Vec<String>,
}

/// Parse KEY=VALUE pairs, ignoring entries without `=`.
fn parse_env(env: &[String]) -> BTreeMap<String, String> {
    env.iter()
        .filter_map(|e| e.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

pub fn run(client: &ApiClient, a: RunArgs) -> Result<ExitCode> {
    let Some((command, args)) = a.cmd_args.split_first() else {
        Cli::command()
            .error(
                clap::error::ErrorKind::MissingRequiredArgument,
                "Missing command to run.",
            )
            .exit();
    };

    let mut payload = json!({
        "tenant_name": a.tenant,
        "namespace": a.namespace,
        "job_name": a.job,
        "command": command,
        "args": args,
        "env": parse_env(&a.env),
    });
    if let Some(dir) = a.workdir.filter(|d| !d.is_empty()) {
        payload["working_dir"] = Value::String(dir);
    }

    let result = client.post("/runs", &payload)?;
    let run_id = result["run_id"].as_str().unwrap_or_default().to_string();

    println!("Started run: {}", style(&run_id, "green"));
    println!("Status: {}", py_str(result.get("status")));
    println!("Output dir: {}", py_str(result.get("output_dir")));

    if !(a.wait || a.follow) {
        return Ok(ExitCode::SUCCESS);
    }

    print!("\nWaiting for completion...");
    let _ = std::io::stdout().flush();
    let status = loop {
        let status = client.get_json(&format!("/runs/{run_id}"))?;
        match status["status"].as_str().unwrap_or_default() {
            "completed" | "failed" | "error" => break status,
            _ => {
                print!(".");
                let _ = std::io::stdout().flush();
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    };
    println!();
    let state = status["status"].as_str().unwrap_or_default();
    let color = match state {
        "completed" => "green",
        _ => "red",
    };
    println!("Status: {}", style(state, color));
    println!(
        "Exit code: {}",
        status
            .get("exit_code")
            .map_or("N/A".to_string(), |v| py_str(Some(v)))
    );

    if a.follow {
        println!("\n--- stdout ---");
        println!("{}", client.get_text(&format!("/runs/{run_id}/stdout"))?);
        let stderr = client.get_text(&format!("/runs/{run_id}/stderr"))?;
        if !stderr.trim().is_empty() {
            println!("\n--- stderr ---");
            println!("{stderr}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_pairs() {
        let env = vec!["A=1".to_string(), "B=x=y".to_string(), "C".to_string()];
        let m = parse_env(&env);
        assert_eq!(m.get("A").map(String::as_str), Some("1"));
        assert_eq!(m.get("B").map(String::as_str), Some("x=y"));
        assert!(!m.contains_key("C"));
    }
}
