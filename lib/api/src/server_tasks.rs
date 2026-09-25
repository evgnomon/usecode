// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `create_server` / `delete_server` tasks: the resumable workflows behind
//! POST /servers and DELETE /servers/{id}.
//!
//! Neither creating nor deleting a server is a single request/response round
//! trip — providers take a while to actually provision or tear down a
//! machine, and we don't want to hold an HTTP request open for that. So the
//! routes only *start* the workflow and return a task id; a periodic sweep
//! resumes it until the provider confirms the outcome.
//!
//! For creation in particular: the provider is the only source of truth for
//! a server's fixed attributes (IP addresses, final status, etc), and those
//! aren't known until the provider has actually finished provisioning. So no
//! `servers` row is created up front — the `requested` step just issues the
//! provider's create call and remembers the provider-side id; the
//! `confirming` step polls until the provider reports the server as
//! reachable (it has a public IPv4 address), and only then writes the local
//! `servers` row, matching the pattern used to remove it on delete.

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::App;
use crate::models::CloudServerCreateIn;
use crate::providers::Provider;
use crate::store::NewServer;
use crate::tasks::{Next, TaskContext};

pub const CREATE_SERVER: &str = "create_server";
pub const DELETE_SERVER: &str = "delete_server";
pub const REQUESTED: &str = "requested";
pub const CONFIRMING: &str = "confirming";

#[derive(Serialize, Deserialize)]
pub struct CreateRequested {
    pub provider: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub spec: CloudServerCreateIn,
}

#[derive(Serialize, Deserialize)]
struct CreateConfirming {
    #[serde(flatten)]
    requested: CreateRequested,
    provider_server_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct DeletePayload {
    pub provider: String,
    pub provider_server_id: String,
}

/// Every registered (kind, state) handler.
#[derive(Debug, Clone, Copy)]
pub enum Step {
    CreateRequested,
    CreateConfirming,
    DeleteRequested,
    DeleteConfirming,
}

fn payload<T: for<'de> Deserialize<'de>>(ctx: &TaskContext) -> Result<T> {
    serde_json::from_value(ctx.payload.clone()).context("task payload does not match its step")
}

async fn credentials(app: &App, ctx: &TaskContext, provider: &str) -> Result<(Provider, Value)> {
    let provider = Provider::parse(provider)?;
    let credentials = app
        .decrypted_credentials(&ctx.user_partition, &ctx.user_id, provider.name())
        .await?
        .ok_or_else(|| anyhow!("No {} credentials configured", provider.name()))?;
    Ok((provider, credentials))
}

impl Step {
    pub fn for_task(kind: &str, state: &str) -> Option<Self> {
        match (kind, state) {
            (CREATE_SERVER, REQUESTED) => Some(Self::CreateRequested),
            (CREATE_SERVER, CONFIRMING) => Some(Self::CreateConfirming),
            (DELETE_SERVER, REQUESTED) => Some(Self::DeleteRequested),
            (DELETE_SERVER, CONFIRMING) => Some(Self::DeleteConfirming),
            _ => None,
        }
    }

    pub async fn run(self, app: &App, ctx: &TaskContext) -> Result<Next> {
        match self {
            Self::CreateRequested => create_requested(app, ctx).await,
            Self::CreateConfirming => create_confirming(app, ctx).await,
            Self::DeleteRequested => delete_requested(app, ctx).await,
            Self::DeleteConfirming => delete_confirming(app, ctx).await,
        }
    }
}

async fn create_requested(app: &App, ctx: &TaskContext) -> Result<Next> {
    let requested: CreateRequested = payload(ctx)?;
    let (provider, credentials) = credentials(app, ctx, &requested.provider).await?;
    let created = provider
        .create_server(&credentials, &requested.spec)
        .await?;
    let next = CreateConfirming {
        requested,
        provider_server_id: created.id,
    };
    Ok(Next::Park {
        state: CONFIRMING,
        payload: serde_json::to_value(next)?,
    })
}

async fn create_confirming(app: &App, ctx: &TaskContext) -> Result<Next> {
    let confirming: CreateConfirming = payload(ctx)?;
    let (provider, credentials) = credentials(app, ctx, &confirming.requested.provider).await?;
    let servers = provider.list_servers(&credentials).await?;
    let found = servers
        .iter()
        .find(|server| server.id == confirming.provider_server_id);
    let Some(server) = found.filter(|server| server.public_ip4.is_some()) else {
        // Still provisioning — the provider hasn't assigned a public address
        // yet. Stay parked here, the next sweep will check again.
        return Ok(Next::Park {
            state: CONFIRMING,
            payload: ctx.payload.clone(),
        });
    };

    app.store
        .create_server(
            &ctx.user_partition,
            &ctx.user_id,
            NewServer {
                provider: provider.name(),
                provider_server_id: &server.id,
                r#type: &confirming.requested.r#type,
                name: &server.name,
                status: &server.status,
                public_ip4: server.public_ip4.as_deref(),
                public_ip6: server.public_ip6.as_deref(),
            },
        )
        .await?;
    Ok(Next::Done)
}

async fn delete_requested(app: &App, ctx: &TaskContext) -> Result<Next> {
    let delete: DeletePayload = payload(ctx)?;
    let (provider, credentials) = credentials(app, ctx, &delete.provider).await?;
    provider
        .delete_server(&credentials, &delete.provider_server_id)
        .await?;
    Ok(Next::Park {
        state: CONFIRMING,
        payload: ctx.payload.clone(),
    })
}

async fn delete_confirming(app: &App, ctx: &TaskContext) -> Result<Next> {
    let delete: DeletePayload = payload(ctx)?;
    let (provider, credentials) = credentials(app, ctx, &delete.provider).await?;
    let still_there = provider
        .list_servers(&credentials)
        .await?
        .iter()
        .any(|server| server.id == delete.provider_server_id);
    if still_there {
        // Provider hasn't finished tearing the machine down yet — stay
        // parked here, the next sweep will check again.
        return Ok(Next::Park {
            state: CONFIRMING,
            payload: ctx.payload.clone(),
        });
    }

    for resource in ctx.resources.as_array().into_iter().flatten() {
        if resource.get("type").and_then(Value::as_str) == Some("server")
            && let Some(id) = resource.get("id").and_then(Value::as_str)
        {
            app.store
                .delete_server(&ctx.user_partition, &ctx.user_id, id)
                .await?;
        }
    }
    Ok(Next::Done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn confirming_payload_extends_the_requested_one() {
        let requested = json!({
            "provider": "hetzner", "type": "x1-fsn",
            "spec": {"provider": "hetzner", "name": "web", "server_type": "cx22",
                     "image": "ubuntu-24.04", "location": "fsn1", "ssh_keys": []},
        });
        let requested: CreateRequested = serde_json::from_value(requested).unwrap();
        let next = serde_json::to_value(CreateConfirming {
            requested,
            provider_server_id: "42".into(),
        })
        .unwrap();
        assert_eq!(
            next.to_string(),
            r#"{"provider":"hetzner","type":"x1-fsn","spec":{"provider":"hetzner","name":"web","server_type":"cx22","image":"ubuntu-24.04","location":"fsn1","ssh_keys":[]},"provider_server_id":"42"}"#
        );
        let back: CreateConfirming = serde_json::from_value(next).unwrap();
        assert_eq!(back.requested.spec.location.as_deref(), Some("fsn1"));
    }
}
