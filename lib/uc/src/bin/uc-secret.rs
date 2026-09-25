// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-secret` — generate secrets and manage the ansible-vault secret stores.
//!
//! Installed under the names of the tools it replaces (`getsecret`,
//! `keychain`, `rchain`, `ghchain`, `ensure_vault`, `ensure_secret`,
//! `rotate_keychain_pass`, `rotsec`), it behaves as they did. `encrypt`,
//! `decrypt` and `serve` hand over to `uc-encrypt`, `uc-decrypt` and `secd`.

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;
use uc::dispatch;
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
    /// `rotate_keychain_pass`), or with --playbook rotate the secrets
    /// themselves (was `rotsec`).
    Rotate {
        #[command(flatten)]
        name: Name,

        /// Rotate the secrets in the store rather than its password, by
        /// running the blueprint checkout's rotate.yaml playbook with them.
        #[arg(long)]
        playbook: bool,

        /// With --playbook, arguments for ansible-playbook, after `--`.
        #[arg(last = true, requires = "playbook")]
        args: Vec<OsString>,
    },
    /// Encrypt files with a password (same as `uc encrypt`).
    #[command(disable_help_flag = true)]
    Encrypt {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Decrypt files written by `uc secret encrypt` (same as `uc decrypt`).
    #[command(disable_help_flag = true)]
    Decrypt {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Serve the SSH key authenticated secret manager over HTTP (was
    /// `secd serve`).
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
        "rotsec" => &["rotate", "--repo", "--playbook", "--"],
        _ => return None,
    })
}

/// The command line with a legacy program name expanded into its
/// subcommand. The legacy tools took at most a store name, which follows,
/// except `rotsec`, whose arguments go to ansible-playbook.
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
        Cmd::Rotate {
            name,
            playbook: false,
            ..
        } => name.store().rotate()?,
        Cmd::Rotate { name, args, .. } => {
            name.store().rotate_secrets(&secret::blueprint(), &args)?
        }
        Cmd::Encrypt { args } => return Ok(dispatch::exec("uc-encrypt", args)),
        Cmd::Decrypt { args } => return Ok(dispatch::exec("uc-decrypt", args)),
        Cmd::Serve { args } => {
            return Ok(dispatch::exec(
                "secd",
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
            "rotsec",
        ] {
            let mut args = vec!["uc-secret"];
            args.extend(legacy(program).unwrap());
            assert!(Cli::try_parse_from(&args).is_ok(), "{program}");
        }
        assert!(legacy("uc-secret").is_none());
    }

    #[test]
    fn rotsec_arguments_reach_ansible_playbook() {
        let mut args = vec!["uc-secret"];
        args.extend(legacy("rotsec").unwrap());
        args.extend(["-t", "db", "--check"]);
        match Cli::try_parse_from(&args).unwrap().command {
            Cmd::Rotate {
                name,
                playbook,
                args,
            } => {
                assert!(name.repo && name.name.is_none() && playbook);
                assert_eq!(args, ["-t", "db", "--check"]);
            }
            _ => panic!("not rotate"),
        }
        assert!(Cli::try_parse_from(["uc-secret", "rotate", "--", "-v"]).is_err());
    }

    #[test]
    fn handovers_keep_every_argument() {
        for (sub, words) in [
            ("encrypt", vec!["-f", "a.txt"]),
            ("decrypt", vec!["-h"]),
            ("serve", vec!["--help"]),
        ] {
            let mut argv = vec!["uc-secret", sub];
            argv.extend(&words);
            let args = match Cli::try_parse_from(&argv).unwrap().command {
                Cmd::Encrypt { args } | Cmd::Decrypt { args } | Cmd::Serve { args } => args,
                _ => panic!("{sub}"),
            };
            assert_eq!(args, words, "{sub}");
        }
    }
}
