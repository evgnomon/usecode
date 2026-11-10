// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! uc-agent-api: the HTTP backend for usecode agent.

mod accounts;
mod billing;
mod chat_store;
mod config;
mod crypto;
mod db;
mod error;
mod extract;
mod import;
mod mail;
mod migrate;
mod model_config;
mod model_process;
mod models;
mod oidc;
mod otp;
mod providers;
mod relay;
mod routes;
mod schema;
mod server_tasks;
mod sms;
mod store;
mod tasks;
#[cfg(test)]
mod testing;
mod web;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use crate::config::Settings;
use crate::db::{Db, Partition};
use crate::store::Store;

// How often to resume tasks that are parked waiting on a slow provider-side
// operation (e.g. "wait for a server deletion to finish"). See tasks.rs.
const TASK_SWEEP_INTERVAL: Duration = Duration::from_secs(5);

/// Everything a request handler needs.
pub struct App {
    pub settings: Settings,
    pub store: Store,
    pub cipher: crypto::Cipher,
    pub chat: chat_store::ChatStore,
    pub templates: web::Templates,
    pub mailer: mail::Mailer,
    pub limits: accounts::Limits,
    pub http: reqwest::Client,
    /// The hosts' relay sockets and the shells riding them, on this instance.
    pub relay: relay::Hub,
}

impl App {
    /// A user's decrypted credentials for one provider. They live on the
    /// user's own database instance, so the caller has to say which one —
    /// from the authenticated client, or from the task's owner.
    pub async fn decrypted_credentials(
        &self,
        partition: &Partition,
        user_id: &Uuid,
        provider: &str,
    ) -> Result<Option<Value>> {
        let encrypted = self
            .store
            .get_provider_credentials(partition, user_id, provider)
            .await?;
        encrypted
            .map(|token| self.cipher.decrypt_json(&token))
            .transpose()
    }
}

async fn sweep_tasks_forever(app: Arc<App>) {
    loop {
        tokio::time::sleep(TASK_SWEEP_INTERVAL).await;
        if let Err(err) = tasks::sweep(&app).await {
            tracing::error!("Task sweep failed: {err:#}");
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

const USAGE: &str = "usage: uc-agent-api [serve]
       uc-agent-api migrate [--status]
       uc-agent-api import-console POSTGRES_URL

  serve             migrate every database, then serve the API (the default)
  migrate           bring every database up to date and exit
  migrate --status  say what each database has applied and what is pending
  import-console    copy the accounts, sessions and billing of the console's
                    own database (before it used this API) into this one;
                    running it again skips what is already here

Settings come from USECODE_AGENT_* variables (and .env); see the README.";

async fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    if args.iter().any(|arg| matches!(*arg, "-h" | "--help")) {
        println!("{USAGE}");
        return Ok(());
    }
    match args.as_slice() {
        [] | ["serve"] => serve(Settings::from_env()?).await,
        ["migrate"] => migrate::run(&Settings::from_env()?).await,
        ["migrate", "--status"] => migrate::status(&Settings::from_env()?).await,
        ["import-console", source] => import::run(Settings::from_env()?, source).await,
        ["-h" | "--help" | "help"] => {
            println!("{USAGE}");
            Ok(())
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}

async fn serve(settings: Settings) -> Result<()> {
    settings.require_node_name()?;
    tracing::info!("Starting API instance {:?}", settings.node_name);

    migrate::run(&settings).await?;
    let db = Arc::new(Db::connect(&settings).await?);
    db.seed_shard_ranges().await?;

    let relay = relay::Hub::new(relay::internal_url(&settings).await);
    tracing::info!("Other instances reach this one at {}", relay.internal_url);
    let app = Arc::new(App {
        relay,
        cipher: crypto::Cipher::new(&settings.secret_key),
        store: Store::new(db.clone()),
        chat: chat_store::ChatStore::default(),
        templates: web::Templates::new()?,
        mailer: mail::Mailer::from_settings(&settings),
        limits: accounts::Limits::default(),
        http: reqwest::Client::new(),
        settings,
    });

    let sweeper = tokio::spawn(sweep_tasks_forever(app.clone()));
    let listener = tokio::net::TcpListener::bind(&app.settings.bind)
        .await
        .with_context(|| format!("binding {}", app.settings.bind))?;
    tracing::info!("Listening on http://{}", listener.local_addr()?);
    axum::serve(listener, routes::router(app))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    sweeper.abort();
    db.close().await;
    Ok(())
}

#[tokio::main]
async fn main() {
    // One process-wide TLS provider for reqwest and the SMTP client;
    // graviola is pure Rust, like the rest of the workspace.
    let _ = rustls_graviola::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    if let Err(err) = run().await {
        tracing::error!("{err:#}");
        std::process::exit(1);
    }
}
