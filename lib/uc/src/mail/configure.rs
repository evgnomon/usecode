// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc mail configure`: everything around the mail server that decides
//! whether its mail arrives and is trusted, kept in shape from this machine.
//!
//! * DNS for the mail name and every domain Stalwart has (see
//!   [`records`]), on Cloudflare when the zone's NS records say it's
//!   there and `CLOUDFLARE_API_TOKEN` can edit it, on the cluster's own
//!   name servers (`uc dns`) when they have the zone, as instructions
//!   otherwise. Only the records mail needs are touched: other TXT records,
//!   the domain's own A records and DKIM selectors someone else made stay.
//! * Reverse DNS of the workers Stalwart can run on, on Hetzner Cloud, as
//!   the mail name: receivers check that the sending address and the name
//!   point at each other.
//! * Stalwart itself: DNS and DKIM keys are left to uc, as its own DKIM
//!   rotation would sign with keys nobody published, and its own ACME can't
//!   reach it behind Traefik, so the mail ports get cert-manager's
//!   certificate from `/tls` instead. It also gets a submission listener on
//!   587, which the mail apps that don't do 465 use.
//!
//! Nothing changes before the plan is shown and confirmed.

use super::records::{self, Data, DkimKey, Record, Server};
use super::stalwart::Stalwart;
use crate::dns::{self, pdns::RRset};
use crate::kube::Cluster;
use crate::kube::cloudflare::{self, Cloudflare, client, send};
use crate::kube::remote::{kubectl, kubectl_json};
use crate::kube::roles::hcloud_csi::{LOCATION_LABEL, PROVIDER, PROVIDER_KEY, cluster_token};
use crate::kube::roles::mail::{NAME, NAMESPACE, TLS_SECRET};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, IsTerminal, Write};
use std::net::{IpAddr, Ipv4Addr};

/// Marks the Cloudflare records and PowerDNS RRsets uc made, so it can drop them when they're
/// no longer needed (a retired DKIM key) without touching anyone else's.
const COMMENT: &str = "uc mail configure";
const HETZNER_API: &str = "https://api.hetzner.cloud/v1";
/// Where the role mounts cert-manager's certificate in the pod.
const TLS_CERT: &str = "/tls/tls.crt";
const TLS_KEY: &str = "/tls/tls.key";
/// DKIM keys in these stages are, or are about to be, or were just, used.
const LIVE_STAGES: [&str; 3] = ["active", "pending", "retiring"];

pub struct Options {
    pub context: String,
    /// The mail name, `mail_host` in the user config.
    pub host: String,
    pub dmarc_policy: String,
    /// The worker Stalwart is pinned to, `mail_node`.
    pub pinned: Option<String>,
    /// Only these domains; all of Stalwart's when empty.
    pub domains: Vec<String>,
    pub check: bool,
    pub yes: bool,
}

/// One change to make, with what it says in the plan.
enum Change {
    Stalwart {
        what: String,
        kind: &'static str,
        id: String,
        patch: Value,
        restart: bool,
    },
    StalwartCreate {
        what: String,
        kind: &'static str,
        object: Value,
        /// Sets the new object's id as this setting of x:SystemSettings.
        then_default: Option<&'static str>,
    },
    Create {
        zone: String,
        record: Record,
    },
    Delete {
        zone: String,
        /// Cloudflare's id, or the content for PowerDNS.
        id: String,
        name: String,
        kind: String,
        what: String,
    },
    Ptr {
        server: u64,
        node: String,
        ip: Ipv4Addr,
        from: String,
    },
}

impl Change {
    fn line(&self, host: &str) -> String {
        match self {
            Change::Stalwart { what, .. } | Change::StalwartCreate { what, .. } => {
                format!("~ Stalwart: {what}")
            }
            Change::Create { record, .. } => format!("+ {record}"),
            Change::Delete { what, .. } => format!("- {what}"),
            Change::Ptr { node, ip, from, .. } => {
                format!("~ reverse DNS of {ip} ({node}): {from} -> {host}")
            }
        }
    }
}

/// A record a DNS host already has.
struct Existing {
    /// Cloudflare's id, or the content as PowerDNS has it; none for one
    /// only seen in public DNS.
    id: Option<String>,
    name: String,
    kind: String,
    data: Option<Data>,
    content: String,
    proxied: bool,
    comment: String,
}

impl Existing {
    fn from_cloudflare(r: &Value) -> Existing {
        let s = |v: &Value| v.as_str().unwrap_or_default().to_string();
        let kind = s(&r["type"]);
        let content = s(&r["content"]);
        let data = match kind.as_str() {
            "A" => content.parse().ok().map(Data::A),
            "MX" => Some(Data::Mx {
                preference: r["priority"].as_u64().unwrap_or_default() as u16,
                host: host_name(&content),
            }),
            "TXT" => Some(Data::Txt(records::unquoted(&content))),
            "SRV" => Some(Data::Srv {
                priority: r["data"]["priority"].as_u64().unwrap_or_default() as u16,
                weight: r["data"]["weight"].as_u64().unwrap_or_default() as u16,
                port: r["data"]["port"].as_u64().unwrap_or_default() as u16,
                target: host_name(r["data"]["target"].as_str().unwrap_or_default()),
            }),
            _ => None,
        };
        Existing {
            id: r["id"].as_str().map(str::to_string),
            name: host_name(&s(&r["name"])),
            kind,
            data,
            content,
            proxied: r["proxied"] == true,
            comment: s(&r["comment"]),
        }
    }

    /// A record of a public DNS JSON answer.
    fn from_answer(a: &Value) -> Option<Existing> {
        let kind = match a["type"].as_u64()? {
            1 => "A",
            5 => "CNAME",
            15 => "MX",
            16 => "TXT",
            28 => "AAAA",
            33 => "SRV",
            _ => return None,
        };
        Some(Existing::from_text(
            a["name"].as_str()?,
            kind,
            a["data"].as_str()?,
        ))
    }

    /// The records of a PowerDNS RRset.
    fn from_rrset(r: &Value) -> Vec<Existing> {
        let (Some(name), Some(kind)) = (r["name"].as_str(), r["type"].as_str()) else {
            return Vec::new();
        };
        let ours = r["comments"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|c| c["content"] == COMMENT);
        r["records"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["disabled"] != true)
            .filter_map(|c| c["content"].as_str())
            .map(|content| Existing {
                id: Some(content.to_string()),
                comment: if ours { COMMENT.into() } else { String::new() },
                ..Existing::from_text(name, kind, content)
            })
            .collect()
    }

    /// A record from its data in zone file form.
    fn from_text(name: &str, kind: &str, content: &str) -> Existing {
        let content = match kind {
            "CNAME" => host_name(content),
            _ => content.to_string(),
        };
        let words: Vec<&str> = content.split_whitespace().collect();
        let num = |i: usize| {
            words
                .get(i)
                .and_then(|w| w.parse().ok())
                .unwrap_or_default()
        };
        let data = match kind {
            "A" => content.parse().ok().map(Data::A),
            "MX" => Some(Data::Mx {
                preference: num(0),
                host: host_name(words.get(1).copied().unwrap_or_default()),
            }),
            "TXT" => Some(Data::Txt(records::unquoted(&content))),
            "SRV" => Some(Data::Srv {
                priority: num(0),
                weight: num(1),
                port: num(2),
                target: host_name(words.get(3).copied().unwrap_or_default()),
            }),
            _ => None,
        };
        Existing {
            id: None,
            name: host_name(name),
            kind: kind.into(),
            data,
            content,
            proxied: false,
            comment: String::new(),
        }
    }

    fn delete(&self, zone: &str) -> Change {
        Change::Delete {
            zone: zone.to_string(),
            id: self.id.clone().unwrap_or_default(),
            name: self.name.clone(),
            kind: self.kind.clone(),
            what: self.describe(),
        }
    }

    fn describe(&self) -> String {
        let content = match &self.data {
            Some(Data::Txt(text)) => text.clone(),
            _ => self.content.clone(),
        };
        format!("{} {} {content}", self.kind, self.name)
    }
}

fn host_name(name: &str) -> String {
    name.trim().trim_end_matches('.').to_ascii_lowercase()
}

/// Which of a name's records are the mail's to manage. A domain's own
/// A, AAAA and CNAME records (its website) and its other TXT records
/// (site verifications, ...) are not.
fn owned(
    e: &Existing,
    wanted_kinds: &BTreeSet<&str>,
    host: &str,
    domains: &BTreeSet<String>,
) -> bool {
    let shared = e.name == host || domains.contains(&e.name);
    match e.kind.as_str() {
        "TXT" if shared => {
            wanted_kinds.contains("TXT") && records::is_spf(&records::unquoted(&e.content))
        }
        "A" | "AAAA" | "CNAME" if e.name == host => true,
        // A CNAME can't share its name with anything, so one at a name
        // only mail uses goes.
        "CNAME" => !shared,
        kind => wanted_kinds.contains(kind),
    }
}

/// The changes that make `existing` (every record at the names of `want`)
/// hold exactly `want`, among the records mail owns.
fn diff(
    zone: &str,
    want: &[Record],
    existing: &[Existing],
    host: &str,
    domains: &BTreeSet<String>,
) -> (Vec<Change>, Vec<Change>) {
    let mut kinds: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for r in want {
        kinds.entry(&r.name).or_default().insert(r.data.kind());
    }
    let empty = BTreeSet::new();
    let owned: Vec<&Existing> = existing
        .iter()
        .filter(|e| {
            owned(
                e,
                kinds.get(e.name.as_str()).unwrap_or(&empty),
                host,
                domains,
            )
        })
        .collect();
    // Mail must reach the server itself, never Cloudflare's proxy.
    let matches = |e: &Existing, r: &Record| {
        e.name == r.name && e.data.as_ref() == Some(&r.data) && !e.proxied
    };
    let creates = want
        .iter()
        .filter(|r| !owned.iter().any(|e| matches(e, r)))
        .map(|r| Change::Create {
            zone: zone.to_string(),
            record: r.clone(),
        })
        .collect();
    let deletes = owned
        .iter()
        .filter(|e| !want.iter().any(|r| matches(e, r)))
        .map(|e| e.delete(zone))
        .collect();
    (creates, deletes)
}

/// The zone `name` is in and its name servers, from public DNS.
async fn zone_of(name: &str) -> Result<(String, Vec<String>)> {
    let mut candidate = name;
    loop {
        let answer = cloudflare::lookup(candidate, "NS").await?;
        let ns: Vec<String> = answer["Answer"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|a| {
                a["type"] == 2 && host_name(a["name"].as_str().unwrap_or_default()) == candidate
            })
            .filter_map(|a| a["data"].as_str().map(host_name))
            .collect();
        if !ns.is_empty() {
            return Ok((candidate.to_string(), ns));
        }
        candidate = match candidate.split_once('.') {
            Some((_, parent)) if parent.contains('.') => parent,
            _ => bail!("found no zone for {name} in public DNS: is the domain registered?"),
        };
    }
}

/// Where a zone's records are kept.
enum Host {
    Cloudflare {
        cf: Cloudflare,
        id: String,
    },
    /// The cluster's PowerDNS: the zone as one instance has it, which the
    /// plan is made from.
    Pdns(Value),
    /// Anywhere else, or Cloudflare without a token that can edit it: why.
    Manual(String),
}

/// The cluster's name servers, the zones they have (`example.com.`) and
/// the addresses they answer on.
struct PowerDns {
    fleet: dns::Fleet,
    zones: BTreeSet<String>,
    answering: Vec<IpAddr>,
}

impl PowerDns {
    /// None when the cluster has no `dns` role, or none of it is up.
    async fn connect(context: &str) -> Option<PowerDns> {
        let fleet = dns::Fleet::connect(context).await.ok()?;
        let mut zones = BTreeSet::new();
        for p in fleet.up() {
            zones.extend(p.zones().await.ok()?);
        }
        let answering = dns::answering(context).await.unwrap_or_default();
        Some(PowerDns {
            fleet,
            zones,
            answering,
        })
    }

    fn has(&self, zone: &str) -> bool {
        self.zones.contains(&format!("{zone}."))
    }

    /// The longest zone `name` is in.
    fn zone_of(&self, name: &str) -> Option<String> {
        self.zones
            .iter()
            .map(|z| z.trim_end_matches('.'))
            .filter(|z| name == *z || name.ends_with(&format!(".{z}")))
            .max_by_key(|z| z.len())
            .map(str::to_string)
    }

    /// Whether every name server in `ns` points at the cluster.
    async fn serves(&self, ns: &[String]) -> Result<bool> {
        for n in ns {
            let answer = cloudflare::lookup(n, "A").await?;
            let ips: Vec<IpAddr> = answer["Answer"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|a| a["type"] == 1)
                .filter_map(|a| a["data"].as_str()?.parse().ok())
                .collect();
            if ips.is_empty() || !ips.iter().all(|ip| self.answering.contains(ip)) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// A Hetzner Cloud server: its id, public IPv4 and that address's
/// reverse DNS.
struct HetznerServer {
    id: u64,
    ip: Ipv4Addr,
    ptr: String,
}

struct Hetzner {
    token: String,
}

impl Hetzner {
    async fn server(&self, name: &str) -> Result<Option<HetznerServer>> {
        let reply: Value = send(&format!("Hetzner Cloud server {name}"), || {
            client()
                .get(format!("{HETZNER_API}/servers"))
                .bearer_auth(&self.token)
                .query(&[("name", name)])
        })
        .await?
        .error_for_status()
        .map_err(|e| anyhow!("Hetzner Cloud API: {e}"))?
        .json()
        .await?;
        let s = &reply["servers"][0];
        let ipv4 = &s["public_net"]["ipv4"];
        let (Some(id), Some(ip)) = (
            s["id"].as_u64(),
            ipv4["ip"].as_str().and_then(|ip| ip.parse().ok()),
        ) else {
            return Ok(None);
        };
        Ok(Some(HetznerServer {
            id,
            ip,
            ptr: ipv4["dns_ptr"].as_str().unwrap_or_default().to_string(),
        }))
    }

    async fn set_ptr(&self, server: u64, ip: Ipv4Addr, name: &str) -> Result<()> {
        let body = json!({"ip": ip.to_string(), "dns_ptr": name});
        send("Hetzner Cloud change_dns_ptr", || {
            client()
                .post(format!(
                    "{HETZNER_API}/servers/{server}/actions/change_dns_ptr"
                ))
                .bearer_auth(&self.token)
                .json(&body)
        })
        .await?
        .error_for_status()
        .map_err(|e| anyhow!("setting the reverse DNS of {ip}: {e}"))?;
        Ok(())
    }
}

/// A node's public IPv4 addresses, as the kubelet reports them.
fn public_ipv4(addresses: &[String]) -> Vec<Ipv4Addr> {
    addresses
        .iter()
        .filter_map(|a| a.parse::<Ipv4Addr>().ok())
        .filter(|ip| !(ip.is_private() || ip.is_loopback() || ip.is_link_local()))
        .collect()
}

/// Runs `uc mail configure`; false when something is left for the user.
pub async fn run(opts: Options) -> Result<bool> {
    let host = host_name(&opts.host);
    let context = opts.context.as_str();
    let cluster = {
        let context = context.to_string();
        tokio::task::spawn_blocking(move || Cluster::load(Some(&context))).await??
    };
    let mut notes: Vec<String> = Vec::new();

    // Where the mail ports answer, and where Stalwart can move to: its
    // volume keeps it in its node's location.
    let service = kubectl_json(context, &["-n", NAMESPACE, "get", "service", "mail"])
        .await
        .context("the mail role isn't installed: uc kube configure -t mail")?;
    let answers_on: Vec<Ipv4Addr> = service["status"]["loadBalancer"]["ingress"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| i["ip"].as_str()?.parse().ok())
        .collect();
    let pods = kubectl_json(
        context,
        &["-n", NAMESPACE, "get", "pods", "-l", &format!("app={NAME}")],
    )
    .await?;
    let Some(running_on) = pods["items"][0]["spec"]["nodeName"].as_str() else {
        bail!("Stalwart isn't running: kubectl -n {NAMESPACE} get pods");
    };
    if answers_on.is_empty() {
        bail!("the mail ports answer nowhere yet: kubectl -n {NAMESPACE} describe service mail");
    }
    let node = cluster
        .nodes
        .iter()
        .find(|n| n.name == running_on)
        .ok_or_else(|| anyhow!("Stalwart runs on {running_on}, which kubectl doesn't list"))?;
    if let Some(pinned) = &opts.pinned
        && pinned != running_on
    {
        bail!(
            "Stalwart runs on {running_on}, not on {pinned} (mail_node): run \
             `uc kube configure -t mail` to move it there first"
        );
    }
    // Pinned, it sends from its node only; else from any worker it can
    // move to.
    let workers: Vec<_> = cluster
        .nodes
        .iter()
        .filter(|n| {
            n.name == running_on
                || (opts.pinned.is_none()
                    && !n.control_plane
                    && n.label(PROVIDER_KEY) == Some(PROVIDER)
                    && n.label(LOCATION_LABEL).is_some()
                    && n.label(LOCATION_LABEL) == node.label(LOCATION_LABEL))
        })
        .collect();

    let hetzner = match cluster_token(&cluster).map_or_else(crate::vm::hetzner_token, Ok) {
        Ok(token) => Some(Hetzner { token }),
        Err(err) => {
            notes.push(format!(
                "no Hetzner Cloud token ({err:#}), so set the reverse DNS of the workers' \
                 IPv4 addresses below to {host} in the Hetzner console"
            ));
            None
        }
    };
    // Reverse DNS goes last, once the name points at the address.
    let mut ptrs: Vec<Change> = Vec::new();
    let mut sends_from = BTreeSet::new();
    for w in &workers {
        let server = match &hetzner {
            Some(h) => h.server(&w.name).await?,
            None => None,
        };
        match server {
            Some(s) => {
                sends_from.insert(s.ip);
                if host_name(&s.ptr) != host {
                    ptrs.push(Change::Ptr {
                        server: s.id,
                        node: w.name.clone(),
                        ip: s.ip,
                        from: if s.ptr.is_empty() {
                            "none".into()
                        } else {
                            s.ptr
                        },
                    });
                }
            }
            None => {
                let ips = public_ipv4(&w.addresses);
                if hetzner.is_some() {
                    notes.push(format!(
                        "{} is not a server of the Hetzner Cloud project; set the reverse DNS \
                         of {} to {host} where it's hosted",
                        w.name,
                        ips.iter()
                            .map(Ipv4Addr::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                sends_from.extend(ips);
            }
        }
    }
    let node_ips: Vec<Ipv4Addr> = cluster
        .nodes
        .iter()
        .flat_map(|n| public_ipv4(&n.addresses))
        .collect();

    let mut changes: Vec<Change> = Vec::new();

    // Stalwart: its domains, their DKIM keys and the settings that
    // decide whether it's trusted.
    let stalwart = Stalwart::admin(context).await?;
    let all_domains = stalwart.get("x:Domain").await?;
    for d in &opts.domains {
        if !all_domains
            .iter()
            .any(|x| x["name"] == host_name(d).as_str())
        {
            bail!("Stalwart has no domain {d}: uc mail domain add {d}");
        }
    }
    let domains: Vec<&Value> = all_domains
        .iter()
        .filter(|d| d["isEnabled"] != false)
        .filter(|d| {
            opts.domains.is_empty()
                || opts
                    .domains
                    .iter()
                    .any(|x| host_name(x) == d["name"].as_str().unwrap_or_default())
        })
        .collect();
    if domains.is_empty() {
        bail!("Stalwart has no domain yet: uc mail domain add example.com, then run this again");
    }
    let keys = stalwart.get("x:DkimSignature").await?;
    let settings = stalwart.singleton("x:SystemSettings").await?;
    let certificates = stalwart.get("x:Certificate").await?;
    let listeners = stalwart.get("x:NetworkListener").await?;

    for d in &domains {
        let name = d["name"].as_str().unwrap_or_default();
        let id = d["id"].as_str().unwrap_or_default();
        for (field, why) in [
            ("dnsManagement", "uc keeps its DNS"),
            ("dkimManagement", "no rotation to keys that aren't in DNS"),
            (
                "certificateManagement",
                "the mail ports use cert-manager's certificate",
            ),
        ] {
            if d[field]["@type"] != "Manual" {
                changes.push(Change::Stalwart {
                    what: format!("{name}: {field} Manual ({why})"),
                    kind: "x:Domain",
                    id: id.into(),
                    patch: json!({field: {"@type": "Manual"}}),
                    restart: false,
                });
            }
        }
    }
    if settings["defaultHostname"]
        .as_str()
        .map(host_name)
        .as_deref()
        != Some(host.as_str())
    {
        changes.push(Change::Stalwart {
            what: format!(
                "hostname {} -> {host} (what it greets other servers with)",
                settings["defaultHostname"].as_str().unwrap_or("none")
            ),
            kind: "x:SystemSettings",
            id: "singleton".into(),
            patch: json!({"defaultHostname": host}),
            restart: true,
        });
    }
    let tls_ready = kubectl(
        context,
        &["-n", NAMESPACE, "get", "secret", TLS_SECRET, "-o", "name"],
    )
    .await
    .is_ok();
    let mounted = certificates
        .iter()
        .find(|c| c["certificate"]["@type"] == "File" && c["certificate"]["filePath"] == TLS_CERT);
    match (tls_ready, mounted) {
        (false, _) => notes.push(format!(
            "cert-manager has no certificate for {host} yet, so the mail ports keep a \
             self-signed one; once DNS is in place it gets one, run this again then \
             (kubectl -n {NAMESPACE} describe certificate {TLS_SECRET})"
        )),
        (true, Some(c)) if settings["defaultCertificateId"] != c["id"] => {
            changes.push(Change::Stalwart {
                what: format!("serve {TLS_CERT} on the mail ports"),
                kind: "x:SystemSettings",
                id: "singleton".into(),
                patch: json!({"defaultCertificateId": c["id"]}),
                restart: true,
            });
        }
        (true, Some(_)) => {}
        (true, None) => changes.push(Change::StalwartCreate {
            what: format!("serve {TLS_CERT} (cert-manager's) on the mail ports"),
            kind: "x:Certificate",
            object: json!({
                "certificate": {"@type": "File", "filePath": TLS_CERT},
                "privateKey": {"@type": "File", "filePath": TLS_KEY},
            }),
            then_default: Some("defaultCertificateId"),
        }),
    }
    let binds = |l: &Value, port: u16| {
        l["bind"]
            .as_object()
            .is_some_and(|b| b.keys().any(|a| a.ends_with(&format!(":{port}"))))
    };
    if !listeners.iter().any(|l| binds(l, 587)) {
        changes.push(Change::StalwartCreate {
            what: "a submission listener on 587 (STARTTLS)".into(),
            kind: "x:NetworkListener",
            object: json!({
                "name": "submission",
                "bind": {"[::]:587": true},
                "protocol": "smtp",
                "useTls": true,
                "tlsImplicit": false,
            }),
            then_default: None,
        });
    }

    // DNS, zone by zone.
    let server = Server {
        host: host.clone(),
        answers_on,
        sends_from: sends_from.into_iter().collect(),
        dmarc_policy: opts.dmarc_policy.clone(),
    };
    let domain_names: BTreeSet<String> = domains
        .iter()
        .filter_map(|d| d["name"].as_str().map(host_name))
        .collect();
    let mut by_zone: BTreeMap<String, Vec<(String, Option<String>)>> = BTreeMap::new();
    let mut nameservers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // Connected once a zone isn't on Cloudflare: it may be on the cluster.
    let mut powerdns: Option<Option<PowerDns>> = None;
    for (name, domain_id) in std::iter::once((host.clone(), None)).chain(domains.iter().map(|d| {
        (
            host_name(d["name"].as_str().unwrap_or_default()),
            d["id"].as_str().map(str::to_string),
        )
    })) {
        let (zone, ns) = match zone_of(&name).await {
            Ok(found) => found,
            // Not delegated yet, but maybe made with `uc dns zone add`.
            Err(err) => {
                if powerdns.is_none() {
                    powerdns = Some(PowerDns::connect(context).await);
                }
                match powerdns
                    .as_ref()
                    .and_then(Option::as_ref)
                    .and_then(|p| p.zone_of(&name))
                {
                    Some(zone) => (zone, Vec::new()),
                    None => return Err(err),
                }
            }
        };
        nameservers.insert(zone.clone(), ns);
        by_zone.entry(zone).or_default().push((name, domain_id));
    }
    let token = Cloudflare::from_env();
    let mut manual: Vec<(String, String, Vec<Record>, Vec<Change>)> = Vec::new();
    let mut on_powerdns = BTreeSet::new();
    for (zone, names) in &by_zone {
        let ns = &nameservers[zone];
        let on_cloudflare = !ns.is_empty() && ns.iter().all(|n| n.ends_with(".ns.cloudflare.com"));
        if !on_cloudflare && powerdns.is_none() {
            powerdns = Some(PowerDns::connect(context).await);
        }
        let pdns = powerdns
            .as_ref()
            .and_then(Option::as_ref)
            .filter(|p| p.has(zone));
        let dns_host = if let Some(p) = pdns {
            if ns.is_empty() {
                notes.push(format!(
                    "{zone} is on the cluster's name servers but not delegated to them yet: \
                     its registrar wants them as its name servers (`uc dns zone ls`)"
                ));
            } else if !p.serves(ns).await? {
                notes.push(format!(
                    "{zone} is on the cluster's name servers, but its name servers are {}: \
                     public DNS sees the records once they're the cluster's",
                    ns.join(", ")
                ));
            }
            // The plan is made from the first instance that has it.
            let mut found = None;
            for instance in p.fleet.up() {
                found = instance.zone(&format!("{zone}.")).await?;
                if found.is_some() {
                    break;
                }
            }
            match found {
                Some(z) => {
                    on_powerdns.insert(zone.clone());
                    Host::Pdns(z)
                }
                None => Host::Manual("the cluster's name servers lost it".into()),
            }
        } else if !on_cloudflare {
            Host::Manual(format!("its name servers are {}", ns.join(", ")))
        } else {
            match &token {
                Err(err) => Host::Manual(format!("it's on Cloudflare, but {err:#}")),
                Ok(_) => {
                    let cf = Cloudflare::from_env()?;
                    match cf.zone_id(zone).await {
                        Ok(id) => Host::Cloudflare { cf, id },
                        Err(err) => Host::Manual(format!("it's on Cloudflare, but {err:#}")),
                    }
                }
            }
        };
        let mut want = Vec::new();
        let mut existing = Vec::new();
        for (name, domain_id) in names {
            let mut names_here = vec![name.clone()];
            let found = fetch(&dns_host, name).await?;
            let records = match domain_id {
                None => records::host_records(&server),
                Some(id) => {
                    let spf_now = found
                        .iter()
                        .filter(|e| e.name == *name && e.kind == "TXT")
                        .filter_map(|e| match &e.data {
                            Some(Data::Txt(t)) if records::is_spf(t) => Some(t.clone()),
                            _ => None,
                        })
                        .next()
                        .unwrap_or_default();
                    let dkim: Vec<DkimKey> = keys
                        .iter()
                        .filter(|k| k["domainId"] == id.as_str())
                        .filter(|k| LIVE_STAGES.contains(&k["stage"].as_str().unwrap_or_default()))
                        .filter_map(|k| {
                            let algorithm = match k["@type"].as_str()? {
                                "Dkim1RsaSha256" => "rsa",
                                "Dkim1Ed25519Sha256" => "ed25519",
                                _ => return None,
                            };
                            Some(DkimKey {
                                selector: k["selector"].as_str()?.to_string(),
                                algorithm: algorithm.into(),
                                public_key: k["publicKey"].as_str()?.to_string(),
                            })
                        })
                        .collect();
                    if dkim.is_empty() {
                        notes.push(format!(
                            "Stalwart has no DKIM key for {name}, so its mail goes unsigned and \
                             DMARC rejects it: make one at https://{host}/admin"
                        ));
                    }
                    let kept = records::foreign_spf(&spf_now, &node_ips);
                    let records = records::domain_records(&server, name, &dkim, &kept);
                    names_here.extend(records.iter().map(|r| r.name.clone()));
                    records
                }
            };
            names_here.sort();
            names_here.dedup();
            existing.extend(found.into_iter().filter(|e| e.name == *name));
            for n in names_here.iter().filter(|n| *n != name) {
                existing.extend(fetch(&dns_host, n).await?);
            }
            // DKIM records uc made for keys Stalwart no longer has.
            let suffix = format!("._domainkey.{name}");
            let dkim_now: Vec<Existing> = match &dns_host {
                _ if domain_id.is_none() => Vec::new(),
                Host::Cloudflare { cf, id } => cf
                    .list(id, &[("type", "TXT"), ("name.endswith", &suffix)])
                    .await?
                    .iter()
                    .map(Existing::from_cloudflare)
                    .collect(),
                Host::Pdns(z) => rrsets(z)
                    .filter(|r| r["type"] == "TXT")
                    .flat_map(Existing::from_rrset)
                    .filter(|e| e.name.ends_with(&suffix))
                    .collect(),
                Host::Manual(_) => Vec::new(),
            };
            existing.extend(
                dkim_now
                    .into_iter()
                    .filter(|e| e.comment == COMMENT && !names_here.contains(&e.name)),
            );
            want.extend(records);
        }
        // A stale DKIM record's kind is wanted at no name of its own.
        let stale_dkim: Vec<Change> = existing
            .iter()
            .filter(|e| e.comment == COMMENT && !want.iter().any(|r| r.name == e.name))
            .map(|e| e.delete(zone))
            .collect();
        let (creates, mut deletes) = diff(zone, &want, &existing, &host, &domain_names);
        deletes.extend(stale_dkim);
        match dns_host {
            // One PATCH per instance makes them all at once.
            Host::Pdns(_) => {
                changes.extend(creates);
                changes.extend(deletes);
            }
            Host::Cloudflare { .. } => {
                // A CNAME goes first: nothing can be added next to it.
                let (cnames, rest): (Vec<_>, Vec<_>) = deletes.into_iter().partition(
                    |c| matches!(c, Change::Delete { what, .. } if what.starts_with("CNAME ")),
                );
                changes.extend(cnames);
                changes.extend(creates);
                changes.extend(rest);
            }
            Host::Manual(why) => {
                let mut todo = creates;
                todo.extend(deletes);
                manual.push((zone.clone(), why, want, todo));
            }
        }
    }

    changes.extend(ptrs);

    // The plan.
    println!("Mail server {host} on context {context}, the mail ports on {running_on}");
    if changes.is_empty() {
        println!("Stalwart, DNS and Hetzner are as they should be.");
    } else {
        println!("\nChanges:");
        for c in &changes {
            println!("  {}", c.line(&host));
        }
    }
    for (zone, why, want, todo) in &manual {
        println!("\n{zone} is yours to change ({why}).");
        if todo.is_empty() {
            println!("Its records are all in public DNS already.");
            continue;
        }
        println!("Public DNS needs these changes:");
        for c in todo {
            println!("  {}", c.line(&host));
        }
        println!("Its mail records, in zone file form:");
        for r in want {
            println!("  {}", r.zone_line());
        }
    }
    for note in &notes {
        println!("\nNote: {note}");
    }
    let left = !manual.iter().all(|(_, _, _, todo)| todo.is_empty());
    if changes.is_empty() || opts.check {
        return Ok(!left);
    }
    if !opts.yes {
        confirm(&format!("Make these {} changes?", changes.len()))?;
    }

    // Applying it, in the plan's order.
    let cf = Cloudflare::from_env().ok();
    let mut zone_ids: BTreeMap<String, String> = BTreeMap::new();
    let mut restart = false;
    let mut ok = !left;
    let mut pending: Vec<&Change> = Vec::new();
    for c in &changes {
        println!("{}", c.line(&host));
        if let Change::Create { zone, .. } | Change::Delete { zone, .. } = c
            && on_powerdns.contains(zone)
        {
            pending.push(c);
            continue;
        }
        // Reverse DNS goes once the name points at the address.
        if matches!(c, Change::Ptr { .. }) && !pending.is_empty() {
            let p = powerdns
                .as_ref()
                .and_then(Option::as_ref)
                .expect("a PowerDNS zone has its fleet");
            ok &= apply_powerdns(&p.fleet, &std::mem::take(&mut pending)).await;
        }
        match c {
            Change::Stalwart {
                kind,
                id,
                patch,
                restart: r,
                ..
            } => {
                stalwart.update(kind, id, patch.clone()).await?;
                restart |= r;
            }
            Change::StalwartCreate {
                kind,
                object,
                then_default,
                ..
            } => {
                let id = stalwart.create(kind, object.clone()).await?;
                if let Some(field) = then_default {
                    stalwart
                        .update("x:SystemSettings", "singleton", json!({*field: id}))
                        .await?;
                }
                restart = true;
            }
            Change::Create { zone, record } => {
                let cf = cf.as_ref().expect("a Cloudflare zone has a token");
                let id = zone_id(cf, &mut zone_ids, zone).await?;
                cf.create(&id, &cloudflare_record(record)).await?;
            }
            Change::Delete {
                zone, id: record, ..
            } => {
                let cf = cf.as_ref().expect("a Cloudflare zone has a token");
                let id = zone_id(cf, &mut zone_ids, zone).await?;
                cf.delete(&id, record).await?;
            }
            Change::Ptr { server, ip, .. } => {
                let h = hetzner.as_ref().expect("a reverse DNS change has a token");
                h.set_ptr(*server, *ip, &host).await?;
            }
        }
    }
    if !pending.is_empty() {
        let p = powerdns
            .as_ref()
            .and_then(Option::as_ref)
            .expect("a PowerDNS zone has its fleet");
        ok &= apply_powerdns(&p.fleet, &pending).await;
    }
    drop(stalwart);
    if restart {
        // Stalwart reads its listeners and certificate when it starts.
        println!("restarting Stalwart for its new listener and certificate");
        kubectl(
            context,
            &[
                "-n",
                NAMESPACE,
                "rollout",
                "restart",
                &format!("deployment/{NAME}"),
            ],
        )
        .await?;
        kubectl(
            context,
            &[
                "-n",
                NAMESPACE,
                "rollout",
                "status",
                &format!("deployment/{NAME}"),
                "--timeout=300s",
            ],
        )
        .await?;
    }
    println!("done");
    Ok(ok)
}

/// Makes the record changes of PowerDNS zones on every instance, from what
/// each has: one PATCH per instance and zone.
async fn apply_powerdns(fleet: &dns::Fleet, changes: &[&Change]) -> bool {
    // (name., type) -> (contents to add, contents to take away), by zone.
    type Edits = BTreeMap<(String, String), (Vec<String>, Vec<String>)>;
    let mut by_zone: BTreeMap<String, Edits> = BTreeMap::new();
    for c in changes {
        match c {
            Change::Create { zone, record } => by_zone
                .entry(format!("{zone}."))
                .or_default()
                .entry((format!("{}.", record.name), record.data.kind().to_string()))
                .or_default()
                .0
                .push(record.zone_data()),
            Change::Delete {
                zone,
                id,
                name,
                kind,
                ..
            } => by_zone
                .entry(format!("{zone}."))
                .or_default()
                .entry((format!("{name}."), kind.clone()))
                .or_default()
                .1
                .push(id.clone()),
            _ => {}
        }
    }
    let mut ok = true;
    for (zone, edits) in &by_zone {
        ok &= dns::each(fleet, &format!("the mail records of {zone}"), |p| {
            let (zone, edits) = (zone.clone(), edits.clone());
            async move {
                let records = p
                    .records(&zone)
                    .await?
                    .ok_or_else(|| anyhow!("has no {zone}"))?;
                let (mut set, mut unset) = (BTreeMap::new(), BTreeMap::new());
                for (key, (add, remove)) in edits {
                    let now = records.get(&key);
                    let after = edited(now, &add, &remove, key.1 == "A");
                    if after.as_ref() == now {
                        continue;
                    }
                    if add.is_empty() {
                        unset.insert(key, after);
                    } else {
                        set.insert(key, after);
                    }
                }
                let n = set.len() + unset.len();
                p.patch_commented(&zone, &set, Some(COMMENT)).await?;
                p.patch(&zone, &unset).await?;
                Ok(format!("{zone}: {n} RRsets changed"))
            }
        })
        .await;
    }
    ok
}

/// The RRset `now` with `remove` taken away and `add` added; None when
/// nothing is left. An A RRset gets a short TTL, so a move to another
/// worker is followed quickly.
fn edited(now: Option<&RRset>, add: &[String], remove: &[String], short: bool) -> Option<RRset> {
    let mut after = now.cloned();
    if !remove.is_empty() {
        after = dns::updated(after.as_ref(), remove, None, dns::Mode::Remove);
    }
    if !add.is_empty() {
        let ttl = short.then_some(60);
        after = dns::updated(after.as_ref(), add, ttl, dns::Mode::Add);
    }
    after
}

/// The RRsets of a zone from PowerDNS's API.
fn rrsets(zone: &Value) -> impl Iterator<Item = &Value> {
    zone["rrsets"].as_array().into_iter().flatten()
}

async fn zone_id(
    cf: &Cloudflare,
    ids: &mut BTreeMap<String, String>,
    zone: &str,
) -> Result<String> {
    if let Some(id) = ids.get(zone) {
        return Ok(id.clone());
    }
    let id = cf.zone_id(zone).await?;
    ids.insert(zone.to_string(), id.clone());
    Ok(id)
}

/// The records at `name`, from the API or else public DNS.
async fn fetch(dns_host: &Host, name: &str) -> Result<Vec<Existing>> {
    match dns_host {
        Host::Cloudflare { cf, id } => Ok(cf
            .records(id, name)
            .await?
            .iter()
            .map(Existing::from_cloudflare)
            .collect()),
        Host::Pdns(z) => Ok(rrsets(z)
            .filter(|r| r["type"] != "SOA")
            .flat_map(Existing::from_rrset)
            .filter(|e| e.name == name)
            .collect()),
        Host::Manual(_) => {
            let mut found = Vec::new();
            for kind in ["A", "AAAA", "CNAME", "MX", "TXT", "SRV"] {
                let answer = cloudflare::lookup(name, kind).await?;
                found.extend(
                    answer["Answer"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Existing::from_answer)
                        .filter(|e| e.name == name && e.kind == kind),
                );
            }
            Ok(found)
        }
    }
}

fn cloudflare_record(r: &Record) -> Value {
    let mut body = json!({"name": r.name, "type": r.data.kind(), "ttl": 1, "comment": COMMENT});
    match &r.data {
        Data::A(ip) => {
            body["content"] = json!(ip.to_string());
            body["proxied"] = json!(false);
            // Short, so a move to another worker is followed quickly.
            body["ttl"] = json!(60);
        }
        Data::Mx { preference, host } => {
            body["content"] = json!(host);
            body["priority"] = json!(preference);
        }
        Data::Txt(text) => body["content"] = json!(records::quoted(text)),
        Data::Srv {
            priority,
            weight,
            port,
            target,
        } => {
            body["data"] =
                json!({"priority": priority, "weight": weight, "port": port, "target": target});
        }
    }
    body
}

pub(crate) fn confirm(question: &str) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        bail!("not asking without a terminal: pass --yes");
    }
    eprint!("{question} [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    if !matches!(answer.trim(), "y" | "Y" | "yes") {
        bail!("nothing changed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cf(name: &str, kind: &str, content: &str) -> Existing {
        Existing::from_cloudflare(&json!({
            "id": format!("{kind}-{content}"), "name": name, "type": kind,
            "content": content, "priority": 10, "proxied": false,
        }))
    }

    fn server() -> Server {
        Server {
            host: "mail.example.com".into(),
            answers_on: vec![Ipv4Addr::new(192, 0, 2, 1)],
            sends_from: vec![Ipv4Addr::new(192, 0, 2, 1)],
            dmarc_policy: "reject".into(),
        }
    }

    fn lines(changes: &[Change]) -> Vec<String> {
        changes.iter().map(|c| c.line("mail.example.com")).collect()
    }

    #[test]
    fn a_cname_for_the_mail_name_becomes_a_records() {
        let domains = BTreeSet::from(["example.com".to_string()]);
        let want = records::host_records(&server());
        let existing = [cf("mail.example.com", "CNAME", "example.com")];
        let (creates, deletes) = diff(
            "example.com",
            &want,
            &existing,
            "mail.example.com",
            &domains,
        );
        assert_eq!(
            lines(&creates),
            [
                "+ A mail.example.com 192.0.2.1",
                "+ TXT mail.example.com v=spf1 ip4:192.0.2.1 -all"
            ]
        );
        assert_eq!(lines(&deletes), ["- CNAME mail.example.com example.com"]);
    }

    #[test]
    fn leaves_the_domains_website_and_other_txt_records() {
        let domains = BTreeSet::from(["example.com".to_string()]);
        let want = records::domain_records(&server(), "example.com", &[], &[]);
        let existing = [
            cf("example.com", "A", "192.0.2.50"),
            cf("example.com", "CNAME", "example.pages.dev"),
            cf("example.com", "TXT", "\"google-site-verification=abc\""),
            cf(
                "example.com",
                "TXT",
                "\"v=spf1 include:_spf.google.com ~all\"",
            ),
            cf("example.com", "MX", "aspmx.l.google.com"),
        ];
        let (_, deletes) = diff(
            "example.com",
            &want,
            &existing,
            "mail.example.com",
            &domains,
        );
        assert_eq!(
            lines(&deletes),
            [
                "- TXT example.com v=spf1 include:_spf.google.com ~all",
                "- MX example.com aspmx.l.google.com"
            ]
        );
    }

    #[test]
    fn records_already_right_are_left_alone() {
        let domains = BTreeSet::from(["example.com".to_string()]);
        let want = records::host_records(&server());
        let existing = [
            cf("mail.example.com", "A", "192.0.2.1"),
            cf("mail.example.com", "TXT", "\"v=spf1 ip4:192.0.2.1 -all\""),
        ];
        let (creates, deletes) = diff(
            "example.com",
            &want,
            &existing,
            "mail.example.com",
            &domains,
        );
        assert!(creates.is_empty() && deletes.is_empty());
    }

    #[test]
    fn a_proxied_mail_name_is_replaced() {
        let domains = BTreeSet::new();
        let want = records::host_records(&server());
        let mut a = cf("mail.example.com", "A", "192.0.2.1");
        a.proxied = true;
        let (creates, deletes) = diff("example.com", &want, &[a], "mail.example.com", &domains);
        assert_eq!(lines(&creates)[0], "+ A mail.example.com 192.0.2.1");
        assert_eq!(lines(&deletes), ["- A mail.example.com 192.0.2.1"]);
    }

    #[test]
    fn reads_powerdns_rrsets_and_whose_they_are() {
        let mx = Existing::from_rrset(&json!({
            "name": "example.com.", "type": "MX", "ttl": 3600,
            "records": [
                {"content": "10 mail.example.com.", "disabled": false},
                {"content": "20 old.example.com.", "disabled": true},
            ],
            "comments": [],
        }));
        assert_eq!(mx.len(), 1);
        assert_eq!(mx[0].id.as_deref(), Some("10 mail.example.com."));
        assert_eq!(mx[0].comment, "");
        assert_eq!(
            mx[0].data,
            Some(Data::Mx {
                preference: 10,
                host: "mail.example.com".into()
            })
        );
        let dkim = Existing::from_rrset(&json!({
            "name": "v1._domainkey.example.com.", "type": "TXT", "ttl": 3600,
            "records": [{"content": "\"v=DKIM1; k=rsa; \" \"p=MIIB\"", "disabled": false}],
            "comments": [{"content": COMMENT, "account": "uc"}],
        }));
        assert_eq!(dkim[0].name, "v1._domainkey.example.com");
        assert_eq!(dkim[0].comment, COMMENT);
        assert_eq!(
            dkim[0].data,
            Some(Data::Txt("v=DKIM1; k=rsa; p=MIIB".into()))
        );
    }

    #[test]
    fn a_powerdns_zone_gets_the_records_it_lacks() {
        let domains = BTreeSet::from(["example.com".to_string()]);
        let want = records::host_records(&server());
        let existing = Existing::from_rrset(&json!({
            "name": "mail.example.com.", "type": "A", "ttl": 60,
            "records": [{"content": "192.0.2.9", "disabled": false}],
        }));
        let (creates, deletes) = diff(
            "example.com",
            &want,
            &existing,
            "mail.example.com",
            &domains,
        );
        assert_eq!(
            lines(&creates),
            [
                "+ A mail.example.com 192.0.2.1",
                "+ TXT mail.example.com v=spf1 ip4:192.0.2.1 -all"
            ]
        );
        match &deletes[..] {
            [Change::Delete { id, name, kind, .. }] => {
                assert_eq!(
                    (id.as_str(), name.as_str(), kind.as_str()),
                    ("192.0.2.9", "mail.example.com", "A")
                );
            }
            _ => panic!("one delete, not {}", deletes.len()),
        }
    }

    #[test]
    fn an_rrset_keeps_what_mail_doesnt_own() {
        let now = RRset {
            ttl: 300,
            records: vec!["\"site-verification=abc\"".into(), "\"v=spf1 -all\"".into()],
        };
        let after = edited(
            Some(&now),
            &["\"v=spf1 ip4:192.0.2.1 -all\"".into()],
            &["\"v=spf1 -all\"".into()],
            false,
        )
        .unwrap();
        assert_eq!(after.ttl, 300);
        assert_eq!(
            after.records,
            ["\"site-verification=abc\"", "\"v=spf1 ip4:192.0.2.1 -all\""]
        );
        let a = RRset {
            ttl: 3600,
            records: vec!["192.0.2.9".into()],
        };
        assert_eq!(edited(Some(&a), &[], &["192.0.2.9".into()], true), None);
        assert_eq!(
            edited(Some(&a), &["192.0.2.1".into()], &["192.0.2.9".into()], true)
                .unwrap()
                .ttl,
            60
        );
    }

    #[test]
    fn reads_public_dns_answers() {
        let mx = Existing::from_answer(&json!(
            {"name": "example.com.", "type": 15, "data": "10 mail.example.com."}
        ))
        .unwrap();
        assert_eq!(
            mx.data,
            Some(Data::Mx {
                preference: 10,
                host: "mail.example.com".into()
            })
        );
        let srv = Existing::from_answer(&json!(
            {"name": "_imaps._tcp.example.com.", "type": 33, "data": "0 1 993 mail.example.com."}
        ))
        .unwrap();
        assert_eq!(
            srv.data,
            Some(Data::Srv {
                priority: 0,
                weight: 1,
                port: 993,
                target: "mail.example.com".into()
            })
        );
    }
}
