// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Hardened vim: no config, no plugins, no swap/backup files, no modelines.

use std::env;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, exit};

const VIM_ARGS: &[&str] = &[
    "-Z",
    "-u",
    "NONE",
    "-U",
    "NONE",
    "--noplugin",
    "-c",
    "set secure",
    "-c",
    "set nobackup",
    "-c",
    "set nowritebackup",
    "-c",
    "set nomodeline",
    "-c",
    "set nocompatible",
];

fn main() {
    let err = Command::new("vim")
        .args(VIM_ARGS)
        .args(env::args_os().skip(1))
        .exec();
    eprintln!("vi: vim: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}

#[cfg(test)]
mod tests {
    use super::VIM_ARGS;

    #[test]
    fn restricted_mode_first() {
        assert_eq!(VIM_ARGS[0], "-Z");
        assert!(VIM_ARGS.contains(&"set secure"));
    }
}
