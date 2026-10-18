// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-kube-logs`: `uc kube logs`, see [`uc::logs`].

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use serde_json::Value;
use std::io::{IsTerminal, Write};
use std::process::ExitCode;
use uc::kube;
use uc::logs::{
    CONTAINER_FIELD, Filter, Logs, NAMESPACE_FIELD, NODE_FIELD, POD_FIELD, line, short_time,
    sort_key,
};

/// The logs of every container on your cluster, in one place.
///
/// They come from the VictoriaLogs that `uc kube sync -t obs` runs,
/// which keeps them even after a pod is gone, or from UC_LOGS_URL. Narrow
/// them down with the options; words after them are looked for in the
/// lines, and can be any LogsQL filter.
#[derive(Parser)]
#[command(
    name = "uc kube logs",
    bin_name = "uc kube logs",
    version,
    after_help = EXAMPLES,
    args_conflicts_with_subcommands = true
)]
struct Cli {
    /// The kubeconfig context [default: the current one].
    #[arg(long, value_name = "NAME", env = "UC_KUBE_CONTEXT", global = true)]
    context: Option<String>,

    #[command(flatten)]
    filter: FilterArgs,

    /// How many of the last lines to show.
    #[arg(long, value_name = "N", default_value_t = 100)]
    tail: usize,

    /// Keep printing new lines as they come.
    #[arg(short, long)]
    follow: bool,

    /// Print each entry as JSON, with all its fields.
    #[arg(long)]
    json: bool,

    /// Words to look for (all of them), or any LogsQL filter, e.g.
    /// `timeout -healthz` or `'level:warn'`.
    #[arg(value_name = "WORDS")]
    words: Vec<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Args)]
struct FilterArgs {
    /// Only this namespace (again for more).
    #[arg(short, long, value_name = "NS", global = true)]
    namespace: Vec<String>,
    /// Only this pod, or the pods whose names start with it, like a
    /// Deployment's name (again for more).
    #[arg(short, long, value_name = "POD", global = true)]
    pod: Vec<String>,
    /// Only this container (again for more).
    #[arg(short, long, value_name = "NAME", global = true)]
    container: Vec<String>,
    /// Only pods on this node (again for more).
    #[arg(long, value_name = "NODE", global = true)]
    node: Vec<String>,
    /// Only pods with this label (again for all of them).
    #[arg(short, long, value_name = "KEY=VALUE", global = true)]
    label: Vec<String>,
    /// Only lines that look like errors.
    #[arg(short, long, global = true)]
    errors: bool,
    /// How far back: 15m, 2h, 7d, or a time like 2026-10-11T15:00:00Z.
    #[arg(short, long, value_name = "WHEN", default_value = "1h", global = true)]
    since: String,
}

#[derive(Subcommand)]
enum Command {
    /// How many lines each namespace logged, and when it last did.
    Namespaces,
    /// How many lines each pod logged, and when it last did.
    Pods,
    /// How many lines each container logged, and when it last did.
    Containers,
    /// How many lines the pods on each node logged.
    Nodes,
    /// The fields the lines have, to filter on with `field:value`.
    Fields,
    /// The LogsQL query the options make, without running it.
    Query,
}

const EXAMPLES: &str = "\
Examples:
  uc kube logs                                  the last 100 lines of the last hour
  uc kube logs -f                               and keep following
  uc kube logs -n obs                           one namespace
  uc kube logs -p backend -f                    every backend-* pod, following
  uc kube logs -p backend -c migrate --since 2d one container of them, 2 days back
  uc kube logs -l app.kubernetes.io/name=web    by label
  uc kube logs --node cp-2 --tail 500           what ran on one node
  uc kube logs -e --since 24h                   errors of the last day
  uc kube logs timeout -healthz                 lines with timeout, without healthz
  uc kube logs 'level:warn'                     any LogsQL filter
  uc kube logs --json -p backend | jq .         every field
  uc kube logs pods -e --since 24h              which pods had errors, and how many
  uc kube logs namespaces                       who logs the most
  uc kube logs fields -n shop                   what there is to filter on
  uc kube logs query -p backend -e              the LogsQL behind it

Somewhere else than the cluster's VictoriaLogs:
  UC_LOGS_URL=http://127.0.0.1:9428 uc kube logs";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("the logs of every container on your cluster");
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    let result = tokio::runtime::Runtime::new()
        .map_err(Into::into)
        .and_then(|rt| rt.block_on(run(cli)));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc kube logs: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    let f = &cli.filter;
    let filter = Filter {
        namespaces: f.namespace.clone(),
        pods: f.pod.clone(),
        containers: f.container.clone(),
        nodes: f.node.clone(),
        labels: f.label.clone(),
        errors: f.errors,
        words: cli.words.clone(),
    };
    let query = filter.query_since(&f.since)?;
    if let Some(Command::Query) = cli.command {
        println!("{query}");
        return Ok(());
    }

    let context = cli.context;
    let logs = Logs::connect(|| context.map_or_else(kube::cluster::current_context, Ok)).await?;
    let by: &[&str] = match cli.command {
        None => return print_lines(&logs, &filter, &query, cli.tail, cli.follow, cli.json).await,
        Some(Command::Query) => unreachable!("printed above"),
        Some(Command::Fields) => {
            for (name, hits) in logs.fields(&query).await? {
                println!("{hits:>10}  {name}");
            }
            return Ok(());
        }
        Some(Command::Namespaces) => &[NAMESPACE_FIELD],
        Some(Command::Pods) => &[NAMESPACE_FIELD, POD_FIELD],
        Some(Command::Containers) => &[NAMESPACE_FIELD, POD_FIELD, CONTAINER_FIELD],
        Some(Command::Nodes) => &[NODE_FIELD],
    };
    let rows = logs.count_by(&query, by).await?;
    println!("{:>10}  {:23}  NAME", "LINES", "LAST");
    for row in &rows {
        let name: Vec<&str> = by.iter().map(|f| row[*f].as_str().unwrap_or("?")).collect();
        println!(
            "{:>10}  {:23}  {}",
            row["logs"].as_str().unwrap_or("?"),
            short_time(row["last"].as_str().unwrap_or_default()),
            name.join("/")
        );
    }
    Ok(())
}

async fn print_lines(
    logs: &Logs,
    filter: &Filter,
    query: &str,
    tail: usize,
    follow: bool,
    json: bool,
) -> Result<()> {
    let color = !json && std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let mut out = std::io::stdout().lock();
    let mut print = |entry: &Value| -> bool {
        let text = if json {
            entry.to_string()
        } else {
            line(entry, color)
        };
        // A closed pipe (`| head`) ends it quietly.
        writeln!(out, "{text}").and_then(|_| out.flush()).is_ok()
    };
    let mut newest = String::new();
    if tail > 0 {
        for entry in logs.last(query, tail).await? {
            if !print(&entry) {
                return Ok(());
            }
            newest = newest.max(sort_key(&entry));
        }
    }
    if !follow {
        return Ok(());
    }
    // Tailing starts a little in the past; skip what was printed already.
    logs.tail(&filter.query()?, |entry| {
        sort_key(&entry) <= newest || print(&entry)
    })
    .await
}
