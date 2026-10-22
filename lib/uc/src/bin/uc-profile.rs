// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc profile`: switch this machine's profile, creating it if needed.

use clap::Parser;
use std::process::ExitCode;
use uc::profile;

const SUMMARY: &str = "switch this machine's profile, creating it if needed";

/// Switch this machine's profile, creating it if needed.
///
/// A profile is a config file, ~/.config/usecode/profiles/NAME.yaml, or a
/// directory, profiles/NAME/, shaped like the config root: its own
/// config.yaml, a profile.yaml and profiles/. The active one is linked from
/// profile.yaml and laid over config.yaml, overriding any of its keys.
///
/// Profiles nest: `uc profile main/contract/client-1` switches to client-1
/// inside contract inside main, creating what's missing; each level adds to
/// the one above.
///
/// If the config then names no GPG key this machine has, the outermost new
/// profile gets one, generated after asking for the default git profile's
/// fullname and useremail, and secret stores of its own in secrets/PATH.
/// Without a name, prints the profiles, the active one marked.
#[derive(Parser)]
#[command(name = "uc profile", bin_name = "uc profile", version)]
struct Cli {
    /// The profile to switch to, e.g. `laptop-wsl` or `laptop-wsl/dark`.
    #[arg(conflicts_with = "off")]
    name: Option<String>,

    /// Don't ask before generating a key.
    #[arg(short, long)]
    yes: bool,

    /// Go back to the shared stores.
    #[arg(long)]
    off: bool,
}

fn run(cli: Cli) -> anyhow::Result<()> {
    if cli.off {
        match profile::off()? {
            true => eprintln!("no profile: using the main config"),
            false => eprintln!("no profile was active"),
        }
        return Ok(());
    }
    let Some(name) = cli.name else {
        let active = profile::active()?;
        for name in profile::list()? {
            let mark = if active.as_deref() == Some(&name) {
                "*"
            } else {
                " "
            };
            println!("{mark} {name}");
        }
        if active.is_none() {
            eprintln!("no profile: using the main config");
        }
        return Ok(());
    };
    match profile::switch(&name, cli.yes)? {
        true => eprintln!("created profile {name} and switched to it"),
        false => eprintln!("switched to profile {name}"),
    }
    Ok(())
}

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("{SUMMARY}");
        return ExitCode::SUCCESS;
    }
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc-profile: {err:#}");
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
}
