// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc daemon install`: see [`uc_daemon::install`].

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match uc_daemon::install::run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("uc daemon install: error: {e}");
            ExitCode::FAILURE
        }
    }
}
