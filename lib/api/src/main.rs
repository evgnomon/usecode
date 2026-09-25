// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! usecode-agent-api: the HTTP backend for usecode agent.

mod chat_store;
mod config;
mod crypto;
mod db;
mod error;
mod extract;
mod migrate;
mod model_config;
mod model_process;
mod models;
mod otp;
mod providers;
mod routes;
mod schema;
mod server_tasks;
mod sms;
mod store;
mod tasks;
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

async fn run() -> Result<()> {
    let settings = Settings::from_env()?;
    tracing::info!("Starting API instance {:?}", settings.node_name);

    migrate::run(&settings).await?;
    let db = Arc::new(Db::connect(&settings).await?);
    db.seed_shard_ranges().await?;

    let app = Arc::new(App {
        cipher: crypto::Cipher::new(&settings.secret_key),
        store: Store::new(db.clone()),
        chat: chat_store::ChatStore::default(),
        templates: web::Templates::new()?,
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
