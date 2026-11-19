// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc vm firewall`: the provider's own firewall in front of cloud VMs,
//! read only.
//!
//! It is the outer layer of two: the provider drops traffic before it
//! reaches the VM, and the host firewall of `uc net firewall` filters what
//! gets through. Each provider's firewall is read through its REST API
//! and shown the same way: who it applies to, its rules, and what happens
//! to traffic no rule matches.

use super::{Api, Provider, array, find, servers, text};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::collections::HashMap;

pub(super) const HELP: &str = "\
usage: uc vm firewall [NAME] --provider <hetzner|digitalocean|upcloud> [--json]

Shows the provider's own firewall: the rules it applies in front of a VM,
before traffic reaches the host firewall of `uc net firewall`. Read only.

Without NAME it lists every firewall of the account (on UpCloud, where each
server has its own, every server's). With NAME only the ones that apply to
that VM, and whether they are in force.

Options:
  --json   print the provider's own objects instead of the table

On Hetzner and DigitalOcean a firewall lets in only what its rules allow,
and a VM without one is open at the provider. Hetzner lets all outbound
traffic out until a firewall has an outbound rule; DigitalOcean lets out
only what the outbound rules allow. UpCloud's rules run in order, the first
match wins, and a last rule with no conditions is the default. OVHcloud
Public Cloud filters with OpenStack security groups, which its API does not
show, so it is not covered.

Examples:
  uc vm firewall --provider hetzner
  uc vm firewall uc3 --provider hetzner
  uc vm firewall uc5 --provider upcloud --json
";

/// A provider firewall, the same for every provider.
#[derive(Debug)]
struct Firewall {
    name: String,
    id: String,
    /// Whether it is in force: Hetzner's per-server `applied`/`pending`,
    /// DigitalOcean's status, UpCloud's `on`/`off`.
    state: Option<String>,
    /// What it applies to: servers by name, tags and label selectors.
    applied_to: Vec<String>,
    /// The ids of the servers it covers, to pick one server's firewalls.
    covers: Vec<String>,
    rules: Vec<Rule>,
    /// What happens to traffic no rule matches.
    default_in: String,
    default_out: String,
    /// The provider's own objects, for `--json`.
    raw: Value,
}

#[derive(Debug, PartialEq)]
struct Rule {
    /// `in` or `out`.
    direction: String,
    protocol: String,
    port: String,
    /// Sources of an inbound rule, destinations of an outbound one.
    addresses: Vec<String>,
    /// `allow` or `drop`.
    action: String,
    note: String,
}

pub(super) fn run(api: &Api, args: &[String]) -> Result<()> {
    let mut name = None;
    let mut as_json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => as_json = true,
            other if other.starts_with('-') => bail!("unknown option: {other}"),
            other if name.is_none() => name = Some(other),
            other => bail!("unexpected argument: {other}"),
        }
    }
    let server = name.map(|name| find(api, name)).transpose()?;
    let firewalls = match api.provider {
        Provider::Hetzner => hetzner(api, server.as_ref().map(|s| s.id.as_str()))?,
        Provider::DigitalOcean => digitalocean(api)?,
        Provider::UpCloud => {
            let targets = match &server {
                Some(server) => vec![server.clone()],
                None => servers(api)?,
            };
            targets
                .iter()
                .map(|server| upcloud(api, &server.id, &server.name))
                .collect::<Result<_>>()?
        }
        Provider::Ovh => bail!(
            "OVHcloud Public Cloud filters with OpenStack security groups, which its API \
             does not show; uc vm firewall covers hetzner, digitalocean and upcloud"
        ),
    };
    let firewalls: Vec<Firewall> = match &server {
        Some(server) => firewalls
            .into_iter()
            .filter(|fw| fw.covers.contains(&server.id))
            .collect(),
        None => firewalls,
    };
    if as_json {
        let raw: Vec<&Value> = firewalls.iter().map(|fw| &fw.raw).collect();
        println!("{}", serde_json::to_string_pretty(&raw)?);
        return Ok(());
    }
    if firewalls.is_empty() {
        let provider = api.provider.name();
        match &server {
            Some(server) => println!(
                "No {provider} firewall applies to {name}: the provider lets every port \
                 through, and only the host firewall filters it (uc net firewall status {name}).",
                name = server.name
            ),
            None => println!("No {provider} firewalls."),
        }
        return Ok(());
    }
    for (i, fw) in firewalls.iter().enumerate() {
        if i > 0 {
            println!();
        }
        print!("{}", render(fw));
    }
    Ok(())
}

/// One firewall as text: a header, its rules and its defaults.
fn render(fw: &Firewall) -> String {
    let mut out = format!("{} (id {})", fw.name, fw.id);
    if let Some(state) = &fw.state {
        out.push_str(&format!(" [{state}]"));
    }
    out.push('\n');
    if !fw.applied_to.is_empty() {
        out.push_str(&format!("  applied to: {}\n", fw.applied_to.join(", ")));
    }
    if fw.rules.is_empty() {
        out.push_str("  no rules\n");
    } else {
        out.push_str(&format!(
            "  {:<4} {:<6} {:<12} {:<34} {:<6} NOTE\n",
            "DIR", "PROTO", "PORT", "FROM/TO", "ACTION"
        ));
        for rule in &fw.rules {
            let line = format!(
                "  {:<4} {:<6} {:<12} {:<34} {:<6} {}",
                rule.direction,
                rule.protocol,
                rule.port,
                rule.addresses.join(", "),
                rule.action,
                rule.note
            );
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }
    out.push_str(&format!(
        "  unmatched: in {}, out {}\n",
        fw.default_in, fw.default_out
    ));
    out
}

/// `GET /firewalls`; with `server`, the state of each on that server from
/// its `public_net.firewalls`.
fn hetzner(api: &Api, server: Option<&str>) -> Result<Vec<Firewall>> {
    let names: HashMap<String, String> =
        servers(api)?.into_iter().map(|s| (s.id, s.name)).collect();
    let states: HashMap<String, String> = match server {
        Some(id) => array(
            &api.call("GET", &format!("/servers/{id}"), None)?["server"]["public_net"]["firewalls"],
        )
        .iter()
        .map(|fw| (text(&fw["id"]), text(&fw["status"])))
        .collect(),
        None => HashMap::new(),
    };
    Ok(api
        .list("/firewalls", "firewalls")?
        .iter()
        .map(|fw| {
            let mut fw = hetzner_firewall(fw, &names);
            fw.state = states.get(&fw.id).cloned();
            fw
        })
        .collect())
}

fn hetzner_firewall(fw: &Value, names: &HashMap<String, String>) -> Firewall {
    let name_of = |id: &Value| {
        let id = text(id);
        names.get(&id).cloned().unwrap_or(id)
    };
    let mut applied_to = Vec::new();
    let mut covers = Vec::new();
    for target in array(&fw["applied_to"]) {
        match target["type"].as_str() {
            Some("server") => {
                covers.push(text(&target["server"]["id"]));
                applied_to.push(name_of(&target["server"]["id"]));
            }
            Some("label_selector") => {
                let matched: Vec<&Value> = array(&target["applied_to_resources"])
                    .iter()
                    .filter(|r| r["type"] == "server")
                    .map(|r| &r["server"]["id"])
                    .collect();
                covers.extend(matched.iter().map(|id| text(id)));
                let servers: Vec<String> = matched.into_iter().map(name_of).collect();
                applied_to.push(format!(
                    "label {} ({})",
                    text(&target["label_selector"]["selector"]),
                    if servers.is_empty() {
                        "no servers".to_string()
                    } else {
                        servers.join(", ")
                    }
                ));
            }
            _ => applied_to.push(text(&target["type"])),
        }
    }
    let rules: Vec<Rule> = array(&fw["rules"])
        .iter()
        .map(|rule| {
            let direction = text(&rule["direction"]);
            let addresses = if direction == "out" {
                &rule["destination_ips"]
            } else {
                &rule["source_ips"]
            };
            Rule {
                port: match &rule["port"] {
                    Value::Null => "-".to_string(),
                    port => text(port),
                },
                direction,
                protocol: text(&rule["protocol"]),
                addresses: array(addresses).iter().map(text).collect(),
                action: "allow".to_string(),
                note: text(&rule["description"]),
            }
        })
        .collect();
    let has_out = rules.iter().any(|rule| rule.direction == "out");
    Firewall {
        name: text(&fw["name"]),
        id: text(&fw["id"]),
        state: None,
        applied_to,
        covers,
        rules,
        default_in: "drop".to_string(),
        default_out: if has_out { "drop" } else { "allow" }.to_string(),
        raw: fw.clone(),
    }
}

/// `GET /firewalls`, with the droplets for names and tags.
fn digitalocean(api: &Api) -> Result<Vec<Firewall>> {
    let droplets = api.list("/droplets", "droplets")?;
    Ok(api
        .list("/firewalls", "firewalls")?
        .iter()
        .map(|fw| digitalocean_firewall(fw, &droplets))
        .collect())
}

fn digitalocean_firewall(fw: &Value, droplets: &[Value]) -> Firewall {
    let name_of = |id: &Value| {
        droplets
            .iter()
            .find(|d| d["id"] == *id)
            .map_or_else(|| text(id), |d| text(&d["name"]))
    };
    let tags: Vec<&Value> = array(&fw["tags"]).iter().collect();
    let mut applied_to: Vec<String> = array(&fw["droplet_ids"]).iter().map(name_of).collect();
    applied_to.extend(tags.iter().map(|tag| format!("tag {}", text(tag))));
    let covers = droplets
        .iter()
        .filter(|d| {
            array(&fw["droplet_ids"]).contains(&d["id"])
                || array(&d["tags"]).iter().any(|tag| tags.contains(&tag))
        })
        .map(|d| text(&d["id"]))
        .collect();
    let peers = |ends: &Value| {
        let mut out: Vec<String> = array(&ends["addresses"]).iter().map(text).collect();
        out.extend(array(&ends["droplet_ids"]).iter().map(name_of));
        out.extend(
            array(&ends["tags"])
                .iter()
                .map(|t| format!("tag {}", text(t))),
        );
        out.extend(
            array(&ends["load_balancer_uids"])
                .iter()
                .map(|lb| format!("lb {}", text(lb))),
        );
        out.extend(
            array(&ends["kubernetes_ids"])
                .iter()
                .map(|k| format!("k8s {}", text(k))),
        );
        out
    };
    let rule = |direction: &str, rule: &Value, ends: &str| Rule {
        direction: direction.to_string(),
        protocol: text(&rule["protocol"]),
        port: match text(&rule["ports"]).as_str() {
            "" if rule["protocol"] == "icmp" => "-".to_string(),
            "" | "0" | "all" => "all".to_string(),
            ports => ports.to_string(),
        },
        addresses: peers(&rule[ends]),
        action: "allow".to_string(),
        note: String::new(),
    };
    let mut rules: Vec<Rule> = array(&fw["inbound_rules"])
        .iter()
        .map(|r| rule("in", r, "sources"))
        .collect();
    rules.extend(
        array(&fw["outbound_rules"])
            .iter()
            .map(|r| rule("out", r, "destinations")),
    );
    let pending = array(&fw["pending_changes"]).len();
    let mut state = text(&fw["status"]);
    if pending > 0 {
        state.push_str(&format!(", {pending} changes pending"));
    }
    Firewall {
        name: text(&fw["name"]),
        id: text(&fw["id"]),
        state: Some(state).filter(|s| !s.is_empty()),
        applied_to,
        covers,
        rules,
        default_in: "drop".to_string(),
        default_out: "drop".to_string(),
        raw: fw.clone(),
    }
}

/// UpCloud has no firewall objects: each server has its own rules, at
/// `GET /server/<uuid>/firewall_rule`, in force while the server's
/// `firewall` is `on`.
fn upcloud(api: &Api, id: &str, name: &str) -> Result<Firewall> {
    let server = api.call("GET", &format!("/server/{id}"), None)?;
    let rules = api.call("GET", &format!("/server/{id}/firewall_rule"), None)?;
    Ok(upcloud_firewall(
        id,
        name,
        &server["server"]["firewall"],
        &rules,
    ))
}

fn upcloud_firewall(id: &str, name: &str, enabled: &Value, rules: &Value) -> Firewall {
    let list = array(&rules["firewall_rules"]["firewall_rule"]);
    let range = |start: &Value, end: &Value| {
        let (start, end) = (text(start), text(end));
        match (start.as_str(), end.as_str()) {
            ("", "") => None,
            (s, e) if s == e || e.is_empty() => Some(s.to_string()),
            (s, e) => Some(format!("{s}-{e}")),
        }
    };
    let parsed: Vec<Rule> = list
        .iter()
        .map(|rule| {
            let direction = text(&rule["direction"]);
            let (start, end) = if direction == "out" {
                ("destination_address_start", "destination_address_end")
            } else {
                ("source_address_start", "source_address_end")
            };
            let family = text(&rule["family"]);
            let address = range(&rule[start], &rule[end]).unwrap_or_else(|| {
                if family.is_empty() {
                    "any".to_string()
                } else {
                    format!("any {family}")
                }
            });
            let icmp = text(&rule["icmp_type"]);
            Rule {
                direction,
                protocol: match text(&rule["protocol"]).as_str() {
                    "" => "any".to_string(),
                    p => p.to_string(),
                },
                port: if !icmp.is_empty() {
                    format!("type {icmp}")
                } else {
                    range(
                        &rule["destination_port_start"],
                        &rule["destination_port_end"],
                    )
                    .unwrap_or_else(|| "any".to_string())
                },
                addresses: vec![address],
                action: match text(&rule["action"]).as_str() {
                    "accept" => "allow".to_string(),
                    a => a.to_string(),
                },
                note: text(&rule["comment"]),
            }
        })
        .collect();
    // The default of a direction is its last rule when that rule matches
    // everything; without one, unmatched traffic passes.
    let default = |direction: &str| {
        list.iter()
            .rev()
            .find(|r| r["direction"] == direction)
            .filter(|r| {
                [
                    "protocol",
                    "family",
                    "source_address_start",
                    "destination_address_start",
                    "source_port_start",
                    "destination_port_start",
                    "icmp_type",
                ]
                .iter()
                .all(|key| text(&r[*key]).is_empty())
            })
            .map_or("allow".to_string(), |r| match text(&r["action"]).as_str() {
                "accept" => "allow".to_string(),
                a => a.to_string(),
            })
    };
    let on = text(enabled);
    Firewall {
        name: name.to_string(),
        id: id.to_string(),
        state: Some(match on.as_str() {
            "off" => "off: the rules are not in force".to_string(),
            "" => "unknown".to_string(),
            other => other.to_string(),
        }),
        applied_to: Vec::new(),
        covers: vec![id.to_string()],
        rules: parsed,
        default_in: default("in"),
        default_out: default("out"),
        raw: json!({
            "server": id,
            "title": name,
            "firewall": enabled,
            "firewall_rules": rules["firewall_rules"],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hetzner_firewalls_name_their_servers_and_selectors() {
        let names = HashMap::from([
            ("1".to_string(), "uc3".to_string()),
            ("2".to_string(), "uc4".to_string()),
        ]);
        let fw = hetzner_firewall(
            &json!({
                "id": 9, "name": "web",
                "applied_to": [
                    {"type": "server", "server": {"id": 1}},
                    {"type": "label_selector", "label_selector": {"selector": "env=prod"},
                     "applied_to_resources": [{"type": "server", "server": {"id": 2}}]}
                ],
                "rules": [
                    {"direction": "in", "protocol": "tcp", "port": "22",
                     "source_ips": ["0.0.0.0/0", "::/0"], "destination_ips": [],
                     "description": "ssh"},
                    {"direction": "in", "protocol": "icmp", "port": null,
                     "source_ips": ["0.0.0.0/0"], "destination_ips": []}
                ]
            }),
            &names,
        );
        assert_eq!(fw.applied_to, ["uc3", "label env=prod (uc4)"]);
        assert_eq!(fw.covers, ["1", "2"]);
        assert_eq!(fw.rules[0].addresses, ["0.0.0.0/0", "::/0"]);
        assert_eq!(fw.rules[0].note, "ssh");
        assert_eq!(fw.rules[1].port, "-");
        assert_eq!(
            (fw.default_in.as_str(), fw.default_out.as_str()),
            ("drop", "allow")
        );
    }

    #[test]
    fn digitalocean_firewalls_cover_droplets_by_id_and_tag() {
        let droplets = [
            json!({"id": 1, "name": "uc3", "tags": []}),
            json!({"id": 2, "name": "uc4", "tags": ["web"]}),
            json!({"id": 3, "name": "uc5", "tags": ["db"]}),
        ];
        let fw = digitalocean_firewall(
            &json!({
                "id": "f1", "name": "web", "status": "succeeded",
                "droplet_ids": [1], "tags": ["web"],
                "inbound_rules": [{"protocol": "tcp", "ports": "443",
                                   "sources": {"addresses": ["0.0.0.0/0"], "droplet_ids": [3]}}],
                "outbound_rules": [{"protocol": "udp", "ports": "0",
                                    "destinations": {"addresses": ["0.0.0.0/0", "::/0"]}}]
            }),
            &droplets,
        );
        assert_eq!(fw.applied_to, ["uc3", "tag web"]);
        assert_eq!(fw.covers, ["1", "2"]);
        assert_eq!(fw.rules[0].addresses, ["0.0.0.0/0", "uc5"]);
        assert_eq!(fw.rules[1].port, "all");
        assert_eq!(fw.state.as_deref(), Some("succeeded"));
        assert_eq!(fw.default_out, "drop");
    }

    #[test]
    fn upcloud_rules_run_in_order_down_to_the_default() {
        let fw = upcloud_firewall(
            "u1",
            "uc5",
            &json!("on"),
            &json!({"firewall_rules": {"firewall_rule": [
                {"action": "accept", "direction": "in", "family": "IPv4", "protocol": "tcp",
                 "destination_port_start": "22", "destination_port_end": "22",
                 "source_address_start": "192.168.1.1", "source_address_end": "192.168.1.255",
                 "comment": "ssh"},
                {"action": "accept", "direction": "in", "family": "IPv4", "protocol": "icmp",
                 "icmp_type": "8"},
                {"action": "drop", "direction": "in", "family": "", "protocol": ""}
            ]}}),
        );
        assert_eq!(fw.rules[0].addresses, ["192.168.1.1-192.168.1.255"]);
        assert_eq!(fw.rules[0].port, "22");
        assert_eq!(fw.rules[1].port, "type 8");
        assert_eq!(fw.rules[2].protocol, "any");
        assert_eq!(fw.rules[2].addresses, ["any"]);
        assert_eq!(
            (fw.default_in.as_str(), fw.default_out.as_str()),
            ("drop", "allow")
        );
        assert_eq!(fw.state.as_deref(), Some("on"));
    }

    #[test]
    fn a_firewall_renders_as_a_table() {
        let fw = upcloud_firewall("u1", "uc5", &json!("off"), &json!({}));
        assert_eq!(
            render(&fw),
            "uc5 (id u1) [off: the rules are not in force]\n  no rules\n  \
             unmatched: in allow, out allow\n"
        );
    }
}
