// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Print only the A records of a DNS lookup.

use std::env;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, exit};

const USAGE: &str = "usage: uc net dig [@SERVER] NAME [DIG OPTIONS...]

Prints only the A records of a DNS lookup: dig +noall +answer -t A, with
the arguments given passed on to dig.

  uc net dig example.com
  uc net dig @1.1.1.1 example.com
  uc net dig example.com +short";

fn main() {
    if env::args().skip(1).any(|arg| arg == "-h" || arg == "--help") {
        println!("{USAGE}");
        return;
    }
    let err = Command::new("dig")
        .args(["+noall", "+answer", "-t", "A"])
        .args(env::args_os().skip(1))
        .exec();
    eprintln!("uc-net-dig: dig: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}
