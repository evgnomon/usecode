// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Ensure the current repository's vault password and encrypted secret file
//! exist, then print the vault name.

mod repofqn;
mod vault;

use std::env;

fn main() {
    // SAFETY: no other threads exist yet.
    unsafe { env::set_var("EDITOR", "vi") };

    let addr = repofqn::repofqn();
    let dir = vault::secrets_dir();
    let (secret_file, vault_file) = if addr.is_empty() {
        (format!("{dir}/secrets.yaml"), "vault".to_string())
    } else {
        (format!("{dir}/{addr}.yaml"), format!("{addr}.vault"))
    };

    vault::ensure(&vault_file, &secret_file);
    println!("{vault_file}");
}
