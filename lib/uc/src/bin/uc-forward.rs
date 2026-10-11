// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-forward`: `uc forward`, see [`uc::forward`].

use anyhow::{Result, bail};
use clap::Parser;
use std::collections::BTreeMap;
use std::process::ExitCode;
use tokio::signal::unix::{SignalKind, signal};
use uc::forward::{self, Service};
use uc::kube::cluster::current_context;
use uc::kube::remote::kubectl_json;

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
    /// `rustfs/rustfs:9001`. Without any, the namespaces there are.
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
  uc forward idiomatic-rs        your own apps, the same way";

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
    for (ns, picks) in &wanted {
        let list = kubectl_json(&context, &["-n", ns, "get", "svc"]).await?;
        let found = forward::services(ns, &list, picks)?;
        if found.is_empty() {
            bail!(
                "{ns} has no Service with a TCP port to forward (uc forward lists the ones that do)"
            );
        }
        services.extend(found);
    }
    forward::assign(&mut services, forward::is_free)?;

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

    for service in services {
        tokio::spawn(forward::keep_forwarding(context.clone(), service));
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
            by_ns.entry(ns.to_string()).or_default().push(s.name);
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
