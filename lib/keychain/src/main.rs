// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Create or edit an ansible-vault encrypted secret file whose password is
//! kept in `vault` (`keychain [name]`).

mod vault;

use std::env;
use std::path::Path;
use std::process::{Command, exit};

fn main() {
    // SAFETY: no other threads exist yet.
    unsafe { env::set_var("EDITOR", "vi") };

    let name = env::args().nth(1).unwrap_or_default();
    let dir = vault::secrets_dir();
    let (secret_file, vault_file) = if name.is_empty() {
        (format!("{dir}/secrets.yaml"), "vault".to_string())
    } else {
        (format!("{dir}/{name}.yaml"), format!("{name}.vault"))
    };

    if !Path::new(&format!("{dir}/{vault_file}.asc")).is_file() {
        vault::vault_encrypt_new(&vault_file);
    }

    let action = if Path::new(&secret_file).is_file() {
        "edit"
    } else {
        "create"
    };
    let mut cmd = Command::new("ansible-vault");
    cmd.args([action, &secret_file]);
    exit(vault::run_with_vault_password(cmd, &vault_file));
}
