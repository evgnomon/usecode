// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-dns`: `uc dns`, see [`uc::dns`].

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::process::ExitCode;
use uc::dns::{self, Mode};
use uc::kube;

/// Look after the name servers on your cluster.
///
/// The `dns` role of `uc kube sync` runs two or more PowerDNS
/// instances that share nothing, behind port 53 of every node. Every
/// change here is made on each of them, so they answer the same.
#[derive(Parser)]
#[command(name = "uc dns", bin_name = "uc dns", version, after_help = EXAMPLES)]
struct Cli {
    /// The kubeconfig context [default: the current one].
    #[arg(long, value_name = "NAME", env = "UC_KUBE_CONTEXT", global = true)]
    context: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The instances, where they run, and the addresses port 53 answers on.
    Servers,
    /// The zones, and those not every name server has.
    Zones,
    /// Add or remove a zone.
    #[command(subcommand)]
    Zone(ZoneCommand),
    /// The records of a zone, marking those the name servers disagree on.
    List {
        /// The zone, e.g. example.com.
        zone: String,
    },
    /// Add records to a name, keeping those it has.
    Add(Record),
    /// Replace a name's records of a type with these.
    Set(Record),
    /// Remove these records, or all of a name's records of a type.
    Rm {
        zone: String,
        /// `@` for the zone itself, `www`, or a full name.
        name: String,
        /// A, AAAA, CNAME, MX, TXT, ...
        #[arg(value_name = "TYPE")]
        kind: String,
        /// The contents to remove [default: all of them].
        contents: Vec<String>,
    },
    /// Make the other name servers the same as one of them.
    Sync {
        /// The name server whose zones are right, e.g. pdns-1.
        #[arg(long, value_name = "NAME")]
        from: String,
        /// Only these zones [default: all of them].
        zones: Vec<String>,
        /// Also delete zones that one doesn't have.
        #[arg(long)]
        prune: bool,
        /// Show what would change, change nothing.
        #[arg(short = 'C', long)]
        check: bool,
        /// Don't ask before changing anything.
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum ZoneCommand {
    /// Make a zone on every name server.
    Add {
        zone: String,
        /// Its name servers [default: ns1, ns2, ... in the zone, one per
        /// worker, with its address].
        #[arg(long, value_name = "NAME")]
        ns: Vec<String>,
        /// The TTL of the NS records and their addresses.
        #[arg(long, default_value_t = 3600)]
        ttl: u32,
    },
    /// Delete a zone, and all its records, from every name server.
    Rm {
        zone: String,
        /// Don't ask first.
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(clap::Args)]
struct Record {
    /// The zone, e.g. example.com.
    zone: String,
    /// `@` for the zone itself, `www`, or a full name.
    name: String,
    /// A, AAAA, CNAME, MX, TXT, ...
    #[arg(value_name = "TYPE")]
    kind: String,
    /// One or more contents: `192.0.2.1`, `10 mail`, `v=spf1 mx -all`.
    #[arg(required = true)]
    contents: Vec<String>,
    /// The TTL in seconds [default: the one there, or 3600].
    #[arg(long)]
    ttl: Option<u32>,
}

const EXAMPLES: &str = "\
Examples:
  uc dns servers                              where the name servers answer
  uc dns zone add example.com                 a zone, ns1 and ns2 on the workers
  uc dns add example.com @ A 192.0.2.10
  uc dns add example.com www CNAME @
  uc dns add example.com @ MX '10 mail'
  uc dns set example.com @ TXT 'v=spf1 mx -all' --ttl 300
  uc dns rm example.com www CNAME
  uc dns list example.com
  uc dns sync --from pdns-1 -C                what pdns-2 is missing

A name without a dot is in the zone (`mail` is mail.example.com), one with
dots is a full name. TXT contents are quoted for you.

If a name server was down for a change, the others got it: once it's
back, `uc dns sync --from <one that has it>` catches it up.";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("zones and records on your own name servers");
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    let run = || -> Result<bool> {
        let context = cli
            .context
            .clone()
            .map_or_else(kube::cluster::current_context, Ok)?;
        let c = context.as_str();
        tokio::runtime::Runtime::new()?.block_on(async {
            match cli.command {
                Command::Servers => dns::show_servers(c).await,
                Command::Zones => dns::show_zones(c).await,
                Command::Zone(ZoneCommand::Add { zone, ns, ttl }) => {
                    dns::add_zone(c, &zone, &ns, ttl).await
                }
                Command::Zone(ZoneCommand::Rm { zone, yes }) => {
                    dns::remove_zone(c, &zone, yes).await
                }
                Command::List { zone } => dns::list(c, &zone).await,
                Command::Add(r) => record(c, r, Mode::Add).await,
                Command::Set(r) => record(c, r, Mode::Set).await,
                Command::Rm {
                    zone,
                    name,
                    kind,
                    contents,
                } => dns::change(c, &zone, &name, &kind, &contents, None, Mode::Remove).await,
                Command::Sync {
                    from,
                    zones,
                    prune,
                    check,
                    yes,
                } => dns::sync(c, &from, &zones, prune, check, yes).await,
            }
        })
    };
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("uc dns: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn record(context: &str, r: Record, mode: Mode) -> Result<bool> {
    dns::change(context, &r.zone, &r.name, &r.kind, &r.contents, r.ttl, mode).await
}
