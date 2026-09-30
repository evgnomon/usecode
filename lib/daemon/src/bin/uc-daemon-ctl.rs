// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc daemon`, run from the control node: `install` (see
//! [`uc_daemon::install`]) and `reload` (see [`uc_daemon::reload`]).

use std::process::ExitCode;

use uc_daemon::{install, reload};

const USAGE: &str = "usage: uc-daemon-ctl install|reload ARGS...";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (name, run): (&str, fn(&[String]) -> uc_daemon::error::Result<()>) =
        match args.first().map(String::as_str) {
            Some("install") => ("install", install::run),
            Some("reload") => ("reload", reload::run),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::FAILURE;
            }
        };
    match run(&args[1..]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("uc daemon {name}: error: {e}");
            ExitCode::FAILURE
        }
    }
}
