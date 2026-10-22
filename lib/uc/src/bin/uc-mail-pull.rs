// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-mail-pull`: `uc mail pull`, see [`uc::mail::inbox`].

use anyhow::Result;
use clap::Parser;
use std::process::ExitCode;
use uc::mail::inbox;

/// Fetch the new mail of your accounts to this machine.
///
/// The accounts are in ~/.config/usecode/mail.yaml (or UC_MAIL_CONFIG), the
/// mail goes to ~/.local/share/usecode/mail (or UC_MAIL_DIR), one .eml file
/// per message. Each pull only fetches what came since the last one, and
/// leaves the mail on the server as it was, unread stays unread.
#[derive(Parser)]
#[command(name = "uc mail pull", bin_name = "uc mail pull", version, after_help = EXAMPLES)]
struct Cli {
    /// Only these accounts [default: all of them].
    #[arg(value_name = "ACCOUNT")]
    accounts: Vec<String>,
    /// Only the newest N new messages of each mailbox; the older new ones
    /// are skipped for good. Handy for a first pull of a big mailbox.
    #[arg(long, value_name = "N")]
    last: Option<usize>,
}

const EXAMPLES: &str = "\
Examples:
  uc mail pull
  uc mail pull work
  uc mail pull work --last 200

~/.config/usecode/mail.yaml:
  accounts:
    - name: work
      host: imap.example.com
      user: ada@example.com
      password_secret: mail.work   # a path in `uc secret`
      # port: 993
      # mailboxes: [INBOX, Archive]

The password is UC_MAIL_PASSWORD_<ACCOUNT> (e.g. UC_MAIL_PASSWORD_WORK),
else password_secret in the `uc secret` store (secret_store: <name> for a
named one), else asked for. `uc mail view` reads what was pulled.";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("fetch new mail from your accounts to this machine");
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    let run = || -> Result<()> {
        tokio::runtime::Runtime::new()?.block_on(inbox::pull(&cli.accounts, cli.last))
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc mail pull: {err:#}");
            ExitCode::FAILURE
        }
    }
}
