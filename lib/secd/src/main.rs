// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Secret manager – SSH pubkey auth + vault-style encrypted storage.
//!
//! Server:     secd serve
//! CLI local:  secd upsert acme prod db --value "secret"
//! CLI remote: secd --remote http://localhost:8000 upsert acme prod db --value "secret"

mod auth;
mod client;
mod keys;
mod server;
mod store;
mod vault;

use std::io::{IsTerminal, Read, Write};

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand, error::ErrorKind};
use serde_json::json;

use crate::client::Signer;

#[derive(Parser)]
#[command(name = "secd", version, verbatim_doc_comment)]
/// Secret manager – SSH pubkey auth + vault-style encrypted storage.
///
/// Secrets live in /var/secrets/<tenant>/<resource_group>/secrets/<name>.
struct Cli {
    /// Remote API base URL (e.g. http://localhost:8000)
    #[arg(long)]
    remote: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Args)]
struct Target {
    tenant: String,
    resource_group: String,
    name: String,
}

#[derive(clap::Args)]
struct KeyOpts {
    /// Vault password (prompted for when omitted)
    #[arg(long)]
    vault_password: Option<String>,
    #[arg(long, default_value = "~/.ssh/passless.pub")]
    pubkey: String,
    #[arg(long, default_value = "~/.ssh/passless")]
    privkey: String,
}

#[derive(Subcommand)]
enum Command {
    /// Encrypt and store a secret.
    Upsert {
        #[command(flatten)]
        target: Target,
        /// Secret value (or read from stdin)
        #[arg(short, long)]
        value: Option<String>,
        #[command(flatten)]
        keys: KeyOpts,
    },
    /// Read and decrypt a secret.
    Read {
        #[command(flatten)]
        target: Target,
        #[command(flatten)]
        keys: KeyOpts,
    },
    /// Run the HTTP API server
    Serve,
}

/// Exit with a clap usage error for subcommand `sub`.
fn usage_error(sub: &str, msg: &str) -> ! {
    let mut cmd = Cli::command();
    let sub = cmd
        .find_subcommand_mut(sub)
        .expect("subcommand exists")
        .clone()
        .bin_name(format!("secd {sub}"));
    let mut sub = sub;
    sub.error(ErrorKind::ValueValidation, msg).exit()
}

/// click-style hidden prompt; re-asks on empty input.
fn vault_password(given: Option<String>) -> Result<String> {
    if let Some(p) = given {
        return Ok(p);
    }
    loop {
        let p = rpassword::prompt_password("Vault password: ").context("reading vault password")?;
        if !p.is_empty() {
            return Ok(p);
        }
    }
}

/// `click.secho(msg, fg=...)`: colored only when the stream is a terminal.
fn secho(msg: &str, color: u8, stderr: bool) {
    let tty = if stderr {
        std::io::stderr().is_terminal()
    } else {
        std::io::stdout().is_terminal()
    };
    let line = if tty {
        format!("\x1b[{color}m{msg}\x1b[0m\n")
    } else {
        format!("{msg}\n")
    };
    if stderr {
        let _ = std::io::stderr().write_all(line.as_bytes());
    } else {
        let _ = std::io::stdout().write_all(line.as_bytes());
    }
}

const GREEN: u8 = 32;
const RED: u8 = 31;

fn upsert(remote: Option<&str>, t: Target, value: Option<String>, k: KeyOpts) -> Result<i32> {
    let password = vault_password(k.vault_password)?;

    let value = match value {
        Some(v) => v,
        None => {
            if std::io::stdin().is_terminal() {
                usage_error("upsert", "Provide --value or pipe input");
            }
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            s.trim_end_matches('\n').to_string()
        }
    };
    if value.is_empty() {
        usage_error("upsert", "Secret value cannot be empty");
    }

    match remote {
        Some(remote) => {
            let signer = Signer {
                tenant: &t.tenant,
                pubkey: &k.pubkey,
                privkey: &k.privkey,
            };
            let payload = json!({
                "resource_group": t.resource_group,
                "name": t.name,
                "value": value,
                "vault_password": password,
            });
            let r = client::call(remote, &signer, &t.resource_group, "/upsert", &payload)?;
            println!("{}", client::py_repr(&r));
        }
        None => {
            let path = store::secret_path(&t.tenant, &t.resource_group, &t.name)
                .map_err(anyhow::Error::msg)?;
            store::write_secret(&path, &value, &password)?;
            secho(&format!("Secret saved → {path}"), GREEN, false);
        }
    }
    Ok(0)
}

fn read(remote: Option<&str>, t: Target, k: KeyOpts) -> Result<i32> {
    let password = vault_password(k.vault_password)?;

    match remote {
        Some(remote) => {
            let signer = Signer {
                tenant: &t.tenant,
                pubkey: &k.pubkey,
                privkey: &k.privkey,
            };
            let payload = json!({
                "resource_group": t.resource_group,
                "name": t.name,
                "vault_password": password,
            });
            let r = client::call(remote, &signer, &t.resource_group, "/read", &payload)?;
            match r.get("value") {
                Some(serde_json::Value::String(s)) => println!("{s}"),
                Some(v) => println!("{}", client::py_repr(v)),
                None => anyhow::bail!("KeyError: 'value'"),
            }
            Ok(0)
        }
        None => {
            let path = store::secret_path(&t.tenant, &t.resource_group, &t.name)
                .map_err(anyhow::Error::msg)?;
            if !store::exists(&path) {
                secho(&format!("Not found: {path}"), RED, true);
                return Ok(1);
            }
            let text = std::fs::read_to_string(&path).with_context(|| format!("reading {path}"))?;
            match vault::decrypt(&text, &password) {
                Ok(v) => {
                    println!("{v}");
                    Ok(0)
                }
                Err(e) => {
                    secho(&format!("Decryption failed: {e}"), RED, true);
                    Ok(1)
                }
            }
        }
    }
}

fn run(cli: Cli) -> Result<i32> {
    let remote = cli.remote.as_deref().filter(|r| !r.is_empty());
    match cli.command {
        Command::Upsert {
            target,
            value,
            keys,
        } => upsert(remote, target, value, keys),
        Command::Read { target, keys } => read(remote, target, keys),
        Command::Serve => {
            println!("Starting server → http://0.0.0.0:8000");
            server::serve("0.0.0.0:8000")?;
            Ok(0)
        }
    }
}

fn main() {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("Error: {e:#}");
            std::process::exit(1);
        }
    }
}
