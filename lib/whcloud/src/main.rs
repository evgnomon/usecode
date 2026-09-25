// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Run hcloud with its config generated on the fly by
//! `~/.config/hcloud/cli.sh`, passed through a `/dev/fd` pipe
//! (bash `--config <(~/.config/hcloud/cli.sh)`).

use std::env;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, exit};

fn main() {
    let script = format!(
        "{}/.config/hcloud/cli.sh",
        env::var("HOME").unwrap_or_default()
    );
    let (r, w) = match io::pipe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("whcloud: pipe: {e}");
            exit(1);
        }
    };
    // The producer keeps running (and is reparented) after we exec hcloud.
    // On failure hcloud simply reads an empty config, as with bash.
    let mut producer = Command::new(&script);
    producer.stdout(w);
    if let Err(e) = producer.spawn() {
        eprintln!("whcloud: {script}: {e}");
    }
    drop(producer);
    // SAFETY: fcntl on a descriptor we own; clears FD_CLOEXEC so hcloud inherits it.
    if unsafe { libc::fcntl(r.as_raw_fd(), libc::F_SETFD, 0) } == -1 {
        eprintln!("whcloud: fcntl: {}", io::Error::last_os_error());
        exit(1);
    }
    let err = Command::new("hcloud")
        .arg("--config")
        .arg(format!("/dev/fd/{}", r.as_raw_fd()))
        .args(env::args_os().skip(1))
        .exec();
    eprintln!("whcloud: hcloud: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}
