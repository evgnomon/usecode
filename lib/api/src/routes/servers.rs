// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! /servers: the caller's cloud servers, in our own type terminology
//! ("{series}-{city}", e.g. "x1-fsn"), and the provider catalog behind it.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Timelike, Utc};
use futures::future::try_join_all;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::providers::{known_provider, provider_error};
use super::task_out;
use crate::App;
use crate::db::Partition;
use crate::error::{AppError, AppResult};
use crate::extract::{Client, JsonBody};
use crate::models::{
    CloudServer, CloudServerCreateIn, ProviderResourceListOut, ProviderResourceOut, ServerCreateIn,
    ServerListOut, ServerOut, ServerSyncOut, ServerTypeListOut, ServerTypeOut, TaskOut,
    normalize_server_status,
};
use crate::providers::{Provider, ProviderError};
use crate::server_tasks::{
    CREATE_SERVER, CreateRequested, DELETE_SERVER, DeletePayload, REQUESTED,
};
use crate::store::{NewServer, ServerRow};
use crate::tasks::{advance, create_task};

pub fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/servers", post(create_server).get(list_servers))
        .route("/servers/types", get(list_server_types))
        .route("/servers/catalog", get(list_catalog))
        .route("/servers/sync", post(sync_servers))
        .route(
            "/servers/{server_id}",
            get(get_server).delete(delete_server),
        )
}

/// `datetime.isoformat()` of a UTC timestamp: microseconds only when
/// non-zero, and an explicit "+00:00" offset.
fn isoformat(value: DateTime<Utc>) -> String {
    let format = if value.nanosecond() / 1000 == 0 {
        "%Y-%m-%dT%H:%M:%S+00:00"
    } else {
        "%Y-%m-%dT%H:%M:%S%.6f+00:00"
    };
    value.format(format).to_string()
}

fn server_out(server: ServerRow) -> ServerOut {
    ServerOut {
        id: server.id.to_string(),
        name: server.name,
        r#type: server.r#type,
        status: normalize_server_status(&server.status),
        public_ip4: server.public_ip4,
        public_ip6: server.public_ip6,
        created: isoformat(server.created_at),
    }
}

/// Split "{series}-{city}" and look up the series' provider + provider
/// server type, and the city's provider location code (via the fixed mapping
/// POST /servers/sync builds). Both the series and the city must already be
/// known (via a prior /servers/types or /servers/sync call) — neither is
/// accepted unresolved, so e.g. a provider's own raw region slug like 'nyc3'
/// is rejected in favor of our own city code 'nyc'.
async fn resolve_type(app: &App, server_type: &str) -> AppResult<(Provider, String, String)> {
    let Some((series, city)) = server_type
        .split_once('-')
        .filter(|(_, city)| !city.is_empty())
    else {
        return Err(AppError::bad_request(format!(
            "Invalid server type '{server_type}', expected '<series>-<city>' e.g. 'x1-fsn'"
        )));
    };
    let mapping = app
        .store
        .get_server_type_mapping(series)
        .await?
        .ok_or_else(|| AppError::bad_request(format!("Unknown server type series '{series}'")))?;
    let location = app
        .store
        .get_location_mapping(city, &mapping.provider)
        .await?
        .ok_or_else(|| {
            AppError::bad_request(format!(
                "City '{city}' is not available for series '{series}'"
            ))
        })?;
    let provider = known_provider(&mapping.provider)?;
    Ok((provider, mapping.provider_server_type, location))
}

async fn credentials_of(
    app: &App,
    partition: &Partition,
    user_id: &Uuid,
    provider: Provider,
) -> AppResult<Value> {
    app.decrypted_credentials(partition, user_id, provider.name())
        .await?
        .ok_or_else(|| {
            AppError::from(anyhow::anyhow!(
                "{} credentials disappeared while in use",
                provider.name()
            ))
        })
}

/// The providers the caller has credentials for.
async fn configured_providers(
    app: &App,
    client: &crate::store::ApiKeyRecord,
) -> AppResult<Vec<Provider>> {
    Ok(app
        .store
        .list_configured_providers(&client.partition, &client.user_id)
        .await?
        .iter()
        .filter_map(|name| Provider::parse(name).ok())
        .collect())
}

/// Start the create_server task. Provisioning a server is a provider-side
/// workflow that can take a while, and the server's fixed attributes (IP
/// addresses, final status) aren't known until the provider finishes — so no
/// `servers` row exists until then either. This only schedules the workflow;
/// poll GET /tasks/{task_id} until it 404s (meaning it finished), then
/// GET /servers to find the new server.
async fn create_server(
    State(app): State<Arc<App>>,
    Client(client): Client,
    JsonBody(payload): JsonBody<ServerCreateIn>,
) -> AppResult<(StatusCode, Json<TaskOut>)> {
    let (provider, provider_server_type, location) = resolve_type(&app, &payload.r#type).await?;
    let name = provider.name();
    if app
        .decrypted_credentials(&client.partition, &client.user_id, name)
        .await?
        .is_none()
    {
        return Err(AppError::bad_request(format!(
            "No {name} credentials configured, set some via PUT /providers/{name}/credentials"
        )));
    }

    let requested = CreateRequested {
        provider: name.to_string(),
        r#type: payload.r#type,
        spec: CloudServerCreateIn {
            provider: name.to_string(),
            name: payload.name,
            server_type: provider_server_type,
            image: payload.image,
            location: Some(location),
            ssh_keys: payload.ssh_keys,
        },
    };
    let task = create_task(
        &app,
        &client.user_id,
        CREATE_SERVER,
        REQUESTED,
        json!([]),
        serde_json::to_value(requested).map_err(anyhow::Error::from)?,
    )
    .await?;
    let advanced = advance(&app, &task.assignee, &task.id).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(task_out(advanced.unwrap_or(task))),
    ))
}

async fn list_servers(
    State(app): State<Arc<App>>,
    Client(client): Client,
) -> AppResult<Json<ServerListOut>> {
    let servers = app
        .store
        .list_servers(&client.partition, &client.user_id)
        .await?;
    Ok(Json(ServerListOut {
        servers: servers.into_iter().map(server_out).collect(),
    }))
}

fn series_sort_key(series: &str) -> (String, u64) {
    let mut chars = series.chars();
    let head: String = chars.next().into_iter().collect();
    let rest = chars.as_str();
    let number = if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
        rest.parse().unwrap_or(u64::MAX)
    } else {
        0
    };
    (head, number)
}

fn number(value: &Value, key: &str) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or_default()
}

fn cities(value: &Value) -> Vec<String> {
    value
        .get("cities")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|city| city.as_str().map(str::to_string))
        .collect()
}

fn text_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .map(crate::providers::text)
        .unwrap_or_default()
}

/// Every server type available across the caller's configured providers,
/// keyed by our own series (e.g. "x1", "y2" — no city, since specs don't
/// vary by city) with cpu, memory, main-disk specs, and the city codes it's
/// available in. Also mints a stable series for any provider type not seen
/// before, and records any newly-seen cities against it, so it can be passed
/// to POST /servers afterwards.
async fn list_server_types(
    State(app): State<Arc<App>>,
    Client(client): Client,
) -> AppResult<Json<ServerTypeListOut>> {
    let providers = configured_providers(&app, &client).await?;
    let fetches = providers.iter().map(|&provider| {
        let app = &app;
        let client = &client;
        async move {
            let credentials =
                credentials_of(app, &client.partition, &client.user_id, provider).await?;
            let raw = provider
                .list_server_types(&credentials)
                .await
                .map_err(provider_error)?;
            AppResult::Ok((provider, raw))
        }
    });

    let mut types = Vec::new();
    for (provider, raw_types) in try_join_all(fetches).await? {
        for raw in raw_types {
            let series = app
                .store
                .get_or_create_series_for_provider_type(
                    provider.name(),
                    &text_field(&raw, "provider_server_type"),
                    &cities(&raw),
                )
                .await?;
            let cities = app.store.get_server_type_cities(&series).await?;
            types.push(ServerTypeOut {
                r#type: series,
                cpu: number(&raw, "cpu") as i64,
                memory_gb: number(&raw, "memory_gb"),
                disk_gb: number(&raw, "disk_gb"),
                cities,
            });
        }
    }
    types.sort_by_key(|server_type| series_sort_key(&server_type.r#type));
    Ok(Json(ServerTypeListOut { types }))
}

#[derive(Deserialize)]
struct CatalogQuery {
    provider: Option<String>,
    kind: Option<String>,
}

/// The provider catalog data mirrored by the most recent POST /servers/sync
/// — locations, server types, and OS images, optionally filtered by
/// `provider` ("hetzner"/"digitalocean") and/or `kind`
/// ("location"/"server_type"/"image").
async fn list_catalog(
    State(app): State<Arc<App>>,
    _: Client,
    Query(query): Query<CatalogQuery>,
) -> AppResult<Json<ProviderResourceListOut>> {
    let resources = app
        .store
        .list_provider_resources(query.provider.as_deref(), query.kind.as_deref())
        .await?;
    Ok(Json(ProviderResourceListOut {
        resources: resources
            .into_iter()
            .map(|resource| ProviderResourceOut {
                provider: resource.provider,
                kind: resource.kind,
                code: resource.code,
                data: resource.data.0,
            })
            .collect(),
    }))
}

async fn get_server(
    State(app): State<Arc<App>>,
    Client(client): Client,
    Path(server_id): Path<String>,
) -> AppResult<Json<ServerOut>> {
    app.store
        .get_server(&client.partition, &client.user_id, &server_id)
        .await?
        .map(|server| Json(server_out(server)))
        .ok_or_else(|| AppError::not_found("Server not found"))
}

/// Start the delete_server task. Deleting a server is a provider-side
/// workflow that can take a while, so this only schedules it — poll
/// GET /tasks/{task_id} (or GET /servers/{server_id}, which disappears once
/// the task completes) to see it finish.
async fn delete_server(
    State(app): State<Arc<App>>,
    Client(client): Client,
    Path(server_id): Path<String>,
) -> AppResult<(StatusCode, Json<TaskOut>)> {
    let server = app
        .store
        .get_server(&client.partition, &client.user_id, &server_id)
        .await?
        .ok_or_else(|| AppError::not_found("Server not found"))?;
    if app
        .decrypted_credentials(&client.partition, &client.user_id, &server.provider)
        .await?
        .is_none()
    {
        return Err(AppError::bad_request(format!(
            "No {} credentials configured",
            server.provider
        )));
    }

    app.store
        .set_server_status(&client.partition, &client.user_id, &server_id, "deleting")
        .await?;
    let payload = DeletePayload {
        provider: server.provider,
        provider_server_id: server.provider_server_id,
    };
    let task = create_task(
        &app,
        &client.user_id,
        DELETE_SERVER,
        REQUESTED,
        json!([{"type": "server", "id": server_id}]),
        serde_json::to_value(payload).map_err(anyhow::Error::from)?,
    )
    .await?;
    let advanced = advance(&app, &task.assignee, &task.id).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(task_out(advanced.unwrap_or(task))),
    ))
}

/// Mirror everything needed to fill out a create-server spec for one
/// provider into our db: every location, server type, and OS image it offers
/// (raw, as provider_resources rows), plus the fixed series/city mappings our
/// own type strings are built from.
async fn sync_catalog(app: &App, provider: Provider, credentials: &Value) -> Result<(), SyncError> {
    let (locations, server_types, images) = tokio::try_join!(
        provider.list_locations(credentials),
        provider.list_server_types(credentials),
        provider.list_images(credentials),
    )?;
    let name = provider.name();
    for location in &locations {
        let code = text_field(location, "code");
        app.store
            .upsert_provider_resource(name, "location", &code, location)
            .await?;
        app.store
            .set_location_mapping(&code, name, &text_field(location, "provider_location_code"))
            .await?;
    }
    for server_type in &server_types {
        let provider_server_type = text_field(server_type, "provider_server_type");
        app.store
            .upsert_provider_resource(name, "server_type", &provider_server_type, server_type)
            .await?;
        app.store
            .get_or_create_series_for_provider_type(
                name,
                &provider_server_type,
                &cities(server_type),
            )
            .await?;
    }
    for image in &images {
        app.store
            .upsert_provider_resource(name, "image", &text_field(image, "code"), image)
            .await?;
    }
    Ok(())
}

enum SyncError {
    Provider(ProviderError),
    Other(anyhow::Error),
}

impl From<ProviderError> for SyncError {
    fn from(err: ProviderError) -> Self {
        Self::Provider(err)
    }
}

impl From<anyhow::Error> for SyncError {
    fn from(err: anyhow::Error) -> Self {
        Self::Other(err)
    }
}

impl From<SyncError> for AppError {
    fn from(err: SyncError) -> Self {
        match err {
            SyncError::Provider(err) => provider_error(err),
            SyncError::Other(err) => err.into(),
        }
    }
}

/// Fetch every server already provisioned with the caller's configured
/// provider credentials and make sure each one is reflected in our database,
/// matched by the provider's own server id. Also mirrors each provider's
/// full catalog (locations, server types, OS images) into our db, and fixes
/// the series/city mappings our own type strings use — see
/// GET /servers/catalog to inspect what was stored.
async fn sync_servers(
    State(app): State<Arc<App>>,
    Client(client): Client,
) -> AppResult<Json<ServerSyncOut>> {
    let providers = configured_providers(&app, &client).await?;
    let fetches = providers.iter().map(|&provider| {
        let app = &app;
        let client = &client;
        async move {
            let credentials =
                credentials_of(app, &client.partition, &client.user_id, provider).await?;
            let servers = provider
                .list_servers(&credentials)
                .await
                .map_err(provider_error)?;
            sync_catalog(app, provider, &credentials).await?;
            AppResult::Ok(servers)
        }
    });

    let (mut added, mut updated) = (0, 0);
    let mut synced = Vec::new();
    for servers in try_join_all(fetches).await? {
        for server in servers {
            let CloudServer {
                provider,
                id,
                name,
                status,
                server_type,
                location,
                public_ip4,
                public_ip6,
            } = server;
            let series = app
                .store
                .get_or_create_series_for_provider_type(&provider, &server_type, &[])
                .await?;
            app.store
                .set_location_mapping(&location, &provider, &location)
                .await?;
            let (row, created) = app
                .store
                .upsert_server_by_provider_id(
                    &client.partition,
                    &client.user_id,
                    NewServer {
                        provider: &provider,
                        provider_server_id: &id,
                        r#type: &format!("{series}-{location}"),
                        name: &name,
                        status: &status,
                        public_ip4: public_ip4.as_deref(),
                        public_ip6: public_ip6.as_deref(),
                    },
                )
                .await?;
            if created {
                added += 1;
            } else {
                updated += 1;
            }
            synced.push(server_out(row));
        }
    }
    Ok(Json(ServerSyncOut {
        added,
        updated,
        servers: synced,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn isoformat_matches_python() {
        let whole = Utc.with_ymd_and_hms(2026, 8, 10, 12, 0, 5).unwrap();
        assert_eq!(isoformat(whole), "2026-08-10T12:00:05+00:00");
        let fractional = whole + chrono::Duration::microseconds(1234);
        assert_eq!(isoformat(fractional), "2026-08-10T12:00:05.001234+00:00");
    }

    #[test]
    fn series_sort_numerically_within_a_prefix() {
        let mut series = vec!["y2", "x10", "x2", "x1", "y1"];
        series.sort_by_key(|s| series_sort_key(s));
        assert_eq!(series, vec!["x1", "x2", "x10", "y1", "y2"]);
    }
}
