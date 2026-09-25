// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use std::process;

const RED: &str = "\x1b[0;31m";
const GREEN: &str = "\x1b[0;32m";
const BLUE: &str = "\x1b[0;34m";
const NC: &str = "\x1b[0m";

pub fn info(msg: &str) {
    println!("{BLUE}[INFO]{NC} {msg}");
}

pub fn success(msg: &str) {
    println!("{GREEN}[SUCCESS]{NC} {msg}");
}

pub fn error(msg: &str) {
    eprintln!("{RED}[ERROR]{NC} {msg}");
}

/// Log an error and exit with status 1.
pub fn die(msg: &str) -> ! {
    error(msg);
    process::exit(1)
}
