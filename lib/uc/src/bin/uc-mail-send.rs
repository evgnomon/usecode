// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-mail-send`: `uc mail send`, see [`uc::mail::compose`].

use anyhow::Result;
use clap::Parser;
use std::process::ExitCode;
use uc::mail::compose::{self, Compose, Draft};
use uc::mail::inbox::Config;

/// Write a new mail and send it.
///
/// Give the message with --body, --body-file or on stdin and it is sent
/// right away, for scripts and agents. Leave it out on a terminal (or add
/// --edit) and it opens in $EDITOR, then asks before sending. It goes out
/// over the SMTP server of the account in ~/.config/usecode/mail.yaml.
#[derive(Parser)]
#[command(name = "uc mail send", bin_name = "uc mail send", version, after_help = EXAMPLES)]
struct Cli {
    /// To these, the same as --to.
    #[arg(value_name = "ADDRESS")]
    recipients: Vec<String>,
    #[command(flatten)]
    compose: Compose,
}

const EXAMPLES: &str = "\
Examples:
  uc mail send                                 write it in $EDITOR
  uc mail send ada@example.com -s Hi           the same, To and Subject filled in
  uc mail send ada@example.com -s Report -b 'Here it is.' -A report.pdf
  echo 'Done.' | uc mail send ada@example.com --cc bob@example.com -s Build
  uc mail send -a work --to 'Ada <ada@example.com>' -s Notes -f notes.txt

The account's SMTP settings in ~/.config/usecode/mail.yaml:
  from: Ada Lovelace <ada@example.com>   # default: user
  smtp_host: smtp.example.com            # default: host with imap. made smtp.
  smtp_port: 465                         # 587 for STARTTLS
  sent_mailbox: Sent                     # keep a copy there

It logs in with the same user and password as `uc mail pull`.";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("write a new mail, with attachments, and send it");
        return ExitCode::SUCCESS;
    }
    let mut cli = Cli::parse();
    cli.compose.to.splice(0..0, cli.recipients.drain(..));
    let run = || -> Result<()> {
        let config = Config::load()?;
        let account = compose::account(&config, cli.compose.account.as_deref(), None)?;
        tokio::runtime::Runtime::new()?.block_on(compose::run(
            account,
            Draft::default(),
            &cli.compose,
        ))
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc mail send: {err:#}");
            ExitCode::FAILURE
        }
    }
}
