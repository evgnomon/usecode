// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Open the current repository's keychain (`<org>_<repo>`).

mod repofqn;

use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, exit};

fn main() {
    let name = repofqn::repofqn();
    let err = Command::new("keychain").arg(name).exec();
    eprintln!("rchain: keychain: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}
