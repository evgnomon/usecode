// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Run the `barge` container against the current directory.
//! Arguments other than `-h`/`--help` are ignored, as in the original script.

use std::env;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, exit};

const USAGE: &str = "usage: uc image run barge

Runs the barge workflow container on the current directory with docker,
interactively: docker run -it --rm -v .:/github/workspace barge.
It needs a terminal. Arguments are ignored.";

fn main() {
    if env::args().skip(1).any(|arg| arg == "-h" || arg == "--help") {
        println!("{USAGE}");
        return;
    }
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
    eprintln!("uc-image-run-barge: docker: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}
