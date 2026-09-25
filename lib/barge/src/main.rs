// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Run the `barge` container against the current directory.
//! Arguments are ignored, as in the original script.

use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, exit};

fn main() {
    let err = Command::new("docker")
        .args([
            "run",
            "-it",
            "--rm",
            "-v.:/github/workspace",
            "--workdir",
            "/github/workspace",
            "barge",
        ])
        .exec();
    eprintln!("barge: docker: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}
