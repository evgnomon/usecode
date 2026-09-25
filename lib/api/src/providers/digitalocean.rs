// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! DigitalOcean provider client.

use std::collections::BTreeSet;

use reqwest::Method;
use serde_json::{Map, Value, json};

use super::{
    Call, ProviderError, ProviderResult, api_base, call, field, items, merged, require_field, text,
    truthy,
};
use crate::models::{CloudServer, CloudServerCreateIn};

const PROVIDER: &str = "digitalocean";
const API_BASE: &str = "https://api.digitalocean.com/v2";

// DigitalOcean's own region slugs ("nyc1", "nyc2", "nyc3", "sfo2", ...)
// append a datacenter number to the city code — our own terminology drops
// it, picking the highest-numbered (newest) datacenter as the one canonical
// region per city, the same way Hetzner's "fsn1" collapses to "fsn".
fn base_city(slug: &str) -> String {
    slug.trim_end_matches(|c: char| c.is_ascii_digit())
        .to_string()
}

fn public_ip(addresses: Option<&Value>) -> Option<String> {
    addresses
        .and_then(Value::as_array)?
        .iter()
        .find(|address| address.get("type").and_then(Value::as_str) == Some("public"))
        .and_then(|address| address.get("ip_address"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn to_server(droplet: &Value) -> ProviderResult<CloudServer> {
    let networks = droplet.get("networks");
    let region_slug = droplet
        .get("region")
        .and_then(|region| region.get("slug"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(CloudServer {
        provider: PROVIDER.to_string(),
        id: text(field(PROVIDER, droplet, "id")?),
        name: text(field(PROVIDER, droplet, "name")?),
        status: text(field(PROVIDER, droplet, "status")?),
        server_type: droplet
            .get("size_slug")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        location: base_city(region_slug),
        public_ip4: public_ip(networks.and_then(|n| n.get("v4"))),
        public_ip6: public_ip(networks.and_then(|n| n.get("v6"))),
    })
}

async fn get(token: &str, path: &str, query: &[(&str, &str)]) -> ProviderResult<Value> {
    call(Call {
        provider: PROVIDER,
        token,
        method: Method::GET,
        url: format!("{}{path}", api_base(PROVIDER, API_BASE)),
        query,
        body: None,
        expect: &[200],
    })
    .await
}

pub async fn list_servers(credentials: &Value) -> ProviderResult<Vec<CloudServer>> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    let body = get(&token, "/droplets", &[]).await?;
    items(PROVIDER, &body, "droplets")?
        .iter()
        .map(to_server)
        .collect()
}

pub async fn create_server(
    credentials: &Value,
    spec: &CloudServerCreateIn,
) -> ProviderResult<CloudServer> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    let Some(region) = spec.location.as_deref().filter(|l| !l.is_empty()) else {
        return Err(ProviderError::Api {
            provider: PROVIDER,
            status: 400,
            detail: "location (region) is required".to_string(),
        });
    };
    let mut body = Map::new();
    body.insert("name".into(), json!(spec.name));
    body.insert("size".into(), json!(spec.server_type));
    body.insert("image".into(), json!(spec.image));
    body.insert("region".into(), json!(region));
    if !spec.ssh_keys.is_empty() {
        body.insert("ssh_keys".into(), json!(spec.ssh_keys));
    }
    let response = call(Call {
        provider: PROVIDER,
        token: &token,
        method: Method::POST,
        url: format!("{}/droplets", api_base(PROVIDER, API_BASE)),
        query: &[],
        body: Some(&Value::Object(body)),
        expect: &[202],
    })
    .await?;
    to_server(field(PROVIDER, &response, "droplet")?)
}

pub async fn list_server_types(credentials: &Value) -> ProviderResult<Vec<Value>> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    let body = get(&token, "/sizes", &[]).await?;
    let mut types = Vec::new();
    for size in items(PROVIDER, &body, "sizes")? {
        if size.get("available").is_some() && !truthy(size.get("available")) {
            continue;
        }
        let memory_mb = field(PROVIDER, size, "memory")?
            .as_f64()
            .unwrap_or_default();
        let cities: BTreeSet<String> = size
            .get("regions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|region| base_city(&text(region)))
            .collect();
        types.push(json!({
            "provider_server_type": field(PROVIDER, size, "slug")?,
            "cpu": field(PROVIDER, size, "vcpus")?,
            "memory_gb": memory_mb / 1024.0,
            "disk_gb": field(PROVIDER, size, "disk")?,
            "cities": cities,
        }));
    }
    Ok(types)
}

pub async fn list_locations(credentials: &Value) -> ProviderResult<Vec<Value>> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    let body = get(&token, "/regions", &[]).await?;
    let mut by_city: Vec<(String, String, Value)> = Vec::new(); // (city, slug, entry)
    for region in items(PROVIDER, &body, "regions")? {
        if region.get("available").is_some() && !truthy(region.get("available")) {
            continue;
        }
        let slug = text(field(PROVIDER, region, "slug")?);
        let city = base_city(&slug);
        let entry = merged(
            vec![
                ("code", json!(city)),
                ("provider_location_code", json!(slug)),
            ],
            region,
        );
        match by_city.iter_mut().find(|(c, _, _)| *c == city) {
            Some(existing) if slug > existing.1 => *existing = (city, slug, entry),
            Some(_) => {}
            None => by_city.push((city, slug, entry)),
        }
    }
    Ok(by_city.into_iter().map(|(_, _, entry)| entry).collect())
}

pub async fn list_images(credentials: &Value) -> ProviderResult<Vec<Value>> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    let body = get(
        &token,
        "/images",
        &[("type", "distribution"), ("per_page", "200")],
    )
    .await?;
    Ok(items(PROVIDER, &body, "images")?
        .iter()
        .filter(|image| truthy(image.get("slug")))
        .map(|image| merged(vec![("code", image["slug"].clone())], image))
        .collect())
}

pub async fn delete_server(credentials: &Value, server_id: &str) -> ProviderResult<()> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    // 404 means the droplet is already gone, which is the state this call
    // exists to reach — treat it as success so a delete that is retried (or
    // raced with itself) converges instead of failing forever.
    call(Call {
        provider: PROVIDER,
        token: &token,
        method: Method::DELETE,
        url: format!("{}/droplets/{server_id}", api_base(PROVIDER, API_BASE)),
        query: &[],
        body: None,
        expect: &[204, 404],
    })
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_collapse_to_cities() {
        assert_eq!(base_city("nyc3"), "nyc");
        assert_eq!(base_city("sfo"), "sfo");
    }

    #[test]
    fn droplets_map_from_the_api_shape() {
        let server = to_server(&json!({
            "id": 7, "name": "db", "status": "active", "created_at": "2026-01-01T00:00:00Z",
            "size_slug": "s-1vcpu-1gb", "region": {"slug": "nyc3"},
            "networks": {"v4": [{"type": "private", "ip_address": "10.0.0.1"},
                                {"type": "public", "ip_address": "5.6.7.8"}]},
        }))
        .unwrap();
        assert_eq!(server.id, "7");
        assert_eq!(server.location, "nyc");
        assert_eq!(server.public_ip4.as_deref(), Some("5.6.7.8"));
        assert_eq!(server.public_ip6, None);
    }
}
