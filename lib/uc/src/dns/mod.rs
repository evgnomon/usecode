// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc dns`: looking after the name servers the `dns` role of
//! `uc kube configure` runs. The PowerDNS instances share nothing, so every
//! change here is made on each of them, through [`pdns`]'s API, and
//! [`sync`] puts one back in line with another after it missed some.
//! [`names`] turns what you type into what PowerDNS wants.

pub mod names;
pub mod pdns;

use crate::kube::cluster::CONTROL_PLANE;
use crate::kube::remote::kubectl_json;
use crate::kube::roles::dns::{API_SECRET, APP, NAME_LABEL, NAMESPACE, SERVICE, addresses};
use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use pdns::{Pdns, RRset, Records};
use std::collections::BTreeMap;
use std::io::{BufRead, IsTerminal, Write};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

/// Sends a query for the root's SOA to `ip`:53 and waits for any answer
/// to it: an authoritative server without the root zone refuses, which
/// still shows it's there.
pub async fn probe(ip: IpAddr) -> Result<()> {
    let bind: SocketAddr = if ip.is_ipv4() {
        "0.0.0.0:0".parse()?
    } else {
        "[::]:0".parse()?
    };
    let socket = tokio::net::UdpSocket::bind(bind).await?;
    socket.connect((ip, 53)).await?;
    let mut id = [0u8; 2];
    getrandom::fill(&mut id).map_err(|e| anyhow!("{e}"))?;
    // Header: id, no flags, one question; then the root name, SOA, IN.
    let mut query = id.to_vec();
    query.extend([0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
    query.extend([0, 0, 6, 0, 1]);
    let mut buf = [0u8; 512];
    for _ in 0..3 {
        socket.send(&query).await?;
        match tokio::time::timeout(Duration::from_secs(2), socket.recv(&mut buf)).await {
            Ok(Ok(n)) if n >= 12 && buf[..2] == id => return Ok(()),
            Ok(Err(e)) => return Err(e.into()),
            _ => {}
        }
    }
    bail!("no answer in 6s")
}

/// One PowerDNS instance as the cluster has it.
#[derive(Debug, Clone)]
pub struct Server {
    /// `pdns-<n>`.
    pub name: String,
    /// The node it runs on, if it runs.
    pub node: Option<String>,
}

/// The instances the dns role made, in order.
pub async fn servers(context: &str) -> Result<Vec<Server>> {
    let selector = format!("{NAME_LABEL}={APP}");
    let get = |kind: &'static str| {
        let selector = selector.clone();
        async move { kubectl_json(context, &["-n", NAMESPACE, "get", kind, "-l", &selector]).await }
    };
    let deployments = get("deployments")
        .await
        .context("the dns role isn't installed: uc kube configure -t dns")?;
    let pods = get("pods").await?;
    let mut servers: Vec<Server> = deployments["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| d["metadata"]["name"].as_str())
        .map(|name| Server {
            name: name.to_string(),
            node: pods["items"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|p| {
                    p["metadata"]["labels"]["app"] == name && p["status"]["phase"] == "Running"
                })
                .and_then(|p| p["spec"]["nodeName"].as_str())
                .map(str::to_string),
        })
        .collect();
    if servers.is_empty() {
        bail!("there are no name servers: uc kube configure -t dns");
    }
    // pdns-2 before pdns-10.
    servers.sort_by_key(|s| {
        s.name
            .rsplit('-')
            .next()
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(u32::MAX)
    });
    Ok(servers)
}

/// The addresses port 53 answers on: every node's, behind one service.
pub async fn answering(context: &str) -> Result<Vec<IpAddr>> {
    let service = kubectl_json(context, &["-n", NAMESPACE, "get", "service", SERVICE])
        .await
        .context("the dns role isn't installed: uc kube configure -t dns")?;
    Ok(addresses(&service))
}

/// The workers port 53 answers on, with their address: the nodes the
/// cluster's domain points at, so the natural name servers.
pub async fn workers(context: &str, answering: &[IpAddr]) -> Result<Vec<(String, IpAddr)>> {
    let nodes = kubectl_json(context, &["get", "nodes"]).await?;
    Ok(nodes["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|n| n["metadata"]["labels"].get(CONTROL_PLANE).is_none())
        .filter_map(|n| {
            let ip = n["status"]["addresses"]
                .as_array()?
                .iter()
                .filter_map(|a| a["address"].as_str()?.parse::<IpAddr>().ok())
                .find(|ip| ip.is_ipv4() && answering.contains(ip))?;
            Some((n["metadata"]["name"].as_str()?.to_string(), ip))
        })
        .collect())
}

async fn api_key(context: &str) -> Result<String> {
    let secret = kubectl_json(context, &["-n", NAMESPACE, "get", "secret", API_SECRET]).await?;
    let raw = secret["data"]["api-key"]
        .as_str()
        .ok_or_else(|| anyhow!("{NAMESPACE}/{API_SECRET} has no api-key"))?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(raw)?;
    Ok(String::from_utf8(bytes)?.trim().to_string())
}

/// Every instance, connected where it can be.
pub struct Fleet {
    pub servers: Vec<Server>,
    /// The API of each server, or why it can't be reached.
    pub apis: Vec<Result<Pdns>>,
}

impl Fleet {
    pub async fn connect(context: &str) -> Result<Fleet> {
        let servers = servers(context).await?;
        let key = api_key(context).await?;
        let mut apis = Vec::new();
        for s in &servers {
            apis.push(if s.node.is_none() {
                Err(anyhow!("{} isn't running", s.name))
            } else {
                Pdns::connect(context, &s.name, &key).await
            });
        }
        Ok(Fleet { servers, apis })
    }

    /// The reachable instances.
    pub fn up(&self) -> impl Iterator<Item = &Pdns> {
        self.apis.iter().filter_map(|a| a.as_ref().ok())
    }

    /// Why each unreachable instance is, as lines.
    pub fn down(&self) -> Vec<String> {
        self.apis
            .iter()
            .filter_map(|a| a.as_ref().err())
            .map(|e| format!("{e:#}"))
            .collect()
    }

    fn get(&self, name: &str) -> Result<&Pdns> {
        self.up().find(|p| p.name == name).ok_or_else(|| {
            let known: Vec<_> = self.servers.iter().map(|s| s.name.as_str()).collect();
            anyhow!(
                "{name} is not a running name server; there are: {}",
                known.join(", ")
            )
        })
    }
}

/// Runs `change` on every reachable instance; false when one failed or
/// was down, which is said with what to do about it.
pub(crate) async fn each<'a, F, Fut>(fleet: &'a Fleet, what: &str, change: F) -> bool
where
    F: Fn(&'a Pdns) -> Fut,
    Fut: std::future::Future<Output = Result<String>>,
{
    let mut ok = true;
    for p in fleet.up() {
        match change(p).await {
            Ok(done) => println!("{}: {done}", p.name),
            Err(err) => {
                ok = false;
                eprintln!("{}: {err:#}", p.name);
            }
        }
    }
    for why in fleet.down() {
        ok = false;
        eprintln!("{why}");
    }
    if !ok {
        eprintln!(
            "\nNot every name server has {what}. Once they're all up, `uc dns sync --from \
             <one that has it>` brings the others in line."
        );
    }
    ok
}

/// `uc dns servers`: the instances, where they run and answer.
pub async fn show_servers(context: &str) -> Result<bool> {
    let fleet = Fleet::connect(context).await?;
    let mut ok = true;
    for (s, api) in fleet.servers.iter().zip(&fleet.apis) {
        let zones = match api {
            Ok(p) => match p.zones().await {
                Ok(z) => format!("{} zones", z.len()),
                Err(e) => {
                    ok = false;
                    format!("{e:#}")
                }
            },
            Err(e) => {
                ok = false;
                format!("{e:#}")
            }
        };
        println!(
            "{}  on {}  {zones}",
            s.name,
            s.node.as_deref().unwrap_or("no node")
        );
    }
    let ips = answering(context).await?;
    if ips.is_empty() {
        ok = false;
        println!("\nport 53 answers nowhere yet");
    } else {
        println!("\nport 53, on every node:");
    }
    for ip in ips {
        match probe(ip).await {
            Ok(()) => println!("  {ip}  answers"),
            Err(err) => {
                ok = false;
                println!("  {ip}  {err:#}");
            }
        }
    }
    Ok(ok)
}

/// `uc dns zones`: the zones, and which instances lack one.
pub async fn show_zones(context: &str) -> Result<bool> {
    let fleet = Fleet::connect(context).await?;
    let mut all: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    let mut up = Vec::new();
    for p in fleet.up() {
        for z in p.zones().await? {
            all.entry(z).or_default().push(&p.name);
        }
        up.push(p.name.as_str());
    }
    let mut ok = fleet.down().is_empty();
    for (zone, on) in &all {
        if on.len() == up.len() {
            println!("{zone}");
        } else {
            ok = false;
            println!("{zone}  (only on {})", on.join(", "));
        }
    }
    for why in fleet.down() {
        eprintln!("{why}");
    }
    Ok(ok)
}

/// `uc dns zone add`: makes `zone` on every instance. Without `ns`, its
/// name servers are `ns1`, `ns2`, ... in the zone, one per worker, with
/// that worker's address: any node answers for every instance.
pub async fn add_zone(context: &str, zone: &str, ns: &[String], ttl: u32) -> Result<bool> {
    let zone = names::zone(zone)?;
    let fleet = Fleet::connect(context).await?;
    let mut records = Records::new();
    if ns.is_empty() {
        let workers = workers(context, &answering(context).await?).await?;
        if workers.len() < 2 {
            bail!(
                "port 53 answers on {} worker(s), and a zone wants two name servers at least; \
                 check `uc dns servers`, or name them with --ns",
                workers.len()
            );
        }
        let mut servers = Vec::new();
        for (i, (_, ip)) in workers.iter().enumerate() {
            let host = format!("ns{}.{zone}", i + 1);
            records.insert(
                (host.clone(), "A".into()),
                RRset {
                    ttl,
                    records: vec![ip.to_string()],
                },
            );
            servers.push(host);
        }
        records.insert(
            (zone.clone(), "NS".into()),
            RRset {
                ttl,
                records: servers,
            },
        );
    } else {
        let mut servers: Vec<String> = ns
            .iter()
            .map(|n| names::content("NS", n, &zone))
            .collect::<Result<_>>()?;
        servers.sort();
        records.insert(
            (zone.clone(), "NS".into()),
            RRset {
                ttl,
                records: servers,
            },
        );
    }
    let ok = each(&fleet, &format!("the zone {zone}"), |p| {
        let (zone, records) = (zone.clone(), records.clone());
        async move {
            if p.zones().await?.contains(&zone) {
                return Ok(format!("{zone} is there already"));
            }
            p.create_zone(&zone, &records).await?;
            Ok(format!("added {zone}"))
        }
    })
    .await;
    if ok {
        println!("\nAt your registrar, give {zone} these name servers:");
        for ((name, kind), set) in records.iter().filter(|((_, k), _)| k != "NS") {
            println!("  {name}  {kind}  {}", set.records.join(", "));
        }
        if ns.is_empty() {
            println!("(the addresses are glue records, since the names are in the zone itself)");
        } else {
            println!("  {}", ns.join(", "));
        }
    }
    Ok(ok)
}

/// `uc dns zone rm`: deletes `zone` from every instance.
pub async fn remove_zone(context: &str, zone: &str, yes: bool) -> Result<bool> {
    let zone = names::zone(zone)?;
    if !yes {
        confirm(&format!(
            "Delete {zone} and all its records from every name server?"
        ))?;
    }
    let fleet = Fleet::connect(context).await?;
    Ok(each(&fleet, &format!("{zone} deleted"), |p| {
        let zone = zone.clone();
        async move {
            if !p.zones().await?.contains(&zone) {
                return Ok(format!("has no {zone}"));
            }
            p.delete_zone(&zone).await?;
            Ok(format!("deleted {zone}"))
        }
    })
    .await)
}

/// `uc dns list`: the records of `zone`, marking those the instances
/// disagree on.
pub async fn list(context: &str, zone: &str) -> Result<bool> {
    let zone = names::zone(zone)?;
    let fleet = Fleet::connect(context).await?;
    let mut each_records: Vec<(&str, Option<Records>)> = Vec::new();
    for p in fleet.up() {
        each_records.push((&p.name, p.records(&zone).await?));
    }
    let mut ok = fleet.down().is_empty();
    let mut keys: Vec<&(String, String)> = each_records
        .iter()
        .flat_map(|(_, r)| r.iter().flat_map(|r| r.keys()))
        .collect();
    keys.sort();
    keys.dedup();
    if each_records.iter().all(|(_, r)| r.is_none()) {
        bail!("no name server has {zone}; `uc dns zone add {zone}` makes it");
    }
    for (name, r) in &each_records {
        if r.is_none() {
            ok = false;
            println!("; {name} has no {zone}");
        }
    }
    let width = keys.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
    for key in keys {
        let versions: Vec<(&str, Option<&RRset>)> = each_records
            .iter()
            .filter(|(_, r)| r.is_some())
            .map(|(n, r)| (*n, r.as_ref().and_then(|r| r.get(key))))
            .collect();
        let agree = versions.windows(2).all(|w| w[0].1 == w[1].1);
        if !agree {
            ok = false;
        }
        let mut shown: Vec<Option<&RRset>> = Vec::new();
        for (_, set) in &versions {
            if shown.contains(set) {
                continue;
            }
            shown.push(*set);
            let Some(set) = set else { continue };
            let on = if agree {
                String::new()
            } else {
                let names: Vec<&str> = versions
                    .iter()
                    .filter(|(_, s)| *s == Some(set))
                    .map(|(n, _)| *n)
                    .collect();
                format!("  ; only on {}", names.join(", "))
            };
            for content in &set.records {
                println!(
                    "{:width$}  {:>6}  {:6}  {content}{on}",
                    key.0, set.ttl, key.1
                );
            }
        }
    }
    for why in fleet.down() {
        eprintln!("{why}");
    }
    Ok(ok)
}

/// How a change treats the records already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Adds the contents to those there.
    Add,
    /// Replaces those there with the contents.
    Set,
    /// Takes the contents away, or all of them when none are given.
    Remove,
}

/// `uc dns add`, `set` and `rm`: changes one name's records of one type
/// on every instance.
pub async fn change(
    context: &str,
    zone: &str,
    name: &str,
    kind: &str,
    contents: &[String],
    ttl: Option<u32>,
    mode: Mode,
) -> Result<bool> {
    let zone = names::zone(zone)?;
    let name = names::name(name, &zone);
    let kind = names::kind(kind)?;
    let contents: Vec<String> = contents
        .iter()
        .map(|c| names::content(&kind, c, &zone))
        .collect::<Result<_>>()?;
    if mode != Mode::Remove && contents.is_empty() {
        bail!("give the record's content");
    }
    let fleet = Fleet::connect(context).await?;
    let what = format!("{name} {kind} changed");
    Ok(each(&fleet, &what, |p| {
        let (zone, name, kind, contents) =
            (zone.clone(), name.clone(), kind.clone(), contents.clone());
        async move {
            let records = p
                .records(&zone)
                .await?
                .ok_or_else(|| anyhow!("has no {zone}; `uc dns zone add {zone}` makes it"))?;
            let key = (name.clone(), kind.clone());
            let now = records.get(&key);
            let wanted = updated(now, &contents, ttl, mode);
            if wanted.as_ref() == now {
                return Ok(format!("{name} {kind} is as asked already"));
            }
            let summary = match &wanted {
                Some(set) => format!("{name} {} {kind} {}", set.ttl, set.records.join(", ")),
                None => format!("{name} {kind} removed"),
            };
            p.patch(&zone, &BTreeMap::from([(key, wanted)])).await?;
            Ok(summary)
        }
    })
    .await)
}

/// The RRset after a change to `now`, None when nothing is left.
pub fn updated(
    now: Option<&RRset>,
    contents: &[String],
    ttl: Option<u32>,
    mode: Mode,
) -> Option<RRset> {
    let mut records: Vec<String> = match mode {
        Mode::Add => now.map(|s| s.records.clone()).unwrap_or_default(),
        Mode::Set => Vec::new(),
        Mode::Remove if contents.is_empty() => return None,
        Mode::Remove => now
            .map(|s| s.records.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|r| !contents.contains(r))
            .collect(),
    };
    if mode != Mode::Remove {
        records.extend(contents.iter().cloned());
    }
    records.sort();
    records.dedup();
    if records.is_empty() {
        return None;
    }
    Some(RRset {
        ttl: ttl.or(now.map(|s| s.ttl)).unwrap_or(3600),
        records,
    })
}

/// `uc dns sync`: makes every other instance's zones the same as those of
/// `from`. Zones only the others have stay unless `prune`.
pub async fn sync(
    context: &str,
    from: &str,
    zones: &[String],
    prune: bool,
    check: bool,
    yes: bool,
) -> Result<bool> {
    let fleet = Fleet::connect(context).await?;
    let source = fleet.get(from)?;
    let wanted: Vec<String> = if zones.is_empty() {
        source.zones().await?
    } else {
        zones
            .iter()
            .map(|z| names::zone(z))
            .collect::<Result<_>>()?
    };
    let mut want: BTreeMap<String, Records> = BTreeMap::new();
    for zone in &wanted {
        let records = source
            .records(zone)
            .await?
            .ok_or_else(|| anyhow!("{from} has no {zone}"))?;
        want.insert(zone.clone(), records);
    }

    // The plan, per instance.
    enum Step {
        Create(String),
        Patch(String, BTreeMap<(String, String), Option<RRset>>),
        Delete(String),
    }
    let mut plan: Vec<(&Pdns, Vec<Step>)> = Vec::new();
    for p in fleet.up().filter(|p| p.name != from) {
        let have = p.zones().await?;
        let mut steps = Vec::new();
        for (zone, records) in &want {
            match p.records(zone).await? {
                None => steps.push(Step::Create(zone.clone())),
                Some(now) => {
                    let d = pdns::diff(records, &now);
                    if !d.is_empty() {
                        steps.push(Step::Patch(zone.clone(), d));
                    }
                }
            }
        }
        if prune && zones.is_empty() {
            for zone in have.iter().filter(|z| !want.contains_key(*z)) {
                steps.push(Step::Delete(zone.clone()));
            }
        } else if zones.is_empty() {
            for zone in have.iter().filter(|z| !want.contains_key(*z)) {
                println!(
                    "{}: keeps {zone}, which {from} doesn't have (--prune deletes it)",
                    p.name
                );
            }
        }
        plan.push((p, steps));
    }
    let mut any = false;
    for (p, steps) in &plan {
        for step in steps {
            any = true;
            match step {
                Step::Create(z) => println!("{}: add {z} with {} RRsets", p.name, want[z].len()),
                Step::Delete(z) => println!("{}: delete {z}", p.name),
                Step::Patch(z, d) => {
                    for ((name, kind), set) in d {
                        match set {
                            Some(s) => println!(
                                "{}: {z} {name} {} {kind} {}",
                                p.name,
                                s.ttl,
                                s.records.join(", ")
                            ),
                            None => println!("{}: {z} remove {name} {kind}", p.name),
                        }
                    }
                }
            }
        }
    }
    let down = fleet.down();
    for why in &down {
        eprintln!("{why}");
    }
    if !any {
        println!("every running name server matches {from}");
        return Ok(down.is_empty());
    }
    if check {
        return Ok(down.is_empty());
    }
    if !yes {
        confirm("Make these changes?")?;
    }
    let mut ok = down.is_empty();
    for (p, steps) in plan {
        for step in steps {
            let done = match &step {
                Step::Create(z) => p.create_zone(z, &want[z]).await,
                Step::Patch(z, d) => p.patch(z, d).await,
                Step::Delete(z) => p.delete_zone(z).await,
            };
            if let Err(err) = done {
                ok = false;
                eprintln!("{}: {err:#}", p.name);
            }
        }
    }
    if ok {
        println!("done");
    }
    Ok(ok)
}

fn confirm(question: &str) -> Result<()> {
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

    fn set(ttl: u32, records: &[&str]) -> RRset {
        RRset {
            ttl,
            records: records.iter().map(|r| r.to_string()).collect(),
        }
    }

    fn c(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn add_keeps_what_is_there() {
        let now = set(300, &["192.0.2.1"]);
        assert_eq!(
            updated(Some(&now), &c(&["192.0.2.2"]), None, Mode::Add),
            Some(set(300, &["192.0.2.1", "192.0.2.2"]))
        );
        assert_eq!(
            updated(None, &c(&["192.0.2.2"]), None, Mode::Add),
            Some(set(3600, &["192.0.2.2"]))
        );
    }

    #[test]
    fn set_replaces_and_rm_takes_away() {
        let now = set(300, &["192.0.2.1", "192.0.2.2"]);
        assert_eq!(
            updated(Some(&now), &c(&["192.0.2.3"]), Some(60), Mode::Set),
            Some(set(60, &["192.0.2.3"]))
        );
        assert_eq!(
            updated(Some(&now), &c(&["192.0.2.1"]), None, Mode::Remove),
            Some(set(300, &["192.0.2.2"]))
        );
        assert_eq!(updated(Some(&now), &[], None, Mode::Remove), None);
        assert_eq!(
            updated(
                Some(&set(300, &["192.0.2.1"])),
                &c(&["192.0.2.1"]),
                None,
                Mode::Remove
            ),
            None
        );
    }
}
