// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Shorthand for `uc configure`.

use std::env;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, exit};

fn main() {
    let err = Command::new("uc")
        .arg("configure")
        .args(env::args_os().skip(1))
        .exec();
    eprintln!("x: uc: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}
