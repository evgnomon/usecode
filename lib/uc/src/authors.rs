// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc repo authors`: add contributors from git history to `AUTHORS`.
//!
//! Existing entries are kept as written, so contributors can change how they
//! are credited by editing `AUTHORS`. Only authors whose email is not listed
//! yet are added. Bots and automated agents are skipped.

use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use std::process::Command;

const HEADER: &str = "\
# Contributors to usecode, generated from git history by uc repo authors.
# To be credited under a different name or email, edit your entry.";

fn git(args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn is_bot(entry: &str) -> bool {
    entry.contains("[bot]") || entry.starts_with("Copilot <")
}

/// The new `AUTHORS` for the current one and `git log --format='%aN <%aE>'`.
pub fn merge(current: &str, log: &str) -> String {
    let known = current.to_lowercase();
    let mut entries: Vec<&str> = current
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    entries.extend(log.lines().filter(|entry| {
        let email = entry.rfind('<').map_or(*entry, |at| &entry[at..]);
        !entry.is_empty() && !is_bot(entry) && !known.contains(&email.to_lowercase())
    }));
    // `sort -fu`: ordered and deduplicated ignoring case.
    entries.sort_by_key(|entry| entry.to_uppercase());
    entries.dedup_by_key(|entry| entry.to_uppercase());
    let mut out = format!("{HEADER}\n\n");
    for entry in entries {
        out += entry;
        out.push('\n');
    }
    out
}

/// Updates `AUTHORS` at the top of the current repository.
pub fn update() -> Result<()> {
    let top = PathBuf::from(git(&["rev-parse", "--show-toplevel"])?.trim_end());
    let file = top.join("AUTHORS");
    let current = match std::fs::read_to_string(&file) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        read => read.with_context(|| format!("reading {}", file.display()))?,
    };
    let top = top.to_string_lossy();
    let log = git(&["-C", &top, "log", "--use-mailmap", "--format=%aN <%aE>"])?;
    std::fs::write(&file, merge(&current, &log))
        .with_context(|| format!("writing {}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_entries_and_adds_new_emails_only() {
        let current = "# old header\n\nZed <z@x.org>\nAnn Old <ANN@x.org>\n";
        let log = "Ann New <ann@x.org>\nbob <b@x.org>\nbob <b@x.org>\n\
                   dependabot[bot] <d@x.org>\nCopilot <c@x.org>\n";
        assert_eq!(
            merge(current, log),
            format!("{HEADER}\n\nAnn Old <ANN@x.org>\nbob <b@x.org>\nZed <z@x.org>\n")
        );
    }

    #[test]
    fn starts_an_empty_file() {
        assert_eq!(merge("", "A <a@x>\n"), format!("{HEADER}\n\nA <a@x>\n"));
    }
}
