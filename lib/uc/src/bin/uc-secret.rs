// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-secret` — generate secrets and manage the ansible-vault secret stores.
//!
//! Installed under the names of the tools it replaces (`getsecret`,
//! `keychain`, `rchain`, `ghchain`, `ensure_vault`, `ensure_secret`,
//! `rotate_keychain_pass`), it behaves as they did. `encrypt`,
//! `decrypt`, `server` and `serve` hand over to `uc-secret-encrypt`,
//! `uc-secret-decrypt` and `uc-secret-server`.

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use uc::password::{self, Charset};
use uc::secret::{self, Store};
use uc::{backup, cli, dispatch};
use zeroize::Zeroizing;

const SUMMARY: &str =
    "generate secrets, manage the ansible-vault secret stores and back up SSH and GPG keys";

/// Generate secrets and manage the ansible-vault secret stores.
///
/// A store is `<name>.yaml`, encrypted with ansible-vault, next to its
/// password `<name>.vault.asc`, encrypted with gpg to `gpg.key`, in
/// ~/src/github.com/$USER/config/secrets. Without a name the default store
/// (`secrets.yaml`) is used; with --repo the current repository's
/// (`<org>_<repo>`), or `<org>_<repo>_<name>` when a name is given too.
///
/// With a profile active (see `uc profile`), the stores are the profile's
/// own, in secrets/<profile>, encrypted to its own GPG key.
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
    /// Save every user's ~/.ssh and ~/.gnupg and the SSH host keys to a
    /// password encrypted tar.gz, with their owners and modes. Run as root
    /// to take them all; otherwise only what is readable is taken.
    Backup {
        /// The archive to write [default:
        /// <hostname>-secrets-<YYYYMMDD>.tar.gz.asc].
        file: Option<PathBuf>,

        #[command(flatten)]
        kinds: KindFlags,

        /// Write a plain tar.gz, without asking for a password.
        #[arg(long)]
        no_password: bool,

        /// Take the secrets of the system mounted at DIR instead of /.
        #[arg(long, value_name = "DIR", default_value = "/")]
        root: PathBuf,
    },
    /// Put the secrets of a `uc secret backup` back in place, with the same
    /// modes and owners (by name, else by id). Existing files are kept. An
    /// encrypted backup asks for its password.
    Restore {
        /// The archive to restore.
        file: PathBuf,

        #[command(flatten)]
        kinds: KindFlags,

        /// Replace existing files.
        #[arg(short, long)]
        force: bool,

        /// Restore into the system mounted at DIR instead of /.
        #[arg(long, value_name = "DIR", default_value = "/")]
        root: PathBuf,
    },
    /// Encrypt a file to <file>.asc with a password and remove the original.
    #[command(disable_help_flag = true)]
    Encrypt {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Decrypt a <file>.asc written by `uc secret encrypt`, keeping it.
    #[command(disable_help_flag = true)]
    Decrypt {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// The SSH key authenticated secret manager: upsert, read and serve
    /// secrets (was `secd`).
    #[command(disable_help_flag = true)]
    Server {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Serve the SSH key authenticated secret manager over HTTP (same as
    /// `uc secret server serve`).
    #[command(disable_help_flag = true)]
    Serve {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
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

    fn store(&self) -> anyhow::Result<Store> {
        Store::open(&self.resolve())
    }
}

#[derive(Args)]
struct KindFlags {
    /// Leave the SSH keys out.
    #[arg(long)]
    no_ssh: bool,

    /// Leave the GPG keys out.
    #[arg(long)]
    no_gpg: bool,
}

impl KindFlags {
    fn kinds(&self) -> backup::Kinds {
        backup::Kinds {
            ssh: !self.no_ssh,
            gpg: !self.no_gpg,
        }
    }
}

/// Asks twice for the password to encrypt a backup with.
fn new_password() -> anyhow::Result<Zeroizing<String>> {
    let password = cli::prompt_password("enter backup password: ")?;
    if password.is_empty() {
        anyhow::bail!("empty password; use --no-password for a plain archive");
    }
    if *password != *cli::prompt_password("verify backup password: ")? {
        anyhow::bail!("passwords do not match");
    }
    Ok(password)
}

/// Prints what a backup or restore skipped.
fn warn(report: &backup::Report) {
    for warning in &report.warnings {
        eprintln!("uc-secret: {warning}");
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum Print {
    /// The secret file's path.
    File,
    /// The name the password is kept under, `<name>.asc`.
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

fn run(cmd: Cmd) -> anyhow::Result<ExitCode> {
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
            let secrets = name.store()?.get()?;
            match field {
                None => println!("{secrets}"),
                Some(path) => match secret::field(&secrets, &path) {
                    Some(value) => println!("{}", secret::raw(value)),
                    None => anyhow::bail!("no field '{path}'"),
                },
            }
        }
        Cmd::Edit { name } => name.store()?.edit()?,
        Cmd::Ensure { name, print } => {
            let store = name.store()?;
            store.ensure()?;
            match print {
                Print::File => println!("{}", store.secret_file.display()),
                Print::Vault => println!("{}", store.vault_name),
            }
        }
        Cmd::Rotate { name } => name.store()?.rotate()?,
        Cmd::Backup {
            file,
            kinds,
            no_password,
            root,
        } => {
            let password = if no_password {
                None
            } else {
                Some(new_password()?)
            };
            let file = file.unwrap_or_else(|| backup::default_name(password.is_some()).into());
            let password = password.as_ref().map(|p| p.as_bytes());
            let report = backup::backup(&root, kinds.kinds(), &file, password)?;
            warn(&report);
            println!(
                "Saved {} entries ({}) to {}",
                report.done.len(),
                backup::counts(&report.done),
                file.display()
            );
        }
        Cmd::Restore {
            file,
            kinds,
            force,
            root,
        } => {
            let report = backup::restore(&file, &root, kinds.kinds(), force, || {
                Ok(cli::prompt_password("enter backup password: ")?)
            })?;
            warn(&report);
            for path in &report.done {
                println!("/{}", path.display());
            }
            if !report.kept.is_empty() {
                eprintln!(
                    "uc-secret: kept {} existing files; use -f to replace them",
                    report.kept.len()
                );
            }
            println!(
                "Restored {} entries ({})",
                report.done.len(),
                backup::counts(&report.done)
            );
        }
        Cmd::Encrypt { args } => return Ok(dispatch::exec("uc-secret-encrypt", args)),
        Cmd::Decrypt { args } => return Ok(dispatch::exec("uc-secret-decrypt", args)),
        Cmd::Server { args } => return Ok(dispatch::exec("uc-secret-server", args)),
        Cmd::Serve { args } => {
            return Ok(dispatch::exec(
                "uc-secret-server",
                ["serve".into()].into_iter().chain(args),
            ));
        }
    }
    Ok(ExitCode::SUCCESS)
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
        Ok(code) => code,
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

    #[test]
    fn handovers_keep_every_argument() {
        for (sub, words) in [
            ("encrypt", vec!["-f", "a.txt"]),
            ("decrypt", vec!["-h"]),
            ("server", vec!["read", "-h"]),
            ("serve", vec!["--help"]),
        ] {
            let mut argv = vec!["uc-secret", sub];
            argv.extend(&words);
            let args = match Cli::try_parse_from(&argv).unwrap().command {
                Cmd::Encrypt { args }
                | Cmd::Decrypt { args }
                | Cmd::Server { args }
                | Cmd::Serve { args } => args,
                _ => panic!("{sub}"),
            };
            assert_eq!(args, words, "{sub}");
        }
    }
}
