// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Open the current repository's GitHub keychain (`<org>_<repo>_github`).

mod repofqn;

use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, exit};

fn main() {
    let name = format!("{}_github", repofqn::repofqn());
    let err = Command::new("keychain").arg(name).exec();
    eprintln!("ghchain: keychain: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}
