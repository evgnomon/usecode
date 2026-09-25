// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `workd serve`: start the API server (HTTPS with mutual TLS).

use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use clap::{ArgAction, Args};

use super::{DEFAULT_HOST, DEFAULT_PORT, check_files};
use crate::server::{AppState, router, tls};

/// Where run outputs are stored (overridable for testing via `WORKD_BASE_DIR`).
const BASE_OUTPUT_DIR: &str = "/var/z";
/// Server certificate directory (overridable via `WORKD_SERVER_CERT_DIR`).
const SERVER_CERT_DIR: &str = "/etc/x509";

#[derive(Args)]
pub struct ServeArgs {
    /// Host to bind to
    #[arg(short = 'h', long, default_value = DEFAULT_HOST)]
    pub host: String,
    /// Port to bind to
    #[arg(short = 'p', long, default_value_t = DEFAULT_PORT)]
    pub port: u16,
    /// Enable auto-reload (accepted for compatibility; no effect)
    #[arg(long)]
    pub reload: bool,
    /// Server name for certificate lookup (default: hostname)
    #[arg(long, env = "WORKD_SERVER_NAME")]
    pub server_name: Option<String>,
    /// Print help
    #[arg(long, action = ArgAction::Help)]
    help: Option<bool>,
}

fn env_path(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

pub fn run(args: &ServeArgs) -> Result<ExitCode> {
    let name = args
        .server_name
        .clone()
        .unwrap_or_else(|| gethostname::gethostname().to_string_lossy().into_owned());
    let cert_dir = env_path("WORKD_SERVER_CERT_DIR", SERVER_CERT_DIR);
    let cert = cert_dir.join(format!("{name}.pub"));
    let key = cert_dir.join(format!("{name}.key"));
    let ca = cert_dir.join("ca.pub");
    if let Some(code) = check_files(&[
        ("server cert: ", &cert),
        ("server key:  ", &key),
        ("CA cert:     ", &ca),
    ]) {
        return Ok(code);
    }

    println!("Starting server on https://{}:{}", args.host, args.port);
    println!("Server cert: {}", cert.display());
    println!("CA cert:     {}", ca.display());

    let addr = (args.host.as_str(), args.port)
        .to_socket_addrs()
        .with_context(|| format!("resolving {}", args.host))?
        .next()
        .with_context(|| format!("no address for {}", args.host))?;
    let state = AppState {
        db: Arc::new(Mutex::new(Vec::new())),
        base_dir: env_path("WORKD_BASE_DIR", BASE_OUTPUT_DIR),
    };

    let rt = tokio::runtime::Runtime::new().context("starting tokio runtime")?;
    rt.block_on(async {
        let config = tls::server_config(&cert, &key, &ca)?;
        axum_server::bind_rustls(addr, config)
            .serve(router(state).into_make_service())
            .await
            .with_context(|| format!("serving on {addr}"))
    })?;
    Ok(ExitCode::SUCCESS)
}
