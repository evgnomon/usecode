// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! CLI definition and subcommand dispatch.

mod delete;
mod list;
mod output;
mod run;
mod serve;
mod status;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use crate::client::ApiClient;
use crate::util::{getuser, style};

pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 8000;

#[derive(Parser)]
#[command(
    name = "workd",
    version,
    about = "Process Runner - Run and manage processes via API."
)]
pub struct Cli {
    /// Base URL of the Process Runner API
    #[arg(
        short = 'u',
        long,
        default_value = "https://localhost:8000",
        env = "PROCESS_RUNNER_URL"
    )]
    url: String,
    /// Client name for certificate lookup (default: current user)
    #[arg(long, env = "WORKD_CLIENT_NAME")]
    client_name: Option<String>,
    /// CA certificate path (default: ~/.x509/ca.pub)
    #[arg(long = "ca", env = "WORKD_CA_CERT")]
    ca_cert: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

/// Job selection shared by the per-job subcommands.
#[derive(Args)]
pub struct Target {
    job_name: String,
    /// Look up by run UUID instead of job name
    #[arg(long = "uuid")]
    run_id: Option<String>,
    /// Filter by tenant
    #[arg(short, long, default_value = "system")]
    tenant: String,
    /// Filter by namespace
    #[arg(short, long, default_value = "main")]
    namespace: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Process Runner API server (HTTPS with mutual TLS).
    #[command(disable_help_flag = true)]
    Serve(serve::ServeArgs),
    /// Run a command.
    ///
    /// Options must come before the command. Everything after the options
    /// is passed as the command and its arguments.
    ///
    /// Examples:
    ///     workd run -j hello -t acme ps -aux
    ///     workd run -j backup -t acme -n prod pg_dump mydb --wait
    ///     workd run -j hello echo "Hello World" -f
    #[command(verbatim_doc_comment)]
    Run(run::RunArgs),
    /// List all runs with optional filters.
    List(list::ListArgs),
    /// Get status of a run by job name (latest run) or --uuid.
    Status(status::StatusArgs),
    /// Get stdout output for a run by job name (latest run) or --uuid.
    Stdout(Target),
    /// Get stderr output for a run by job name (latest run) or --uuid.
    Stderr(Target),
    /// Get combined logs (stdout and stderr) for a run by job name (latest run) or --uuid.
    Logs(output::LogsArgs),
    /// Delete runs for a job, or a specific run with --uuid.
    Delete(delete::DeleteArgs),
    /// Delete all runs except the latest run for each job.
    ///
    /// Optionally provide JOB_NAME to clear only that job's old runs.
    Clear(delete::ClearArgs),
    /// Check API health status.
    Health,
}

pub fn dispatch(cli: Cli) -> Result<ExitCode> {
    // The server does not need client certificates.
    if let Commands::Serve(args) = &cli.command {
        return serve::run(args);
    }
    let client = make_client(&cli)?;
    match cli.command {
        Commands::Serve(_) => unreachable!("handled above"),
        Commands::Run(a) => run::run(&client, a),
        Commands::List(a) => list::run(&client, a),
        Commands::Status(a) => status::run(&client, a),
        Commands::Stdout(t) => output::stream(&client, t, "stdout"),
        Commands::Stderr(t) => output::stream(&client, t, "stderr"),
        Commands::Logs(a) => output::logs(&client, a),
        Commands::Delete(a) => delete::delete(&client, a),
        Commands::Clear(a) => delete::clear(&client, a),
        Commands::Health => health(&client),
    }
}

fn client_cert_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".x509")
}

/// Report missing files (like the original, one line each) and fail.
pub fn check_files(files: &[(&str, &Path)]) -> Option<ExitCode> {
    let missing: Vec<_> = files.iter().filter(|(_, p)| !p.exists()).collect();
    for (label, path) in &missing {
        eprintln!("Error: missing {label}{}", path.display());
    }
    (!missing.is_empty()).then_some(ExitCode::FAILURE)
}

fn make_client(cli: &Cli) -> Result<ApiClient> {
    let name = cli.client_name.clone().unwrap_or_else(getuser);
    let cert_dir = client_cert_dir();
    let client_cert = cert_dir.join(format!("{name}.pub"));
    let client_key = cert_dir.join(format!("{name}.key"));
    let ca_path = cli
        .ca_cert
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| cert_dir.join("ca.pub"));
    if check_files(&[
        ("client cert: ", &client_cert),
        ("client key:  ", &client_key),
        ("CA cert:     ", &ca_path),
    ])
    .is_some()
    {
        std::process::exit(1);
    }
    ApiClient::new(&cli.url, &client_cert, &client_key, &ca_path)
}

/// Percent-encode a query parameter value.
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Build a query string, skipping empty values (Python falsy semantics).
pub fn query(params: &[(&str, Option<&str>)]) -> String {
    let parts: Vec<String> = params
        .iter()
        .filter_map(|(k, v)| {
            v.filter(|v| !v.is_empty())
                .map(|v| format!("{k}={}", encode(v)))
        })
        .collect();
    match parts.is_empty() {
        true => String::new(),
        false => format!("?{}", parts.join("&")),
    }
}

/// Query for runs of a given job, newest first.
pub fn job_runs(client: &ApiClient, t: &Target) -> Result<Vec<serde_json::Value>> {
    let q = query(&[
        ("job_name", Some(&t.job_name)),
        ("tenant_name", Some(&t.tenant)),
        ("namespace", Some(&t.namespace)),
    ]);
    let runs = client.get_json(&format!("/runs{q}"))?;
    Ok(runs.as_array().cloned().unwrap_or_default())
}

/// Print "no runs" and exit 1 when empty.
pub fn require_runs(runs: &[serde_json::Value], job_name: &str) {
    if runs.is_empty() {
        println!("No runs found for job '{job_name}'.");
        std::process::exit(1);
    }
}

/// Resolve the target to a run id (--uuid, or the latest run of the job).
pub fn resolve_run_id(client: &ApiClient, t: &Target) -> Result<String> {
    if let Some(id) = &t.run_id {
        return Ok(id.clone());
    }
    let runs = job_runs(client, t)?;
    require_runs(&runs, &t.job_name);
    Ok(runs[0]["run_id"].as_str().unwrap_or_default().to_string())
}

/// Serialize like Python's `json.dumps(value, indent=2)` (ASCII-only output).
pub fn dumps(value: &serde_json::Value) -> String {
    let pretty = serde_json::to_string_pretty(value).unwrap_or_default();
    let mut out = String::with_capacity(pretty.len());
    for c in pretty.chars() {
        match c.is_ascii() {
            true => out.push(c),
            false => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{u:04x}"));
                }
            }
        }
    }
    out
}

fn health(client: &ApiClient) -> Result<ExitCode> {
    let result = client.get_json("/health")?;
    println!(
        "Status: {}",
        style(result["status"].as_str().unwrap_or_default(), "green")
    );
    println!("Active runs: {}", result["active_runs"]);
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn query_skips_empty_and_encodes() {
        assert_eq!(query(&[("a", None), ("b", Some(""))]), "");
        assert_eq!(
            query(&[("job_name", Some("a b&c")), ("namespace", Some("main"))]),
            "?job_name=a%20b%26c&namespace=main"
        );
    }

    #[test]
    fn dumps_is_ascii() {
        let v = serde_json::json!({"b": "é😀", "a": []});
        assert_eq!(
            dumps(&v),
            "{\n  \"b\": \"\\u00e9\\ud83d\\ude00\",\n  \"a\": []\n}"
        );
    }

    #[test]
    fn run_trailing_args() {
        let cli = Cli::try_parse_from(["workd", "run", "-j", "x", "echo", "-f", "--wait"]).unwrap();
        match cli.command {
            Commands::Run(a) => {
                assert!(!a.follow);
                assert_eq!(a.cmd_args, vec!["echo", "-f", "--wait"]);
            }
            _ => panic!("expected run"),
        }
    }

    #[test]
    fn serve_short_host() {
        let cli = Cli::try_parse_from(["workd", "serve", "-h", "0.0.0.0", "-p", "9"]).unwrap();
        match cli.command {
            Commands::Serve(a) => {
                assert_eq!(a.host, "0.0.0.0");
                assert_eq!(a.port, 9);
            }
            _ => panic!("expected serve"),
        }
    }
}
