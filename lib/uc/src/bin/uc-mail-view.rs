// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-mail-view`: `uc mail view`, see [`uc::mail::inbox`].

use anyhow::Result;
use clap::Parser;
use std::io::{IsTerminal, Write};
use std::process::{Command, ExitCode, Stdio};
use uc::mail::inbox::{self, Entry};

/// Read the mail `uc mail pull` fetched, the newest first.
///
/// On a terminal it lists the mail in skim: type to narrow it down, read
/// the one under the cursor below, Tab to pick more, Enter to open them.
/// With -n, --full, --json, ids, or without a terminal it only prints.
#[derive(Parser)]
#[command(name = "uc mail view", bin_name = "uc mail view", version, after_help = EXAMPLES)]
struct Cli {
    /// Print these messages, e.g. work/INBOX/1-42.
    #[arg(value_name = "ID")]
    ids: Vec<String>,
    /// Only the newest N.
    #[arg(short = 'n', long, value_name = "N")]
    limit: Option<usize>,
    /// Only this account's.
    #[arg(short, long, value_name = "NAME")]
    account: Option<String>,
    /// Print the messages whole, not one line each.
    #[arg(short, long)]
    full: bool,
    /// Print JSON: one object per message, with its text under "text"
    /// with --full or ids.
    #[arg(long)]
    json: bool,
}

const EXAMPLES: &str = "\
Examples:
  uc mail view                       pick and read with skim
  uc mail view -n 10                 the 10 newest, one line each
  uc mail view -n 10 --full          the 10 newest, whole
  uc mail view -n 10 --full --json   the same as JSON, for scripts and agents
  uc mail view -a work -n 5
  uc mail view work/INBOX/1-42       one message, by the id a listing shows

Lines are: received (local time), id, sender, subject, tab-separated.";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("read the mail on this machine, the newest first");
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
    let mut entries = inbox::entries(cli.account.as_deref())?;
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
    let interactive = cli.limit.is_none()
        && !cli.full
        && !cli.json
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal();
    if !interactive {
        let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
        return print(&ids, &entries, cli.full, cli.json);
    }
    if entries.is_empty() {
        eprintln!("No mail here yet, `uc mail pull` fetches it.");
        return Ok(());
    }
    let picked = inbox::pick(entries);
    if picked.is_empty() {
        return Ok(());
    }
    let text = picked
        .iter()
        .map(|id| inbox::render(id))
        .collect::<Result<Vec<_>>>()?
        .join(&format!("\n{}\n\n", "─".repeat(72)));
    page(&text)
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

/// `text` through $PAGER (less by default), or printed when there is none.
fn page(text: &str) -> Result<()> {
    let pager = std::env::var("PAGER")
        .ok()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| "less".into());
    let child = Command::new("sh")
        .args(["-c", &pager])
        .env(
            "LESS",
            std::env::var("LESS").unwrap_or_else(|_| "FRX".into()),
        )
        .stdin(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        println!("{text}");
        return Ok(());
    };
    if let Some(mut stdin) = child.stdin.take() {
        // The pager may quit before reading it all.
        let _ = writeln!(stdin, "{text}");
    }
    child.wait()?;
    Ok(())
}
