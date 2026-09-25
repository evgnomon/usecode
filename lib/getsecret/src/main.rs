// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Decrypt an ansible-vault secret file and print it as JSON (via `yj`).

mod vault;

use std::env;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::{Command, Stdio, exit};

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

    let asc = format!("{dir}/{vault_file}.asc");
    if !Path::new(&asc).is_file() {
        println!("Vault file not found: {asc}");
        exit(1);
    }
    if !Path::new(&secret_file).is_file() {
        println!("Secret file not found: {secret_file}");
        exit(1);
    }

    let (producer, r) = match vault::vault_decrypt_sub(&vault_file) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("getsecret: pipe: {e}");
            exit(1);
        }
    };
    let mut view = Command::new("ansible-vault");
    view.args(["view", &secret_file])
        .env("ANSIBLE_VAULT_PASSWORD_FILE", vault::fd_path(&r))
        .stdout(Stdio::piped());
    vault::pass_fds(&mut view, vec![r.as_raw_fd()]);
    let mut yj = Command::new("yj");
    let viewer = match view.spawn() {
        Ok(mut c) => {
            if let Some(out) = c.stdout.take() {
                yj.stdin(out);
            }
            Some(c)
        }
        Err(e) => {
            vault::spawn_failed("ansible-vault", &e);
            yj.stdin(Stdio::null());
            None
        }
    };
    drop(view);
    drop(r);
    let rc = vault::status(&mut yj);
    if let Some(mut c) = viewer {
        let _ = c.wait();
    }
    vault::reap(producer);
    exit(rc);
}
