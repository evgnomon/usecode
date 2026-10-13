// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-mail-account`: `uc mail account`, see [`uc::mail::manage`].

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::process::ExitCode;
use uc::kube;
use uc::mail::manage::{self, NewAccount};

/// The mail accounts on your mail server, their passwords and aliases.
///
/// For the Stalwart the mail role runs on the cluster kubectl points at.
/// An account's domain has to be there first: `uc mail domain add`.
#[derive(Parser)]
#[command(name = "uc mail account", bin_name = "uc mail account", version, after_help = EXAMPLES)]
struct Cli {
    /// The kubeconfig context [default: the current one].
    #[arg(long, value_name = "NAME", env = "UC_KUBE_CONTEXT", global = true)]
    context: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The accounts, with the mail they keep and their aliases.
    #[command(visible_alias = "ls")]
    List {
        /// Only this domain's.
        domain: Option<String>,
    },
    /// Make an account.
    Add {
        /// Its address, e.g. ada@example.com.
        address: String,
        /// Let it manage Stalwart too.
        #[arg(long)]
        admin: bool,
        /// Who it's for, e.g. "Ada Lovelace".
        #[arg(long, value_name = "TEXT")]
        description: Option<String>,
        /// The most mail it may keep, e.g. 5G [default: no limit].
        #[arg(long, value_name = "SIZE", value_parser = manage::size)]
        quota: Option<u64>,
        /// Make up a password and print it, instead of asking for one.
        #[arg(long)]
        random: bool,
    },
    /// Delete an account and all its mail.
    Rm {
        address: String,
        /// Don't ask first.
        #[arg(short, long)]
        yes: bool,
    },
    /// Change an account's password.
    #[command(visible_alias = "passwd")]
    Password {
        address: String,
        /// Make up a password and print it, instead of asking for one.
        #[arg(long)]
        random: bool,
    },
    /// Give an account more addresses, or take them away.
    #[command(subcommand)]
    Alias(AliasCommand),
    /// Set the most mail an account may keep.
    Quota {
        address: String,
        /// e.g. 5G or 500M, or `none` for no limit.
        #[arg(value_name = "SIZE|none")]
        size: String,
    },
}

#[derive(Subcommand)]
enum AliasCommand {
    /// Send mail to these addresses to the account too.
    Add {
        address: String,
        #[arg(required = true, value_name = "ALIAS")]
        aliases: Vec<String>,
    },
    /// Stop sending mail to these addresses to the account.
    Rm {
        address: String,
        #[arg(required = true, value_name = "ALIAS")]
        aliases: Vec<String>,
    },
}

const EXAMPLES: &str = "\
Examples:
  uc mail account list
  uc mail account list example.com
  uc mail account add ada@example.com                asks for a password
  uc mail account add bob@example.com --random --quota 5G
  uc mail account alias add ada@example.com hello@example.com info@example.org
  uc mail account password ada@example.com
  uc mail account quota ada@example.com none
  uc mail account rm bob@example.com

Without a terminal, the password is the first line of stdin. An alias's
domain has to be in Stalwart too. `uc mail settings <address>` prints what
to type into a mail app. Stalwart is reached as the recovery admin in
mail/mail-admin, or UC_MAIL_USER and UC_MAIL_PASSWORD.";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("mail accounts, their passwords and aliases");
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
                Command::List { domain } => manage::list_accounts(c, domain.as_deref()).await,
                Command::Add {
                    address,
                    admin,
                    description,
                    quota,
                    random,
                } => {
                    let new = NewAccount {
                        admin,
                        description,
                        quota,
                        random,
                    };
                    manage::add_account(c, &address, new).await
                }
                Command::Rm { address, yes } => manage::remove_account(c, &address, yes).await,
                Command::Password { address, random } => {
                    manage::set_password(c, &address, random).await
                }
                Command::Alias(AliasCommand::Add { address, aliases }) => {
                    manage::change_aliases(c, &address, &aliases, true).await
                }
                Command::Alias(AliasCommand::Rm { address, aliases }) => {
                    manage::change_aliases(c, &address, &aliases, false).await
                }
                Command::Quota { address, size } => {
                    let quota = match size.as_str() {
                        "none" => None,
                        s => Some(manage::size(s)?),
                    };
                    manage::set_quota(c, &address, quota).await
                }
            }
        })
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc mail account: {err:#}");
            ExitCode::FAILURE
        }
    }
}
