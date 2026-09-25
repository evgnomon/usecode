// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! MCP server that operates usecode agent over stdio.

mod client;
mod compose;
mod config;
mod error;
mod server;

use std::sync::Arc;

use rmcp::ServiceExt;
use rmcp::transport::stdio;

use crate::config::Settings;
use crate::server::UsecodeServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let settings = Arc::new(Settings::from_env());
    let service = UsecodeServer::new(settings)?.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
