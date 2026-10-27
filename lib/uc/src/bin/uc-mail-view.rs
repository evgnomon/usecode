// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-mail-view`: `uc mail view`, see [`uc::mail::inbox`] and
//! [`uc::mail::compose`].

use anyhow::{Context, Result, bail};
use clap::Parser;
use std::fs;
use std::io::{IsTerminal, Write};
use std::process::ExitCode;
use uc::mail::compose::{self, Compose, Draft};
use uc::mail::inbox::{self, Config, Entry};

/// Read the mail `uc mail pull` fetched, the newest first, and reply to it.
///
/// On a terminal it lists the mail in the picker: type to narrow it down, read
/// the one under the cursor below, Tab to pick more. Enter opens them in
/// $EDITOR to read, Ctrl-R replies to the one picked.
/// With -n, --full, --json, ids, or without a terminal it only prints.
///
/// With --reply it replies to the id given (or the one picked), to the
/// sender or, with --all, to everyone on it. Give the reply with --body,
/// --body-file or on stdin and it is sent right away, the mail quoted below
/// it. Leave it out on a terminal (or add --edit) and the reply opens in
/// $EDITOR, then asks before sending.
#[derive(Parser)]
#[command(name = "uc mail view", bin_name = "uc mail view", version, after_help = EXAMPLES)]
struct Cli {
    /// Print these messages, e.g. work/INBOX/1-42.
    #[arg(value_name = "ID")]
    ids: Vec<String>,
    /// Only the newest N.
    #[arg(short = 'n', long, value_name = "N")]
    limit: Option<usize>,
    /// Print the messages whole, not one line each.
    #[arg(long)]
    full: bool,
    /// Print JSON: one object per message, with its text under "text"
    /// with --full or ids.
    #[arg(long)]
    json: bool,
    /// Fetch the new mail first, like `uc mail pull`; with --account,
    /// only that account's.
    #[arg(short, long)]
    pull: bool,
    /// Reply to the mail, not read it.
    #[arg(short, long, help_heading = "Reply")]
    reply: bool,
    /// Reply to everyone it went to, not only the sender.
    #[arg(long, help_heading = "Reply")]
    all: bool,
    /// Leave the mail out of the reply.
    #[arg(long, help_heading = "Reply")]
    no_quote: bool,
    #[command(flatten, next_help_heading = "Reply")]
    compose: Compose,
}

const EXAMPLES: &str = "\
Examples:
  uc mail view                       pick with the picker: Enter reads, Ctrl-R replies
  uc mail view -p                    pull the new mail first, then pick
  uc mail view -n 10                 the 10 newest, one line each
  uc mail view -n 10 --full          the 10 newest, whole
  uc mail view -n 10 --full --json   the same as JSON, for scripts and agents
  uc mail view -a work -n 5
  uc mail view work/INBOX/1-42       one message, by the id a listing shows
  uc mail view -r                    pick the mail, write the reply in $EDITOR
  uc mail view -r work/INBOX/1-42 --all
  uc mail view -r work/INBOX/1-42 -b 'Thanks, done.' -A fix.patch
  echo 'Sounds good.' | uc mail view -r work/INBOX/1-42 --all --cc joe@example.com

Lines are: received (local time), id, sender, subject, tab-separated.
A reply goes out from the account the mail came to, unless --account says
otherwise.";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("read the mail on this machine, the newest first, and reply to it");
        return ExitCode::SUCCESS;
    }
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc mail view: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let terminal = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    if cli.pull {
        let accounts: Vec<String> = cli.compose.account.iter().cloned().collect();
        tokio::runtime::Runtime::new()?.block_on(inbox::pull(&accounts, None))?;
    }
    if cli.reply {
        let id = match cli.ids.as_slice() {
            [id] => id.clone(),
            [] if terminal => match pick(&cli)? {
                Some((ids, _)) => one(ids)?,
                None => return Ok(()),
            },
            [] => bail!("which mail? Give its id, `uc mail view -n 10` shows them"),
            _ => bail!("give one mail to reply to"),
        };
        return reply(&cli, &id);
    }
    let mut entries = inbox::entries(cli.compose.account.as_deref())?;
    if !cli.ids.is_empty() {
        entries.retain(|e| cli.ids.contains(&e.id));
        for id in &cli.ids {
            if !entries.iter().any(|e| &e.id == id) {
                // Not indexed, but the file may be there.
                inbox::raw(id)?;
            }
        }
        return print(&cli.ids, &entries, true, cli.json);
    }
    if let Some(n) = cli.limit {
        entries.truncate(n);
    }
    if cli.full || cli.json || cli.limit.is_some() || !terminal {
        let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
        return print(&ids, &entries, cli.full, cli.json);
    }
    match pick(&cli)? {
        Some((ids, true)) => reply(&cli, &one(ids)?),
        Some((ids, false)) => read(&ids),
        None => Ok(()),
    }
}

/// The mail picked with the picker and whether to reply to it, `None` when none
/// was.
fn pick(cli: &Cli) -> Result<Option<(Vec<String>, bool)>> {
    let entries = inbox::entries(cli.compose.account.as_deref())?;
    if entries.is_empty() {
        eprintln!("No mail here yet, `uc mail pull` fetches it.");
        return Ok(None);
    }
    let picked = inbox::pick(entries);
    if picked.ids.is_empty() {
        return Ok(None);
    }
    Ok(Some((picked.ids, picked.reply || cli.reply)))
}

fn one(ids: Vec<String>) -> Result<String> {
    match <[String; 1]>::try_from(ids) {
        Ok([id]) => Ok(id),
        Err(_) => bail!("pick one mail to reply to"),
    }
}

/// Replies to `id` as the flags say.
fn reply(cli: &Cli, id: &str) -> Result<()> {
    let config = Config::load()?;
    let account = compose::account(&config, cli.compose.account.as_deref(), Some(id))?;
    let mut draft = Draft::reply(id, &compose::sender(account)?, cli.all)?;
    if cli.no_quote {
        draft.body.clear();
    }
    tokio::runtime::Runtime::new()?.block_on(compose::run(account, draft, &cli.compose))
}

/// Opens the messages `ids` in $EDITOR, to read; the file is gone after.
fn read(ids: &[String]) -> Result<()> {
    let text = ids
        .iter()
        .map(|id| inbox::render(id))
        .collect::<Result<Vec<_>>>()?
        .join(&format!("\n{}\n\n", "─".repeat(72)));
    let path = std::env::temp_dir().join(format!("uc-mail-{}.txt", std::process::id()));
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    let mut perms = fs::metadata(&path)?.permissions();
    perms.set_readonly(true);
    fs::set_permissions(&path, perms)?;
    let read = compose::editor(&path);
    fs::remove_file(&path).ok();
    read
}

/// The messages `ids` as lines, whole, or as JSON.
fn print(ids: &[String], entries: &[Entry], full: bool, json: bool) -> Result<()> {
    let mut out = std::io::stdout().lock();
    let entry = |id: &String| entries.iter().find(|e| &e.id == id);
    if json {
        let messages = ids
            .iter()
            .map(|id| {
                let mut value = match entry(id) {
                    Some(e) => serde_json::json!({
                        "id": e.id,
                        "account": e.account,
                        "mailbox": e.mailbox,
                        "received": chrono::DateTime::from_timestamp(e.received, 0)
                            .map(|t| t.to_rfc3339()),
                        "from": e.from,
                        "subject": e.subject,
                    }),
                    None => serde_json::json!({ "id": id }),
                };
                if full {
                    value["text"] = inbox::render(id)?.into();
                }
                Ok(value)
            })
            .collect::<Result<Vec<_>>>()?;
        writeln!(out, "{}", serde_json::to_string_pretty(&messages)?)?;
    } else if full {
        for (i, id) in ids.iter().enumerate() {
            if i > 0 {
                writeln!(out, "\n{}\n", "─".repeat(72))?;
            }
            writeln!(out, "{}", inbox::render(id)?)?;
        }
    } else {
        for e in ids.iter().filter_map(entry) {
            writeln!(out, "{}\t{}\t{}\t{}", e.date(), e.id, e.from, e.subject)?;
        }
    }
    Ok(())
}
