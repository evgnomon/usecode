// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! MCP server that operates usecode agent over stdio.

mod client;
mod compose;
mod config;
mod error;
mod server;
mod ssh;

use std::sync::Arc;

use rmcp::ServiceExt;
use rmcp::transport::stdio;

use crate::config::Settings;
use crate::server::UsecodeServer;

const USAGE: &str = "usage: uc agent mcp

The MCP server for operating the usecode agent, over stdin and stdout: an
MCP client starts it and talks to it, it is not for typing at. It takes no
arguments.

Settings come from USECODE_MCP_* variables (and .env):
  USECODE_MCP_API_BASE_URLS        the API instances, comma separated
  USECODE_MCP_API_BASE_URL         one instance, instead of those
  USECODE_MCP_API_KEY              your usecode.dev API key
  USECODE_MCP_REQUEST_TIMEOUT_SECONDS
  USECODE_MCP_API_VERIFY_SSL       0 to skip TLS verification
  USECODE_MCP_COMPOSE_FILE
  USECODE_MCP_CONTAINER_CLI
  USECODE_MCP_SSH_CONFIG_DIR       where ssh_config entries of new servers go
  USECODE_MCP_SSH_PUBLIC_KEY       the key new servers let in";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        if args.iter().any(|arg| arg == "-h" || arg == "--help") {
            println!("{USAGE}");
            return Ok(());
        }
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    let settings = Arc::new(Settings::from_env());
    let service = UsecodeServer::new(settings)?.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
