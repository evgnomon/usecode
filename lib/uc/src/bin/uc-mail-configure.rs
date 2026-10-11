// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-mail-configure`: `uc mail configure`, see [`uc::mail::configure`].

use anyhow::{Result, bail};
use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;
use uc::configure::vars::{LoadOptions, Vars};
use uc::kube::{self, var};
use uc::mail::configure::{self, Options};

/// Make the mail server's mail arrive, and be trusted where it goes.
///
/// For the Stalwart the mail role runs on the cluster kubectl points at:
/// the DNS records of the mail name and of every domain Stalwart has (MX,
/// SPF, DKIM, DMARC, TLS reporting and the SRV records mail apps look for),
/// the workers' reverse DNS, and the Stalwart settings that go with them.
/// DNS changes on Cloudflare when a zone's NS records point there; any
/// other zone gets the records to add. Shows the plan and asks first.
#[derive(Parser)]
#[command(name = "uc mail configure", bin_name = "uc mail configure", version, after_help = EXAMPLES)]
struct Cli {
    /// Only these domains [default: every domain Stalwart has].
    #[arg(value_name = "DOMAIN")]
    domains: Vec<String>,

    /// The kubeconfig context [default: the current one].
    #[arg(long, value_name = "NAME", env = "UC_KUBE_CONTEXT")]
    context: Option<String>,

    /// Show what would change, change nothing.
    #[arg(short = 'C', long)]
    check: bool,

    /// Don't ask before changing anything.
    #[arg(short, long)]
    yes: bool,

    /// Set a variable, e.g. -e mail_dmarc_policy=quarantine (repeatable).
    #[arg(short = 'e', long = "extra-var", value_name = "KEY=VALUE")]
    extra_vars: Vec<String>,

    /// The user config, as for `uc kube configure`.
    #[arg(long, value_name = "FILE", env = "UC_CONFIGURE_CONFIG")]
    config: Option<PathBuf>,
}

const EXAMPLES: &str = "\
Examples:
  uc mail configure -C            show what would change
  uc mail configure               fix it all, after a yes
  uc mail configure example.com   only one domain

It reads mail_host, mail_node and mail_dmarc_policy (none, quarantine or the
default reject) from your user config, under kube.<context> like the mail
role.
Cloudflare needs CLOUDFLARE_API_TOKEN (source ~/.bashrc.d/cloudflare.sh)
with Zone:Read and DNS:Edit; Hetzner the cluster's token in
kube-system/hcloud, or HCLOUD_TOKEN. Stalwart is reached as the recovery
admin in mail/mail-admin, or UC_MAIL_USER and UC_MAIL_PASSWORD.

Run it again after adding a domain or a DKIM key in Stalwart, and after
Stalwart moves to another worker.";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("DNS, reverse DNS and Stalwart settings for trusted mail");
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    match run(cli) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("uc mail configure: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<bool> {
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
    let dmarc_policy =
        var::<String>(&vars, "mail_dmarc_policy")?.unwrap_or_else(|| "reject".into());
    if !matches!(dmarc_policy.as_str(), "none" | "quarantine" | "reject") {
        bail!("mail_dmarc_policy is {dmarc_policy}: none, quarantine or reject");
    }
    let opts = Options {
        context,
        host,
        dmarc_policy,
        pinned: var::<String>(&vars, "mail_node")?,
        domains: cli.domains,
        check: cli.check,
        yes: cli.yes,
    };
    tokio::runtime::Runtime::new()?.block_on(configure::run(opts))
}
