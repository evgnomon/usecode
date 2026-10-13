// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-forward`: `uc forward`, see [`uc::forward`].

use anyhow::{Result, bail};
use clap::Parser;
use std::collections::BTreeMap;
use std::process::ExitCode;
use tokio::signal::unix::{SignalKind, signal};
use uc::forward::{self, Service, Taken};
use uc::kube::cluster::current_context;
use uc::kube::remote::kubectl_json;
use uc::workspace;

/// Your cluster's services on localhost, in one go.
///
/// Name a namespace and every Service in it with a TCP port is forwarded,
/// each to the same port on localhost when it's free, and started again
/// when it drops. Ctrl-C stops them all.
#[derive(Parser)]
#[command(name = "uc forward", bin_name = "uc forward", version, after_help = EXAMPLES)]
struct Cli {
    /// The kubeconfig context [default: the current one].
    #[arg(long, value_name = "NAME", env = "UC_KUBE_CONTEXT")]
    context: Option<String>,

    /// A namespace, or one Service of it: `obs`, `obs/vmsingle`,
    /// `rustfs/rustfs:9001`; `ws` for your workspaces, `ws/dev` or
    /// `ws/dev:3000`. Without any, the namespaces there are.
    #[arg(value_name = "TARGET")]
    targets: Vec<String>,
}

const EXAMPLES: &str = "\
Examples:
  uc forward                     the namespaces with something to forward
  uc forward obs                 metrics, logs and alerts: vmui and friends
  uc forward obs rustfs          more than one namespace
  uc forward obs/vmsingle        one Service
  uc forward rustfs/rustfs:9001  one port of it, the RustFS console
  uc forward idiomatic-rs        your own apps, the same way
  uc forward ws                  what listens in your workspaces, as it comes and goes
  uc forward ws/dev:3000         one port of workspace dev, listening yet or not";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("your cluster's services on localhost, in one go");
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    let result = tokio::runtime::Runtime::new()
        .map_err(Into::into)
        .and_then(|rt| rt.block_on(run(cli)));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc forward: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    let context = match cli.context {
        Some(c) => c,
        None => current_context()?,
    };
    if cli.targets.is_empty() {
        return list(&context).await;
    }

    // The picks of each namespace; none means all of it.
    let mut wanted: BTreeMap<String, Vec<(String, Option<u16>)>> = BTreeMap::new();
    for target in &cli.targets {
        match target.split_once('/') {
            Some((ns, pick)) => wanted
                .entry(ns.to_string())
                .or_default()
                .push(forward::parse_pick(pick)?),
            None => {
                wanted.entry(target.clone()).or_default();
            }
        }
    }
    let mut services: Vec<Service> = Vec::new();
    // Workspaces whose ports are followed rather than named.
    let mut follow: Vec<String> = Vec::new();
    for (ns, picks) in &wanted {
        if forward::is_workspaces(ns) {
            workspaces(&context, picks, &mut services, &mut follow).await?;
            continue;
        }
        let list = kubectl_json(&context, &["-n", ns, "get", "svc"]).await?;
        let found = forward::services(ns, &list, picks)?;
        if found.is_empty() {
            bail!(
                "{ns} has no Service with a TCP port to forward (uc forward lists the ones that do)"
            );
        }
        services.extend(found);
    }
    let taken = Taken::default();
    forward::assign(&mut services, &taken, forward::is_free)?;

    println!("context {context}, Ctrl-C to stop:");
    let rows: Vec<(String, String, String)> = services
        .iter()
        .flat_map(|s| {
            s.ports.iter().map(move |p| {
                let (what, at) = forward::describe(s, p);
                (format!("{}/{}:{}", s.namespace, s.name, p.port), at, what)
            })
        })
        .collect();
    let (w1, w2) = rows.iter().fold((0, 0), |(a, b), (s, at, _)| {
        (a.max(s.len()), b.max(at.len()))
    });
    for (service, at, what) in &rows {
        println!("  {service:w1$}  {at:w2$}  {what}");
    }
    for name in &follow {
        println!(
            "  {}/{name}: what listens in it, as it opens",
            workspace::NAMESPACE
        );
    }

    for service in services {
        tokio::spawn(forward::keep_forwarding(context.clone(), service));
    }
    for name in follow {
        tokio::spawn(forward::follow_workspace(
            context.clone(),
            name,
            taken.clone(),
        ));
    }
    // Returning drops the forwards, which ends their kubectl.
    let mut term = signal(SignalKind::terminate())?;
    let mut hangup = signal(SignalKind::hangup())?;
    tokio::select! {
        r = tokio::signal::ctrl_c() => r?,
        _ = term.recv() => {}
        _ = hangup.recv() => {}
    }
    Ok(())
}

/// The workspaces `picks` names, or all that aren't stopped: those with a
/// port named go in `services`, the others in `follow`.
async fn workspaces(
    context: &str,
    picks: &[(String, Option<u16>)],
    services: &mut Vec<Service>,
    follow: &mut Vec<String>,
) -> Result<()> {
    let list = kubectl_json(context, &["-n", workspace::NAMESPACE, "get", "deployments"]).await?;
    let rows = workspace::rows(&list);
    if picks.is_empty() {
        let running: Vec<String> = rows
            .into_iter()
            .filter(|r| r.state != "stopped")
            .map(|r| r.name)
            .collect();
        if running.is_empty() {
            bail!("no workspace runs on {context}; `uc ws up` makes one");
        }
        follow.extend(running);
        return Ok(());
    }
    // Each workspace once, with the ports named; none named means follow.
    let mut ports: BTreeMap<&str, Option<Vec<u16>>> = BTreeMap::new();
    for (name, port) in picks {
        let name = name.strip_prefix("ws-").unwrap_or(name);
        let Some(row) = rows.iter().find(|r| r.name == name) else {
            bail!("no workspace {name} on {context}; `uc ws ls` lists them");
        };
        if row.state == "stopped" {
            bail!("{name} is stopped; `uc ws start {name}` starts it");
        }
        let entry = ports.entry(&row.name).or_insert_with(|| Some(Vec::new()));
        match (entry.as_mut(), port) {
            (Some(list), Some(port)) => list.push(*port),
            _ => *entry = None,
        }
    }
    for (name, ports) in ports {
        match ports {
            Some(ports) => services.push(forward::workspace_service(name, &ports)),
            None => follow.push(name.to_string()),
        }
    }
    Ok(())
}

/// The namespaces with something to forward, and how many Services each.
async fn list(context: &str) -> Result<()> {
    let all = kubectl_json(context, &["get", "svc", "-A"]).await?;
    let mut by_ns: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for item in all["items"].as_array().into_iter().flatten() {
        let ns = item["metadata"]["namespace"].as_str().unwrap_or_default();
        if ns == "default" || ns == "kube-system" {
            continue;
        }
        let list = serde_json::json!({"items": [item]});
        for s in forward::services(ns, &list, &[])? {
            // A workspace's Service is its sshd; `uc forward ws` forwards
            // what listens in it instead.
            let (ns, name) = match s.name.strip_prefix("ws-") {
                Some(name) if forward::is_workspaces(ns) => ("ws", name.to_string()),
                _ => (ns, s.name),
            };
            by_ns.entry(ns.to_string()).or_default().push(name);
        }
    }
    if by_ns.is_empty() {
        println!("nothing to forward on {context}");
        return Ok(());
    }
    println!("on {context}, `uc forward <namespace>` forwards:");
    let width = by_ns.keys().map(String::len).max().unwrap_or(0);
    for (ns, names) in &by_ns {
        println!("  {ns:width$}  {}", names.join(", "));
    }
    Ok(())
}
