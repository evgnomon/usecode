// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Rotate the vault password of a secret file: generate a new password,
//! re-encrypt the secret file with it and replace the old `<name>.asc`.

mod vault;

use std::env;
use std::fs;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::{Command, Stdio, exit};

struct Names {
    secret_file: String,
    vault_file: String,
    new_vault_file: String,
}

fn names(dir: &str, name: &str) -> Names {
    if name.is_empty() {
        Names {
            secret_file: format!("{dir}/secrets.yaml"),
            vault_file: "vault".into(),
            new_vault_file: "vault_new".into(),
        }
    } else {
        Names {
            secret_file: format!("{dir}/{name}.yaml"),
            vault_file: format!("{name}.vault"),
            new_vault_file: format!("{name}.vault_new"),
        }
    }
}

fn mv(from: &str, to: &str) {
    if let Err(e) = fs::rename(from, to) {
        eprintln!("mv: cannot move '{from}' to '{to}': {e}");
        exit(1);
    }
}

fn rm(path: &str) {
    if let Err(e) = fs::remove_file(path) {
        eprintln!("rm: cannot remove '{path}': {e}");
        exit(1);
    }
}

fn check(rc: i32) {
    if rc != 0 {
        exit(rc);
    }
}

/// `ansible-vault decrypt --vault-password-file <(vault -d OLD) --output - BAK |
///  ansible-vault encrypt --vault-password-file <(vault -d NEW) --output SECRET`
fn reencrypt(n: &Names, bak: &str) -> i32 {
    let (old_producer, old_r) = match vault::vault_decrypt_sub(&n.vault_file) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("rotate_keychain_pass: pipe: {e}");
            return 1;
        }
    };
    let mut decrypt = Command::new("ansible-vault");
    decrypt
        .args(["decrypt", "--vault-password-file"])
        .arg(vault::fd_path(&old_r))
        .args(["--output", "-", bak])
        .stdout(Stdio::piped());
    vault::pass_fds(&mut decrypt, vec![old_r.as_raw_fd()]);

    let mut encrypt = Command::new("ansible-vault");
    let decryptor = match decrypt.spawn() {
        Ok(mut c) => {
            if let Some(out) = c.stdout.take() {
                encrypt.stdin(out);
            }
            Some(c)
        }
        Err(e) => {
            vault::spawn_failed("ansible-vault", &e);
            encrypt.stdin(Stdio::null());
            None
        }
    };
    drop(decrypt);
    drop(old_r);

    let (new_producer, new_r) = match vault::vault_decrypt_sub(&n.new_vault_file) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("rotate_keychain_pass: pipe: {e}");
            return 1;
        }
    };
    encrypt
        .args(["encrypt", "--vault-password-file"])
        .arg(vault::fd_path(&new_r))
        .args(["--output", &n.secret_file]);
    vault::pass_fds(&mut encrypt, vec![new_r.as_raw_fd()]);
    let rc = vault::status(&mut encrypt);
    drop(encrypt);
    drop(new_r);
    if let Some(mut c) = decryptor {
        let _ = c.wait();
    }
    vault::reap(old_producer);
    vault::reap(new_producer);
    rc
}

fn main() {
    let dir = vault::secrets_dir();
    let n = names(&dir, &env::args().nth(1).unwrap_or_default());
    let bak = format!("{}.bak", n.secret_file);

    if !Path::new(&bak).is_file() {
        mv(&n.secret_file, &bak);
    }

    check(vault::vault_encrypt_new(&n.new_vault_file));
    check(reencrypt(&n, &bak));

    let new_asc = format!("{dir}/{}.asc", n.new_vault_file);
    if Path::new(&new_asc).is_file() {
        let asc = format!("{dir}/{}.asc", n.vault_file);
        rm(&asc);
        mv(&new_asc, &asc);
    }
    if Path::new(&bak).is_file() {
        rm(&bak);
    }
}

#[cfg(test)]
mod tests {
    use super::names;

    #[test]
    fn default_and_named() {
        let d = names("/s", "");
        assert_eq!(d.secret_file, "/s/secrets.yaml");
        assert_eq!(d.vault_file, "vault");
        assert_eq!(d.new_vault_file, "vault_new");
        let n = names("/s", "x");
        assert_eq!(n.secret_file, "/s/x.yaml");
        assert_eq!(n.vault_file, "x.vault");
        assert_eq!(n.new_vault_file, "x.vault_new");
    }
}
