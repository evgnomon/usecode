// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-kube-configure`: `uc kube configure`, configure the Kubernetes cluster
//! kubectl points at, as a native, parallel task graph.

use anyhow::{Result, bail};
use clap::Parser;
use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use uc::configure::engine::Selection;
use uc::configure::vars::LoadOptions;
use uc::configure::{self, Options};
use uc::kube::{self, Cluster};

/// Configure the Kubernetes cluster kubectl points at.
///
/// `uc configure` for a cluster: the same tags, check mode, listing and
/// parallelism, but every task works on the cluster behind kubectl's
/// current context (or --context) with kubectl and helm. Nothing is
/// changed before you confirm which cluster that is.
#[derive(Parser)]
#[command(name = "uc kube configure", bin_name = "uc kube configure", version, after_help = EXAMPLES)]
struct Cli {
    /// The kubeconfig context to configure [default: the current one].
    #[arg(long, value_name = "NAME", env = "UC_KUBE_CONTEXT")]
    context: Option<String>,

    /// Don't ask before changing the cluster.
    #[arg(short, long)]
    yes: bool,

    /// Only run tasks with one of these tags (comma separated or repeated).
    #[arg(short, long, value_name = "TAGS", value_delimiter = ',')]
    tags: Vec<String>,

    /// Leave out tasks with one of these tags.
    #[arg(short = 's', long, value_name = "TAGS", value_delimiter = ',')]
    skip_tags: Vec<String>,

    /// Also run whatever the selected tasks run after.
    #[arg(short = 'd', long)]
    with_deps: bool,

    /// How many tasks may run at the same time.
    #[arg(short, long, value_name = "N", default_value_t = 4)]
    jobs: usize,

    /// Set a variable, e.g. -e hcloud_primary_location=fsn1 (repeatable).
    #[arg(short = 'e', long = "extra-var", value_name = "KEY=VALUE")]
    extra_vars: Vec<String>,

    /// Report what would change without changing anything.
    #[arg(short = 'C', long)]
    check: bool,

    /// Stop starting new tasks after the first failure.
    #[arg(short = 'x', long)]
    fail_fast: bool,

    /// List the selected tasks with their tags and dependencies, then exit.
    #[arg(short, long)]
    list: bool,

    /// List every tag with the number of tasks carrying it, then exit.
    #[arg(long)]
    list_tags: bool,

    /// Print the selected task graph in Graphviz DOT format, then exit.
    #[arg(long)]
    graph: bool,

    /// Stream the output of every command.
    #[arg(short, long)]
    verbose: bool,

    /// The user config file, whose variables the roles read too
    /// [default: ~/src/github.com/<user>/config/config.yaml].
    #[arg(long, value_name = "FILE", env = "UC_CONFIGURE_CONFIG")]
    config: Option<PathBuf>,

    /// Never color the output.
    #[arg(long, env = "NO_COLOR", value_parser = clap::builder::FalseyValueParser::new())]
    no_color: bool,
}

const EXAMPLES: &str = "\
Examples:
  uc kube configure -C                    show the plan and what would change
  uc kube configure                       configure the current context
  uc kube configure --context prod -t hcloud_csi
  uc kube configure -e hcloud_primary_location=fsn1
  uc kube configure -t forgejo -e forgejo_host=git.example.com
                                          Forgejo on a Hetzner Volume
  uc kube configure -t registry -e registry_host=registry.example.com
                                          a container registry on a 10Gi
                                          Hetzner Volume
  uc kube configure -t obs -e obs_enabled=true
                                          metrics, logs and alerts, each
                                          store on a Hetzner Volume
  uc kube configure -t smoke             a 10Gi test volume (-t failover,
                                          then -t smoke-clean deletes it)

Node locations come from the Hetzner Cloud API, with HCLOUD_TOKEN (or
hetzner.prod in your secrets, as for uc vm); the cluster keeps the token in
kube-system/hcloud, so later runs need neither.";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("configure the Kubernetes cluster kubectl points at");
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    let read_only = cli.check || cli.list || cli.list_tags || cli.graph;
    let ask = !read_only && !cli.yes;
    let context = cli.context;
    let opts = Options {
        selection: Selection {
            tags: cli.tags,
            skip_tags: cli.skip_tags,
            with_deps: cli.with_deps,
        },
        jobs: cli.jobs.max(1),
        check: cli.check,
        fail_fast: cli.fail_fast,
        verbose: cli.verbose,
        color: !cli.no_color && std::io::stderr().is_terminal(),
        list: cli.list,
        list_tags: cli.list_tags,
        graph: cli.graph,
        load: LoadOptions {
            profile: None,
            extra: cli.extra_vars,
            roles_dir: None,
            config_file: cli.config,
            need_roles: false,
        },
    };
    let result = configure::run_with(opts, "uc kube configure", |vars| {
        let cluster = Arc::new(Cluster::load(context.as_deref())?);
        if ask {
            confirm(&cluster)?;
        }
        let target = format!("context {}", cluster.context);
        Ok((kube::plan(&cluster, vars)?, target))
    });
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("uc kube configure: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// Makes sure the cluster is the one meant before anything changes on it.
fn confirm(cluster: &Cluster) -> Result<()> {
    let what = format!(
        "context {} at {} ({} nodes)",
        cluster.context,
        cluster.server,
        cluster.nodes.len()
    );
    if !std::io::stdin().is_terminal() {
        bail!("not asking without a terminal: pass --yes to configure {what}");
    }
    eprint!("Configure {what}? Try -C first to see what changes. [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    if !matches!(answer.trim(), "y" | "Y" | "yes") {
        bail!("nothing changed");
    }
    Ok(())
}
