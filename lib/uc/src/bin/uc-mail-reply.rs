// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-mail-reply`: `uc mail reply`, see [`uc::mail::compose`].

use anyhow::{Result, bail};
use clap::Parser;
use std::io::IsTerminal;
use std::process::ExitCode;
use uc::mail::compose::{self, Compose, Draft};
use uc::mail::inbox::{self, Config};

/// Reply to a mail `uc mail pull` fetched, to the sender or, with --all, to
/// everyone on it.
///
/// Without an id, on a terminal, pick the mail with skim. Give the reply
/// with --body, --body-file or on stdin and it is sent right away, the mail
/// quoted below it. Leave it out on a terminal (or add --edit) and the
/// reply opens in $EDITOR, then asks before sending.
#[derive(Parser)]
#[command(name = "uc mail reply", bin_name = "uc mail reply", version, after_help = EXAMPLES)]
struct Cli {
    /// The mail to reply to, e.g. work/INBOX/1-42 [default: pick it].
    #[arg(value_name = "ID")]
    id: Option<String>,
    /// To everyone it went to, not only the sender.
    #[arg(long)]
    all: bool,
    /// Leave the mail out of the reply.
    #[arg(long)]
    no_quote: bool,
    #[command(flatten)]
    compose: Compose,
}

const EXAMPLES: &str = "\
Examples:
  uc mail reply                               pick the mail, write in $EDITOR
  uc mail reply work/INBOX/1-42 --all
  uc mail reply work/INBOX/1-42 -b 'Thanks, done.' -A fix.patch
  echo 'Sounds good.' | uc mail reply work/INBOX/1-42 --all --cc joe@example.com

`uc mail view` shows the ids. It goes out from the account the mail came
to, unless --account says otherwise.";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("reply to a mail, to the sender or to all");
        return ExitCode::SUCCESS;
    }
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc mail reply: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let id = match cli.id {
        Some(id) => id,
        None => pick(cli.compose.account.as_deref())?,
    };
    let config = Config::load()?;
    let account = compose::account(&config, cli.compose.account.as_deref(), Some(&id))?;
    let mut draft = Draft::reply(&id, &compose::sender(account)?, cli.all)?;
    if cli.no_quote {
        draft.body.clear();
    }
    tokio::runtime::Runtime::new()?.block_on(compose::run(account, draft, &cli.compose))
}

/// The mail to reply to, picked with skim.
fn pick(account: Option<&str>) -> Result<String> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("which mail? Give its id, `uc mail view -n 10` shows them");
    }
    let entries = inbox::entries(account)?;
    if entries.is_empty() {
        bail!("no mail here yet, `uc mail pull` fetches it");
    }
    match inbox::pick(entries).as_slice() {
        [] => bail!("no mail picked"),
        [id] => Ok(id.clone()),
        _ => bail!("pick one mail to reply to"),
    }
}
