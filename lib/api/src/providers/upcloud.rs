// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! UpCloud provider client.
//!
//! UpCloud nests every list one level deeper than the other providers
//! (`{"servers": {"server": [...]}}`), keeps no ssh keys on the account
//! (they're passed to each server as literal public keys) and refuses to
//! delete a server that is still running.

use std::collections::{BTreeMap, BTreeSet};

use futures::future::try_join_all;
use reqwest::Method;
use serde_json::{Value, json};

use super::{
    Call, ProviderError, ProviderResult, api_base, call, field, merged, require_field, text,
};
use crate::models::{CloudServer, CloudServerCreateIn};

const PROVIDER: &str = "upcloud";
const API_BASE: &str = "https://api.upcloud.com/1.3";

// Disk size for plans that come without storage of their own (the
// CLOUDNATIVE and GPU families report `storage_size: 0`).
const DEFAULT_DISK_GB: u64 = 50;

// UpCloud's zone ids ("fi-hel1", "fi-hel2", "de-fra1") prefix the city with a
// country code and append a datacenter number — our own terminology keeps
// just the city, picking the highest-numbered (newest) zone as the one
// canonical zone per city, the same way DigitalOcean's "nyc3" becomes "nyc".
fn base_city(zone: &str) -> String {
    zone.split_once('-')
        .map_or(zone, |(_, rest)| rest)
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .to_string()
}

/// `{"<outer>": {"<inner>": [...]}}` → the list.
fn nested_items<'a>(body: &'a Value, outer: &str, inner: &str) -> ProviderResult<&'a Vec<Value>> {
    field(PROVIDER, field(PROVIDER, body, outer)?, inner)?
        .as_array()
        .ok_or_else(|| ProviderError::Malformed {
            provider: PROVIDER,
            detail: format!("'{outer}.{inner}' is not a list"),
        })
}

fn public_ip(server: &Value, family: &str) -> Option<String> {
    server
        .get("ip_addresses")?
        .get("ip_address")?
        .as_array()?
        .iter()
        .find(|ip| {
            ip.get("access").and_then(Value::as_str) == Some("public")
                && ip.get("family").and_then(Value::as_str) == Some(family)
        })
        .and_then(|ip| ip.get("address"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn to_server(server: &Value) -> ProviderResult<CloudServer> {
    Ok(CloudServer {
        provider: PROVIDER.to_string(),
        id: text(field(PROVIDER, server, "uuid")?),
        name: text(field(PROVIDER, server, "title")?),
        status: text(field(PROVIDER, server, "state")?),
        server_type: server
            .get("plan")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        location: base_city(
            server
                .get("zone")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        public_ip4: public_ip(server, "IPv4"),
        public_ip6: public_ip(server, "IPv6"),
    })
}

async fn request(
    token: &str,
    method: Method,
    path: &str,
    query: &[(&str, &str)],
    body: Option<&Value>,
    expect: &[u16],
) -> ProviderResult<Value> {
    call(Call {
        provider: PROVIDER,
        token,
        method,
        url: format!("{}{path}", api_base(PROVIDER, API_BASE)),
        query,
        body,
        expect,
    })
    .await
}

async fn get(token: &str, path: &str) -> ProviderResult<Value> {
    request(token, Method::GET, path, &[], None, &[200]).await
}

async fn server_details(token: &str, uuid: &str) -> ProviderResult<CloudServer> {
    let detail = get(token, &format!("/server/{uuid}")).await?;
    to_server(field(PROVIDER, &detail, "server")?)
}

pub async fn list_servers(credentials: &Value) -> ProviderResult<Vec<CloudServer>> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    let body = get(&token, "/server").await?;
    // The list leaves out IP addresses, which only a server's own details
    // carry.
    let details = nested_items(&body, "servers", "server")?
        .iter()
        .map(|server| {
            let token = &token;
            async move { server_details(token, &text(field(PROVIDER, server, "uuid")?)).await }
        });
    try_join_all(details).await
}

fn public_zones(body: &Value) -> ProviderResult<Vec<&Value>> {
    Ok(nested_items(body, "zones", "zone")?
        .iter()
        .filter(|zone| zone.get("public").and_then(Value::as_str) != Some("no"))
        .collect())
}

/// One zone per city: the highest-numbered one.
fn zones_by_city(zones: &[&Value]) -> ProviderResult<BTreeMap<String, (String, Value)>> {
    let mut by_city: BTreeMap<String, (String, Value)> = BTreeMap::new();
    for zone in zones {
        let id = text(field(PROVIDER, zone, "id")?);
        let city = base_city(&id);
        match by_city.get(&city) {
            Some((existing, _)) if *existing >= id => {}
            _ => {
                by_city.insert(city, (id, (*zone).clone()));
            }
        }
    }
    Ok(by_city)
}

pub async fn list_locations(credentials: &Value) -> ProviderResult<Vec<Value>> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    let body = get(&token, "/zone").await?;
    Ok(zones_by_city(&public_zones(&body)?)?
        .into_iter()
        .map(|(city, (id, zone))| {
            merged(
                vec![("code", json!(city)), ("provider_location_code", json!(id))],
                &zone,
            )
        })
        .collect())
}

/// The zone to create in: `location` as-is when it's already a zone id
/// ("fi-hel2"), else the city's canonical zone.
async fn zone(token: &str, location: &str) -> ProviderResult<String> {
    if location.contains('-') {
        return Ok(location.to_string());
    }
    let body = get(token, "/zone").await?;
    zones_by_city(&public_zones(&body)?)?
        .remove(location)
        .map(|(id, _)| id)
        .ok_or_else(|| ProviderError::Api {
            provider: PROVIDER,
            status: 400,
            detail: format!("unknown location '{location}'"),
        })
}

fn disk_gb(plan: &Value) -> u64 {
    plan.get("storage_size")
        .and_then(Value::as_u64)
        .filter(|size| *size > 0)
        .unwrap_or(DEFAULT_DISK_GB)
}

pub async fn list_server_types(credentials: &Value) -> ProviderResult<Vec<Value>> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    let (plans, prices) = tokio::try_join!(get(&token, "/plan"), get(&token, "/price"))?;
    // A plan is offered in a zone when that zone has a price for it.
    let mut cities: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for zone in nested_items(&prices, "prices", "zone")? {
        let city = base_city(&text(field(PROVIDER, zone, "name")?));
        for key in zone.as_object().into_iter().flat_map(|o| o.keys()) {
            if let Some(plan) = key.strip_prefix("server_plan_") {
                cities
                    .entry(plan.to_string())
                    .or_default()
                    .insert(city.clone());
            }
        }
    }
    let mut types = Vec::new();
    for plan in nested_items(&plans, "plans", "plan")? {
        // Legacy plans UpCloud still runs but no longer sells.
        if plan.get("current_offering").and_then(Value::as_str) == Some("no") {
            continue;
        }
        let name = text(field(PROVIDER, plan, "name")?);
        let memory_mb = field(PROVIDER, plan, "memory_amount")?
            .as_f64()
            .unwrap_or_default();
        types.push(json!({
            "provider_server_type": name,
            "cpu": field(PROVIDER, plan, "core_number")?,
            "memory_gb": memory_mb / 1024.0,
            "disk_gb": disk_gb(plan),
            "cities": cities.remove(&name).unwrap_or_default(),
        }));
    }
    Ok(types)
}

/// Our own code for an OS template, built from its title the way the other
/// providers spell theirs: "Debian GNU/Linux 13 (Trixie)" → "debian-13",
/// "Ubuntu Server 24.04 LTS (Noble Numbat)" → "ubuntu-24-04". A "(with ...)"
/// suffix is kept, since that's what tells the variants apart.
fn image_code(title: &str) -> String {
    let mut kept = String::new();
    for (i, part) in title.split('(').enumerate() {
        let (inner, outer) = match i {
            0 => ("", part),
            _ => part.split_once(')').unwrap_or((part, "")),
        };
        if inner.trim_start().to_lowercase().starts_with("with ") {
            kept.push_str(inner);
        }
        kept.push(' ');
        kept.push_str(outer);
    }
    kept.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '/')
        .filter(|word| !matches!(*word, "gnu/linux" | "server" | "lts"))
        .flat_map(|word| word.split('/'))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

async fn templates(token: &str) -> ProviderResult<Vec<Value>> {
    let body = get(token, "/storage/template").await?;
    Ok(nested_items(&body, "storages", "storage")?.clone())
}

pub async fn list_images(credentials: &Value) -> ProviderResult<Vec<Value>> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    templates(&token)
        .await?
        .iter()
        .map(|template| {
            let title = text(field(PROVIDER, template, "title")?);
            Ok(merged(vec![("code", json!(image_code(&title)))], template))
        })
        .collect()
}

/// The template uuid for `image`: one of our image codes, a template's
/// title, or a template uuid.
fn template_uuid(templates: &[Value], image: &str) -> Option<String> {
    templates
        .iter()
        .find(|template| {
            let title = template
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default();
            template.get("uuid").and_then(Value::as_str) == Some(image)
                || title == image
                || image_code(title) == image
        })
        .and_then(|template| template.get("uuid"))
        .map(text)
}

fn bad_request(detail: String) -> ProviderError {
    ProviderError::Api {
        provider: PROVIDER,
        status: 400,
        detail,
    }
}

pub async fn create_server(
    credentials: &Value,
    spec: &CloudServerCreateIn,
) -> ProviderResult<CloudServer> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    let Some(location) = spec.location.as_deref().filter(|l| !l.is_empty()) else {
        return Err(bad_request("location (zone) is required".to_string()));
    };
    // No account key store to look a named key up in: the keys go onto the
    // server as they are.
    let ssh_keys: Vec<&str> = spec
        .ssh_public_key
        .iter()
        .chain(&spec.ssh_keys)
        .map(|key| key.trim())
        .filter(|key| !key.is_empty())
        .collect();
    if ssh_keys.is_empty() {
        return Err(bad_request(
            "an ssh public key is required (ssh_public_key)".to_string(),
        ));
    }
    // A create takes UpCloud several seconds to answer; when the answer is
    // lost (a timeout), the task runs this step again. Names are unique per
    // account (as Hetzner enforces), so a server already called `spec.name`
    // is this create's own.
    let servers = get(&token, "/server").await?;
    let existing = nested_items(&servers, "servers", "server")?
        .iter()
        .find(|server| server.get("title").and_then(Value::as_str) == Some(&spec.name));
    if let Some(server) = existing {
        return server_details(&token, &text(field(PROVIDER, server, "uuid")?)).await;
    }
    let (zone, templates, plans) = tokio::try_join!(
        zone(&token, location),
        templates(&token),
        get(&token, "/plan")
    )?;
    let template = template_uuid(&templates, &spec.image)
        .ok_or_else(|| bad_request(format!("unknown image '{}'", spec.image)))?;
    let plan = nested_items(&plans, "plans", "plan")?
        .iter()
        .find(|plan| plan.get("name").and_then(Value::as_str) == Some(&spec.server_type))
        .ok_or_else(|| bad_request(format!("unknown plan '{}'", spec.server_type)))?;
    let tier = plan
        .get("storage_tier")
        .and_then(Value::as_str)
        .unwrap_or("maxiops");
    let body = json!({"server": {
        "zone": zone,
        "title": spec.name,
        "hostname": spec.name,
        "plan": spec.server_type,
        "metadata": "yes",
        "storage_devices": {"storage_device": [{
            "action": "clone",
            "storage": template,
            "title": format!("{}-disk", spec.name),
            "size": disk_gb(plan),
            "tier": tier,
        }]},
        "networking": {"interfaces": {"interface": [
            {"type": "public", "ip_addresses": {"ip_address": [{"family": "IPv4"}]}},
            {"type": "public", "ip_addresses": {"ip_address": [{"family": "IPv6"}]}},
        ]}},
        "login_user": {
            "username": "root",
            "create_password": "no",
            "ssh_keys": {"ssh_key": ssh_keys},
        },
    }});
    let response = request(&token, Method::POST, "/server", &[], Some(&body), &[202]).await?;
    to_server(field(PROVIDER, &response, "server")?)
}

pub async fn delete_server(credentials: &Value, server_id: &str) -> ProviderResult<()> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    // See the note in digitalocean::delete_server: an absent server is the
    // outcome we want, not an error.
    let detail = request(
        &token,
        Method::GET,
        &format!("/server/{server_id}"),
        &[],
        None,
        &[200, 404],
    )
    .await?;
    let Some(state) = detail.pointer("/server/state").and_then(Value::as_str) else {
        return Ok(());
    };
    // UpCloud only deletes a stopped server, and stopping takes a while.
    // Rather than hold the task up, ask for the stop and fail this attempt;
    // the task retries the delete on its next sweep until the server is down.
    if state != "stopped" {
        if state == "started" {
            request(
                &token,
                Method::POST,
                &format!("/server/{server_id}/stop"),
                &[],
                Some(&json!({"stop_server": {"stop_type": "hard", "timeout": "60"}})),
                &[200, 202],
            )
            .await?;
        }
        return Err(ProviderError::Api {
            provider: PROVIDER,
            status: 409,
            detail: format!("server is {state}, waiting for it to stop before deleting it"),
        });
    }
    request(
        &token,
        Method::DELETE,
        &format!("/server/{server_id}"),
        &[("storages", "1"), ("backups", "delete")],
        None,
        &[204, 404],
    )
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_collapse_to_cities() {
        assert_eq!(base_city("fi-hel2"), "hel");
        assert_eq!(base_city("us-nyc1"), "nyc");
        assert_eq!(base_city("fra"), "fra");
        let zones = [
            json!({"id": "fi-hel1"}),
            json!({"id": "fi-hel2"}),
            json!({"id": "de-fra1"}),
        ];
        let by_city = zones_by_city(&zones.iter().collect::<Vec<_>>()).unwrap();
        assert_eq!(by_city["hel"].0, "fi-hel2");
        assert_eq!(by_city["fra"].0, "de-fra1");
    }

    #[test]
    fn image_codes_read_like_the_other_providers() {
        assert_eq!(image_code("Debian GNU/Linux 13 (Trixie)"), "debian-13");
        assert_eq!(
            image_code("Ubuntu Server 24.04 LTS (Noble Numbat)"),
            "ubuntu-24-04"
        );
        assert_eq!(
            image_code("Ubuntu Server 24.04 LTS (with NVIDIA drivers & CUDA)"),
            "ubuntu-24-04-with-nvidia-drivers-cuda"
        );
        assert_eq!(image_code("UpCloud K8s 1.36"), "upcloud-k8s-1-36");
        assert_eq!(
            image_code("Windows Server 2025 Standard"),
            "windows-2025-standard"
        );
        assert_eq!(image_code("Rocky Linux 10"), "rocky-linux-10");
    }

    #[test]
    fn images_resolve_by_code_title_or_uuid() {
        let templates = vec![json!({"uuid": "u-13", "title": "Debian GNU/Linux 13 (Trixie)"})];
        assert_eq!(
            template_uuid(&templates, "debian-13").as_deref(),
            Some("u-13")
        );
        assert_eq!(template_uuid(&templates, "u-13").as_deref(), Some("u-13"));
        assert_eq!(
            template_uuid(&templates, "Debian GNU/Linux 13 (Trixie)").as_deref(),
            Some("u-13")
        );
        assert_eq!(template_uuid(&templates, "debian-12"), None);
    }

    #[test]
    fn servers_map_from_the_api_shape() {
        let server = to_server(&json!({
            "uuid": "0062-abc", "title": "web", "state": "started",
            "plan": "STARTER-1xCPU-1GB", "zone": "fi-hel2",
            "ip_addresses": {"ip_address": [
                {"access": "utility", "address": "10.0.0.1", "family": "IPv4"},
                {"access": "public", "address": "2a04::1", "family": "IPv6"},
                {"access": "public", "address": "94.237.12.151", "family": "IPv4"},
            ]},
        }))
        .unwrap();
        assert_eq!(server.id, "0062-abc");
        assert_eq!(server.location, "hel");
        assert_eq!(server.public_ip4.as_deref(), Some("94.237.12.151"));
        assert_eq!(server.public_ip6.as_deref(), Some("2a04::1"));
    }
}
