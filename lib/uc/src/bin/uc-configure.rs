// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-configure` — configure this machine: packages, repositories, dotfiles,
//! toolchains, as a native, parallel task graph.

use clap::Parser;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use uc::configure::engine::Selection;
use uc::configure::vars::{LoadOptions, Profile};
use uc::configure::{self, Options};

const SUMMARY: &str = "configure this machine, running independent tasks in parallel";

/// Configure this machine: packages, repositories, dotfiles and toolchains.
///
/// Every task names the tasks it runs after, and starts as soon as those
/// have run or were left out by the tags, so independent tasks run in
/// parallel. Tags work as in Ansible: `always` tasks run unless skipped by
/// name, `never` tasks only when asked for. A task's id and its role
/// (the part before the `/`) can be used as tags, and tags may contain `*`.
#[derive(Parser)]
#[command(name = "uc configure", bin_name = "uc configure", version, after_help = EXAMPLES)]
struct Cli {
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
    #[arg(short, long, value_name = "N", env = "UC_CONFIGURE_JOBS", default_value_t = default_jobs())]
    jobs: usize,

    /// The installation profile; detected from DEV_CONTAINER and WSL when unset.
    #[arg(short, long, env = "INSTALL_PROFILE", value_enum)]
    profile: Option<Profile>,

    /// Set a variable for the templates, e.g. -e blueprint_user=alice (repeatable).
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

    /// The roles directory holding the files and templates [default: lib/uc/roles].
    #[arg(long, value_name = "DIR", env = "UC_CONFIGURE_ROLES")]
    roles: Option<PathBuf>,

    /// The user config file [default: ~/src/github.com/<user>/config/config.yaml].
    #[arg(long, value_name = "FILE", env = "UC_CONFIGURE_CONFIG")]
    config: Option<PathBuf>,

    /// Never color the output.
    #[arg(long, env = "NO_COLOR", value_parser = clap::builder::FalseyValueParser::new())]
    no_color: bool,
}

const EXAMPLES: &str = "\
Examples:
  uc configure                       run everything for the detected profile
  uc configure -C                    show what would change
  uc configure -t dotfiles,git       only the dotfiles and git roles
  uc configure -t zls -d             zls and everything it needs
  uc configure -t upgrade            also run the `never` upgrade tasks
  uc configure -s apt,extrepo -j 16  skip packages, 16 tasks at a time
  uc configure -l -t 'git/clone:*'   list the clone tasks
  uc configure --graph | dot -Tsvg > graph.svg";

fn default_jobs() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get())
}

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("{SUMMARY}");
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
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
            profile: cli.profile,
            extra: cli.extra_vars,
            roles_dir: cli.roles,
            config_file: cli.config,
        },
    };
    match configure::run(opts) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("uc-configure: {err:#}");
            ExitCode::FAILURE
        }
    }
}
