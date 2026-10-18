// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-kube-usage`: `uc kube usage`, see [`uc::usage`].

use anyhow::Result;
use clap::Parser;
use std::io::{IsTerminal, Write};
use std::process::ExitCode;
use std::time::Duration;
use uc::kube;
use uc::usage::Usage;

/// CPU, memory, disk and network across your cluster.
///
/// The cluster's totals, each node against what it can give, each
/// namespace, and the pods using the most. Requests are shown next to
/// usage, so you see how much of the cluster is promised away. CPU and
/// memory come from metrics-server; without it you still get the rest.
/// Disk and network come from each node's kubelet. Network shows the bytes
/// moved since each node or pod started, and bytes a second with --watch.
#[derive(Parser)]
#[command(
    name = "uc kube usage",
    bin_name = "uc kube usage",
    version,
    after_help = EXAMPLES
)]
struct Cli {
    /// The kubeconfig context [default: the current one].
    #[arg(long, value_name = "NAME", env = "UC_KUBE_CONTEXT")]
    context: Option<String>,

    /// Only the pods of this namespace (again for more).
    #[arg(short, long, value_name = "NS")]
    namespace: Vec<String>,

    /// Rows in the top-pod tables.
    #[arg(short, long, value_name = "N", default_value_t = 10,
          value_parser = clap::value_parser!(u16).range(1..))]
    top: u16,

    /// Keep running, redrawing every --interval seconds.
    #[arg(short, long)]
    watch: bool,

    /// Seconds between redraws with --watch.
    #[arg(short, long, value_name = "SECS", default_value_t = 5.0)]
    interval: f64,

    /// Plain output, no colours.
    #[arg(long)]
    no_color: bool,
}

const EXAMPLES: &str = "\
Examples:
  uc kube usage                     the whole cluster
  uc kube usage -n shop -n obs      only the pods of two namespaces
  uc kube usage -t 20               the top 20 pods
  uc kube usage -w                  keep watching, every 5 seconds
  uc kube usage -w -i 2             every 2 seconds";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("CPU, memory, disk and network across your cluster");
        return ExitCode::SUCCESS;
    }
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc kube usage: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let context = cli
        .context
        .map_or_else(kube::cluster::current_context, Ok)?;
    let tty = std::io::stdout().is_terminal();
    let color = !cli.no_color && tty && std::env::var_os("NO_COLOR").is_none();
    let top = cli.top.into();
    if !cli.watch {
        print!(
            "{}",
            Usage::read(&context, &cli.namespace)?.render(top, color)
        );
        return Ok(());
    }
    let interval = Duration::from_secs_f64(cli.interval.max(0.5));
    let mut before: Option<Usage> = None;
    loop {
        // Read before clearing, so the screen doesn't sit empty meanwhile.
        let report = Usage::read(&context, &cli.namespace).map(|mut u| {
            if let Some(before) = &before {
                u.since(before);
            }
            let report = u.render(top, color);
            before = Some(u);
            report
        });
        let mut out = std::io::stdout().lock();
        if tty {
            write!(out, "\x1b[H\x1b[2J")?;
        }
        match report {
            Ok(report) => write!(out, "{report}")?,
            Err(err) => writeln!(out, "uc kube usage: {err:#}")?,
        }
        writeln!(out, "every {}s, Ctrl-C to stop", cli.interval.max(0.5))?;
        out.flush()?;
        drop(out);
        std::thread::sleep(interval);
    }
}
