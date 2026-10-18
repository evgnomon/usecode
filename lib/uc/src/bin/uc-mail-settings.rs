// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-mail-settings`: `uc mail settings`, see [`uc::mail::settings`].

use anyhow::{Result, bail};
use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;
use uc::configure::vars::{LoadOptions, Vars};
use uc::kube::{self, var};
use uc::mail::settings;

/// Print what a mail app needs to read and send mail through the server.
///
/// The IMAP and SMTP server, ports, security and login for the Stalwart the
/// mail role runs on the cluster kubectl points at, ready to type into
/// Thunderbird, Apple Mail, K-9, mutt or any other mail app.
#[derive(Parser)]
#[command(name = "uc mail settings", bin_name = "uc mail settings", version, after_help = EXAMPLES)]
struct Cli {
    /// The account to show as the username [default: a placeholder].
    #[arg(value_name = "USER")]
    user: Option<String>,

    /// The kubeconfig context [default: the current one].
    #[arg(long, value_name = "NAME", env = "UC_KUBE_CONTEXT")]
    context: Option<String>,

    /// Set a variable, e.g. -e mail_host=mail.example.com (repeatable).
    #[arg(short = 'e', long = "extra-var", value_name = "KEY=VALUE")]
    extra_vars: Vec<String>,

    /// The user config, as for `uc kube sync`.
    #[arg(long, value_name = "FILE", env = "UC_CONFIGURE_CONFIG")]
    config: Option<PathBuf>,
}

const EXAMPLES: &str = "\
Examples:
  uc mail settings                    the settings, with a placeholder login
  uc mail settings ada@example.com    the settings for one account

It reads mail_host from your user config, under kube.<context> like the
mail role. Accounts are made with `uc mail account add`.";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("IMAP and SMTP settings for mail apps");
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc mail settings: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let context = cli
        .context
        .map_or_else(kube::cluster::current_context, Ok)?;
    let vars = Vars::load(LoadOptions {
        profile: None,
        extra: cli.extra_vars,
        roles_dir: None,
        config_file: cli.config,
        need_roles: false,
        overlay: vec!["kube".into(), context.clone()],
    })?;
    let Some(host) = var::<String>(&vars, "mail_host")? else {
        bail!(
            "mail_host is not set; put `mail_host: mail.example.com` under kube.{context} in your config"
        );
    };
    print!("{}", settings::render(&host, cli.user.as_deref()));
    Ok(())
}
