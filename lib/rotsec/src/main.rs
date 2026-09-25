// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Run the blueprint `rotate.yaml` playbook with the current repository's
//! secrets (creating the vault password and secret file if missing).

mod repofqn;
mod vault;

use std::env;
use std::path::Path;
use std::process::{Command, exit};

fn main() {
    // SAFETY: no other threads exist yet.
    unsafe {
        env::set_var("EDITOR", "vi");
    }

    let addr = repofqn::repofqn();
    let dir = vault::secrets_dir();
    let (secret_file, vault_file) = if addr.is_empty() {
        (format!("{dir}/secrets.yaml"), "vault".to_string())
    } else {
        (format!("{dir}/{addr}.yaml"), format!("{addr}.vault"))
    };

    vault::ensure(&vault_file, &secret_file);

    let blueprint = format!(
        "{}/src/github.com/{}/blueprint",
        env::var("HOME").unwrap_or_default(),
        env::var("USER").unwrap_or_default()
    );
    let mut cmd = Command::new("ansible-playbook");
    cmd.env("ANSIBLE_STDOUT_CALLBACK", "yaml")
        .arg("--extra-vars")
        .arg(format!("@{secret_file}"))
        .args(["-i", "inventory.py", "rotate.yaml"])
        .args(env::args_os().skip(1));
    if Path::new(&blueprint).is_dir() {
        cmd.current_dir(&blueprint);
    } else {
        eprintln!("rotsec: cd: {blueprint}: No such file or directory");
    }
    exit(vault::run_with_vault_password(cmd, &vault_file));
}
