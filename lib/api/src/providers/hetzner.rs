// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Hetzner Cloud provider client.

use std::collections::BTreeSet;

use reqwest::Method;
use serde_json::{Map, Value, json};

use super::{
    Call, ProviderResult, api_base, call, field, items, merged, require_field, text, truthy,
};
use crate::models::{CloudServer, CloudServerCreateIn};

const PROVIDER: &str = "hetzner";
const API_BASE: &str = "https://api.hetzner.cloud/v1";

// Hetzner's own location codes ("fsn1", "nbg1", "hel1") append a trailing
// datacenter number to the city code — our own terminology drops it, since
// each city currently maps to exactly one Hetzner location.
const LOCATIONS: &[(&str, &str)] = &[
    ("ash", "ash"),
    ("fsn", "fsn1"),
    ("hel", "hel1"),
    ("hil", "hil"),
    ("nbg", "nbg1"),
    ("sin", "sin"),
];

fn city(location: &str) -> String {
    LOCATIONS
        .iter()
        .find(|(_, loc)| *loc == location)
        .map_or(location, |(city, _)| city)
        .to_string()
}

fn location(city: &str) -> String {
    LOCATIONS
        .iter()
        .find(|(c, _)| *c == city)
        .map_or(city, |(_, loc)| loc)
        .to_string()
}

fn nested<'a>(value: &'a Value, key: &str) -> &'a Value {
    value
        .get(key)
        .filter(|v| truthy(Some(v)))
        .unwrap_or(&Value::Null)
}

fn opt_str(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn to_server(server: &Value) -> ProviderResult<CloudServer> {
    let public_net = nested(server, "public_net");
    Ok(CloudServer {
        provider: PROVIDER.to_string(),
        id: text(field(PROVIDER, server, "id")?),
        name: text(field(PROVIDER, server, "name")?),
        status: text(field(PROVIDER, server, "status")?),
        server_type: opt_str(nested(server, "server_type"), "name").unwrap_or_default(),
        location: city(&opt_str(nested(server, "location"), "name").unwrap_or_default()),
        public_ip4: opt_str(nested(public_net, "ipv4"), "ip"),
        public_ip6: opt_str(nested(public_net, "ipv6"), "ip"),
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
    let body = get(&token, "/servers", &[]).await?;
    items(PROVIDER, &body, "servers")?
        .iter()
        .map(to_server)
        .collect()
}

pub async fn create_server(
    credentials: &Value,
    spec: &CloudServerCreateIn,
) -> ProviderResult<CloudServer> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    let mut body = Map::new();
    body.insert("name".into(), json!(spec.name));
    body.insert("server_type".into(), json!(spec.server_type));
    body.insert("image".into(), json!(spec.image));
    if let Some(city) = spec.location.as_deref().filter(|l| !l.is_empty()) {
        body.insert("location".into(), json!(location(city)));
    }
    if !spec.ssh_keys.is_empty() {
        body.insert("ssh_keys".into(), json!(spec.ssh_keys));
    }
    let response = call(Call {
        provider: PROVIDER,
        token: &token,
        method: Method::POST,
        url: format!("{}/servers", api_base(PROVIDER, API_BASE)),
        query: &[],
        body: Some(&Value::Object(body)),
        expect: &[201],
    })
    .await?;
    to_server(field(PROVIDER, &response, "server")?)
}

pub async fn list_server_types(credentials: &Value) -> ProviderResult<Vec<Value>> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    let body = get(&token, "/server_types", &[]).await?;
    let mut types = Vec::new();
    for server_type in items(PROVIDER, &body, "server_types")? {
        if truthy(server_type.get("deprecated")) {
            continue;
        }
        let cities: BTreeSet<String> = server_type
            .get("prices")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|price| price.get("location").map(|l| city(&text(l))))
            .collect();
        types.push(json!({
            "provider_server_type": field(PROVIDER, server_type, "name")?,
            "cpu": field(PROVIDER, server_type, "cores")?,
            "memory_gb": field(PROVIDER, server_type, "memory")?,
            "disk_gb": field(PROVIDER, server_type, "disk")?,
            "cities": cities,
        }));
    }
    Ok(types)
}

pub async fn list_locations(credentials: &Value) -> ProviderResult<Vec<Value>> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    let body = get(&token, "/locations", &[]).await?;
    items(PROVIDER, &body, "locations")?
        .iter()
        .map(|location| {
            let name = text(field(PROVIDER, location, "name")?);
            Ok(merged(
                vec![
                    ("code", json!(city(&name))),
                    ("provider_location_code", json!(name)),
                ],
                location,
            ))
        })
        .collect()
}

pub async fn list_images(credentials: &Value) -> ProviderResult<Vec<Value>> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    let body = get(&token, "/images", &[("type", "system")]).await?;
    items(PROVIDER, &body, "images")?
        .iter()
        .map(|image| {
            Ok(merged(
                vec![("code", field(PROVIDER, image, "name")?.clone())],
                image,
            ))
        })
        .collect()
}

pub async fn delete_server(credentials: &Value, server_id: &str) -> ProviderResult<()> {
    let token = require_field(PROVIDER, credentials, "apiKey")?;
    // See the note in digitalocean::delete_server: an absent server is the
    // outcome we want, not an error.
    call(Call {
        provider: PROVIDER,
        token: &token,
        method: Method::DELETE,
        url: format!("{}/servers/{server_id}", api_base(PROVIDER, API_BASE)),
        query: &[],
        body: None,
        expect: &[200, 404],
    })
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cities_drop_the_datacenter_number() {
        assert_eq!(city("fsn1"), "fsn");
        assert_eq!(city("unknown9"), "unknown9");
        assert_eq!(location("fsn"), "fsn1");
        assert_eq!(location("xyz"), "xyz");
    }

    #[test]
    fn servers_map_from_the_api_shape() {
        let server = to_server(&json!({
            "id": 42, "name": "web", "status": "running", "created": "2026-01-01T00:00:00+00:00",
            "server_type": {"name": "cx22"}, "location": {"name": "fsn1"},
            "public_net": {"ipv4": {"ip": "1.2.3.4"}, "ipv6": null},
        }))
        .unwrap();
        assert_eq!(server.id, "42");
        assert_eq!(server.location, "fsn");
        assert_eq!(server.public_ip4.as_deref(), Some("1.2.3.4"));
        assert_eq!(server.public_ip6, None);
    }
}
