// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Print only the A records of a DNS lookup.

use std::env;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, exit};

fn main() {
    let err = Command::new("dig")
        .args(["+noall", "+answer", "-t", "A"])
        .args(env::args_os().skip(1))
        .exec();
    eprintln!("diga: dig: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}
