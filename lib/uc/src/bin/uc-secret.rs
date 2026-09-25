// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-secret` — generate secrets and manage the ansible-vault secret stores.
//!
//! Installed under the names of the tools it replaces (`getsecret`,
//! `keychain`, `rchain`, `ghchain`, `ensure_vault`, `ensure_secret`,
//! `rotate_keychain_pass`), it behaves as they did.

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;
use uc::password::{self, Charset};
use uc::secret::{self, Store};

const SUMMARY: &str = "generate secrets and manage the ansible-vault secret stores";

/// Generate secrets and manage the ansible-vault secret stores.
///
/// A store is `<name>.yaml`, encrypted with ansible-vault, next to its
/// password `<name>.vault.asc`, kept by the `vault` command, in
/// ~/src/github.com/$USER/config/secrets. Without a name the default store
/// (`secrets.yaml`) is used; with --repo the current repository's
/// (`<org>_<repo>`), or `<org>_<repo>_<name>` when a name is given too.
#[derive(Parser)]
#[command(name = "uc secret", bin_name = "uc secret", version)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print a random password drawn from the OS random source.
    Gen {
        /// Length of the password.
        #[arg(short, long, default_value_t = 32)]
        length: usize,

        /// Leave letters out.
        #[arg(long)]
        no_letters: bool,

        /// Leave digits out.
        #[arg(long)]
        no_digits: bool,

        /// Leave symbols out.
        #[arg(long)]
        no_symbols: bool,
    },
    /// Print a store's secrets as JSON (was `getsecret`).
    Get {
        #[command(flatten)]
        name: Name,

        /// Print only this field, as `jq -r .FIELD` would (e.g. `hetzner.prod`).
        #[arg(short, long)]
        field: Option<String>,
    },
    /// Edit a store in $EDITOR, creating it if needed (was `keychain`,
    /// `rchain`, `ghchain`).
    Edit {
        #[command(flatten)]
        name: Name,
    },
    /// Create a store's password and secret file if missing, then print the
    /// secret file's path (was `ensure_secret`, `ensure_vault`).
    Ensure {
        #[command(flatten)]
        name: Name,

        /// What to print once the store exists.
        #[arg(long, value_enum, default_value_t = Print::File)]
        print: Print,
    },
    /// Re-encrypt a store under a new random password (was
    /// `rotate_keychain_pass`).
    Rotate {
        #[command(flatten)]
        name: Name,
    },
}

#[derive(Args)]
struct Name {
    /// Use the current repository's store (`<org>_<repo>[_NAME]`).
    #[arg(short, long)]
    repo: bool,

    /// Store name; with --repo, a suffix to the repository's name.
    name: Option<String>,
}

impl Name {
    fn resolve(&self) -> String {
        let name = self.name.clone().unwrap_or_default();
        match (self.repo, name.is_empty()) {
            (false, _) => name,
            (true, true) => uc::repo::fqn(),
            (true, false) => format!("{}_{name}", uc::repo::fqn()),
        }
    }

    fn store(&self) -> Store {
        Store::new(&secret::dir(), &self.resolve())
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum Print {
    /// The secret file's path.
    File,
    /// The name the password is kept under by `vault`.
    Vault,
}

/// The `uc secret` arguments a legacy tool name stands for.
fn legacy(program: &str) -> Option<&'static [&'static str]> {
    Some(match program {
        "getsecret" => &["get"],
        "keychain" => &["edit"],
        "rchain" => &["edit", "--repo"],
        "ghchain" => &["edit", "--repo", "github"],
        "ensure_secret" => &["ensure", "--repo", "--print", "file"],
        "ensure_vault" => &["ensure", "--repo", "--print", "vault"],
        "rotate_keychain_pass" => &["rotate"],
        _ => return None,
    })
}

/// The command line with a legacy program name expanded into its
/// subcommand. The legacy tools took at most a store name, which follows.
fn args() -> Vec<OsString> {
    let mut args: Vec<OsString> = std::env::args_os().collect();
    let program = args
        .first()
        .and_then(|a| Path::new(a).file_name())
        .and_then(|a| a.to_str())
        .unwrap_or_default()
        .to_string();
    if let Some(expansion) = legacy(&program) {
        let rest = args.split_off(1);
        args.extend(expansion.iter().map(OsString::from));
        args.extend(rest);
    }
    args
}

fn run(cmd: Cmd) -> anyhow::Result<()> {
    match cmd {
        Cmd::Gen {
            length,
            no_letters,
            no_digits,
            no_symbols,
        } => {
            let charset = Charset {
                letters: !no_letters,
                digits: !no_digits,
                symbols: !no_symbols,
            };
            println!("{}", password::generate(length, charset)?);
        }
        Cmd::Get { name, field } => {
            let secrets = name.store().get()?;
            match field {
                None => println!("{secrets}"),
                Some(path) => match secret::field(&secrets, &path) {
                    Some(value) => println!("{}", secret::raw(value)),
                    None => anyhow::bail!("no field '{path}'"),
                },
            }
        }
        Cmd::Edit { name } => name.store().edit()?,
        Cmd::Ensure { name, print } => {
            let store = name.store();
            store.ensure()?;
            match print {
                Print::File => println!("{}", store.secret_file.display()),
                Print::Vault => println!("{}", store.vault_name),
            }
        }
        Cmd::Rotate { name } => name.store().rotate()?,
    }
    Ok(())
}

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("{SUMMARY}");
        return ExitCode::SUCCESS;
    }
    // The secrets are only ever edited in vi, which on these machines is the
    // hardened lib/vi: no plugins, backups or swap files holding plaintext.
    // SAFETY: no other threads exist yet.
    unsafe { std::env::set_var("EDITOR", "vi") };
    match run(Cli::parse_from(args()).command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc-secret: {err:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_cli_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn every_legacy_expansion_parses() {
        for program in [
            "getsecret",
            "keychain",
            "rchain",
            "ghchain",
            "ensure_secret",
            "ensure_vault",
            "rotate_keychain_pass",
        ] {
            let mut args = vec!["uc-secret"];
            args.extend(legacy(program).unwrap());
            assert!(Cli::try_parse_from(&args).is_ok(), "{program}");
        }
        assert!(legacy("uc-secret").is_none());
    }
}
