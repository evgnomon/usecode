// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-mail-domain`: `uc mail domain`, see [`uc::mail::manage`].

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::process::ExitCode;
use uc::kube;
use uc::mail::manage;

/// The domains your mail server takes mail for.
///
/// For the Stalwart the mail role runs on the cluster kubectl points at.
/// A new domain gets its DKIM keys from Stalwart; `uc mail configure`
/// then puts them, and the rest of the DNS mail needs, in place.
#[derive(Parser)]
#[command(name = "uc mail domain", bin_name = "uc mail domain", version, after_help = EXAMPLES)]
struct Cli {
    /// The kubeconfig context [default: the current one].
    #[arg(long, value_name = "NAME", env = "UC_KUBE_CONTEXT", global = true)]
    context: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The domains, with their accounts, aliases and catch-all.
    #[command(visible_alias = "ls")]
    List,
    /// Take mail for a domain.
    Add {
        /// The domain, e.g. example.com.
        domain: String,
        /// Another domain whose mail goes to the same accounts (repeatable).
        #[arg(long, value_name = "DOMAIN")]
        alias: Vec<String>,
    },
    /// Stop taking mail for a domain that has no accounts left.
    Rm {
        domain: String,
        /// Don't ask first.
        #[arg(short, long)]
        yes: bool,
    },
    /// Show or set where mail to a domain's unknown addresses goes.
    CatchAll {
        domain: String,
        /// The address to send it to [default: show where it goes].
        to: Option<String>,
        /// Refuse it again.
        #[arg(long)]
        off: bool,
    },
}

const EXAMPLES: &str = "\
Examples:
  uc mail domain list
  uc mail domain add example.com
  uc mail domain add example.com --alias example.org
  uc mail domain catch-all example.com ada@example.com
  uc mail domain catch-all example.com --off
  uc mail domain rm example.com

After adding a domain, `uc mail configure <domain>` puts its DNS records in
place. Stalwart is reached as the recovery admin in mail/mail-admin, or
UC_MAIL_USER and UC_MAIL_PASSWORD.";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("the domains the mail server takes mail for");
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    let run = || -> Result<()> {
        let context = cli
            .context
            .clone()
            .map_or_else(kube::cluster::current_context, Ok)?;
        let c = context.as_str();
        tokio::runtime::Runtime::new()?.block_on(async {
            match cli.command {
                Command::List => manage::list_domains(c).await,
                Command::Add { domain, alias } => manage::add_domain(c, &domain, &alias).await,
                Command::Rm { domain, yes } => manage::remove_domain(c, &domain, yes).await,
                Command::CatchAll { domain, to, off } => {
                    manage::catch_all(c, &domain, to.as_deref(), off).await
                }
            }
        })
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc mail domain: {err:#}");
            ExitCode::FAILURE
        }
    }
}
