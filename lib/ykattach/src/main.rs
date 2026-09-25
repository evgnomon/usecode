// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! WSL: attach the YubiKey (1050:0407) via usbipd, then show lsusb and
//! `ykman info`. Exits with the status of `ykman info`.

use std::os::unix::process::ExitStatusExt;
use std::process::{Command, exit};

fn run(prog: &str, args: &[&str]) -> i32 {
    match Command::new(prog).args(args).status() {
        Ok(s) => s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("ykattach: {prog}: not found");
            127
        }
        Err(e) => {
            eprintln!("ykattach: {prog}: {e}");
            126
        }
    }
}

fn main() {
    run("powershell.exe", &["-c", "usbipd attach -i 1050:0407 -w"]);
    run("lsusb", &[]);
    exit(run("ykman", &["info"]));
}
