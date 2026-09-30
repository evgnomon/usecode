// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `usecoded`: the usecode daemon, the one binary on every host. See
//! [`uc_daemon::daemon`] for what `run` does.

use std::process::ExitCode;

use uc_daemon::daemon::control;
use uc_daemon::{bail, daemon, error::Result, setup};

const USAGE: &str = "usage: usecoded COMMAND

  run           run the daemon (what usecode.service starts)
  setup         install this binary and usecode.service on this host
  join BUNDLE   take what the control node delivered and reload
  reload        have the running daemon converge every module now,
                and print how each one did
  version       print the version

setup and join are run for you by `uc daemon install` and
`uc net mesh apply`; they need root. reload talks to the daemon's
control socket, which is root's too, so over ssh it's
`ssh root@HOST usecoded reload`.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("usecoded: error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<()> {
    let strs: Vec<&str> = args.iter().map(String::as_str).collect();
    match strs.as_slice() {
        ["run"] => daemon::run(),
        ["setup"] => {
            require_root()?;
            setup::run()
        }
        ["join", bundle] => {
            require_root()?;
            setup::join(bundle)
        }
        ["reload"] => {
            let answer = control::request(control::SOCKET_PATH, "reload")?;
            print!("{answer}");
            if control::failed(&answer) {
                bail!("reload did not go cleanly (journalctl -u usecode has the details)");
            }
            Ok(())
        }
        ["version" | "-v" | "--version"] => {
            println!("usecoded {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        ["help" | "-h" | "--help"] | [] => {
            println!("{USAGE}");
            Ok(())
        }
        _ => bail!("{USAGE}"),
    }
}

fn require_root() -> Result<()> {
    // SAFETY: geteuid is always safe to call and cannot fail.
    if unsafe { libc::geteuid() } != 0 {
        bail!("this command must be run as root");
    }
    Ok(())
}
