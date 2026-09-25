// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Thin wrappers around the `podman` CLI.

use std::io::Write;
use std::process::{Command, Output};

/// Run podman with captured output. Exits the process if podman cannot be
/// started at all.
pub fn capture(args: &[&str]) -> Output {
    match Command::new("podman").args(args).output() {
        Ok(out) => out,
        Err(e) => {
            eprintln!("Error: failed to run podman: {e}");
            std::process::exit(1);
        }
    }
}

/// Run podman with inherited stdio; returns true on success.
fn inherit(args: &[&str]) -> bool {
    let _ = std::io::stdout().flush();
    match Command::new("podman").args(args).status() {
        Ok(status) => status.success(),
        Err(e) => {
            eprintln!("Error: failed to run podman: {e}");
            false
        }
    }
}

fn ok(args: &[&str]) -> bool {
    capture(args).status.success()
}

/// Pull `image`, streaming progress to the terminal.
pub fn pull(image: &str) -> bool {
    inherit(&["pull", image])
}

pub fn network_exists(name: &str) -> bool {
    ok(&["network", "exists", name])
}

pub fn create_network(name: &str) -> bool {
    ok(&["network", "create", "--dns-enabled", name])
}

/// State of the container named exactly `name`, if it exists.
pub fn container_state(name: &str) -> Option<String> {
    let filter = format!("name=^{name}$");
    let out = capture(&["ps", "-a", "--filter", &filter, "--format", "{{.State}}"]);
    let state = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!state.is_empty()).then_some(state)
}

pub fn start_container(name: &str) -> bool {
    ok(&["start", name])
}

pub fn remove_container(name: &str) -> bool {
    ok(&["rm", "-f", name])
}
