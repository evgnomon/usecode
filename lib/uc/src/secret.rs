// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The ansible-vault secret store behind `uc secret`.
//!
//! A store named `<name>` is two files in [`dir`]: `<name>.yaml`, encrypted
//! with ansible-vault, and `<name>.vault.asc`, that file's vault password as
//! encrypted by the `vault` command (`vault -e`/`vault -d`). The unnamed
//! store is `secrets.yaml` with `vault.asc`.
//!
//! Vault passwords only ever travel through pipes: ansible-vault reads them
//! from an inherited `/dev/fd/N`, never from a file or the command line.

use crate::password::{self, Charset};
use anyhow::{Context, Result, bail};
use std::fs;
use std::io::{self, PipeReader, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zeroize::Zeroizing;

/// `$HOME/src/github.com/$USER/config/secrets`
pub fn dir() -> PathBuf {
    let var = |name| std::env::var(name).unwrap_or_default();
    PathBuf::from(format!(
        "{}/src/github.com/{}/config/secrets",
        var("HOME"),
        var("USER")
    ))
}

/// The files making up one store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    /// The ansible-vault encrypted secrets.
    pub secret_file: PathBuf,
    /// The name `vault -e`/`vault -d` know the password by.
    pub vault_name: String,
    dir: PathBuf,
}

impl Store {
    /// The store called `name` in `dir`; the empty name is the default store.
    pub fn new(dir: &Path, name: &str) -> Self {
        let (file, vault_name) = match name {
            "" => ("secrets.yaml".to_string(), "vault".to_string()),
            name => (format!("{name}.yaml"), format!("{name}.vault")),
        };
        Store {
            secret_file: dir.join(file),
            vault_name,
            dir: dir.to_path_buf(),
        }
    }

    fn vault_asc(&self, vault_name: &str) -> PathBuf {
        self.dir.join(format!("{vault_name}.asc"))
    }

    fn has_password(&self) -> bool {
        self.vault_asc(&self.vault_name).is_file()
    }

    fn require(&self) -> Result<()> {
        if !self.has_password() {
            bail!(
                "vault file not found: {}",
                self.vault_asc(&self.vault_name).display()
            );
        }
        if !self.secret_file.is_file() {
            bail!("secret file not found: {}", self.secret_file.display());
        }
        Ok(())
    }

    fn password(&self) -> Result<Zeroizing<Vec<u8>>> {
        vault_decrypt(&self.vault_name)
    }

    /// The decrypted secrets as JSON, in file order.
    pub fn get(&self) -> Result<serde_json::Value> {
        self.require()?;
        let mut view = Command::new("ansible-vault");
        view.arg("view").arg(&self.secret_file);
        let plain = with_password(&mut view, &self.password()?, |cmd| {
            output(cmd, "ansible-vault view")
        })?;
        yaml_to_json(&String::from_utf8_lossy(&plain))
    }

    /// Opens the secrets in `$EDITOR`, creating the store first if needed.
    pub fn edit(&self) -> Result<()> {
        self.ensure_password()?;
        let action = if self.secret_file.is_file() {
            "edit"
        } else {
            "create"
        };
        let mut cmd = Command::new("ansible-vault");
        cmd.arg(action).arg(&self.secret_file);
        with_password(&mut cmd, &self.password()?, |cmd| {
            status(cmd, &format!("ansible-vault {action}"))
        })
    }

    /// Creates whatever of the password and the secret file is missing.
    pub fn ensure(&self) -> Result<()> {
        self.ensure_password()?;
        if !self.secret_file.is_file() {
            let mut cmd = Command::new("ansible-vault");
            cmd.arg("create").arg(&self.secret_file);
            with_password(&mut cmd, &self.password()?, |cmd| {
                status(cmd, "ansible-vault create")
            })?;
        }
        Ok(())
    }

    fn ensure_password(&self) -> Result<()> {
        if !self.has_password() {
            vault_encrypt(&self.vault_name, &new_password()?)?;
        }
        Ok(())
    }

    /// Re-encrypts the secrets under a new random password and replaces the
    /// stored one.
    ///
    /// The secret file is first moved to `<file>.bak`, which is only removed
    /// once the new password is in place, so an interrupted rotation can be
    /// run again and picks up from the backup.
    pub fn rotate(&self) -> Result<()> {
        let mut bak = self.secret_file.clone().into_os_string();
        bak.push(".bak");
        let bak = PathBuf::from(bak);
        if !bak.is_file() {
            fs::rename(&self.secret_file, &bak)
                .with_context(|| format!("moving {} aside", self.secret_file.display()))?;
        }

        let new_name = format!("{}_new", self.vault_name);
        let new = new_password()?;
        vault_encrypt(&new_name, &new)?;

        let mut decrypt = Command::new("ansible-vault");
        decrypt.args(["decrypt", "--output", "-"]).arg(&bak);
        let plain = with_password(&mut decrypt, &self.password()?, |cmd| {
            output(cmd, "ansible-vault decrypt")
        })?;
        let mut encrypt = Command::new("ansible-vault");
        encrypt.args(["encrypt", "--output"]).arg(&self.secret_file);
        with_password(&mut encrypt, &new, |cmd| {
            feed(cmd, &plain, "ansible-vault encrypt")
        })?;

        let (old_asc, new_asc) = (self.vault_asc(&self.vault_name), self.vault_asc(&new_name));
        fs::rename(&new_asc, &old_asc)
            .with_context(|| format!("replacing {}", old_asc.display()))?;
        fs::remove_file(&bak).with_context(|| format!("removing {}", bak.display()))
    }
}

/// 64 random letters and digits and a newline, as the vault passwords have
/// always been.
fn new_password() -> Result<Zeroizing<Vec<u8>>> {
    let charset = Charset {
        letters: true,
        digits: true,
        symbols: false,
    };
    let mut password = password::generate(64, charset)?;
    password.push('\n');
    Ok(Zeroizing::new(password.into_bytes()))
}

/// `vault -d <name>`: the stored password.
fn vault_decrypt(name: &str) -> Result<Zeroizing<Vec<u8>>> {
    let mut cmd = Command::new("vault");
    cmd.args(["-d", name]);
    output(&mut cmd, "vault -d").map(Zeroizing::new)
}

/// `vault -e <name>` with the password on stdin.
fn vault_encrypt(name: &str, password: &[u8]) -> Result<()> {
    let mut cmd = Command::new("vault");
    cmd.args(["-e", name]);
    feed(&mut cmd, password, "vault -e")
}

/// Runs `run(cmd)` with `ANSIBLE_VAULT_PASSWORD_FILE` pointing at a pipe
/// that holds `password`.
fn with_password<T>(
    cmd: &mut Command,
    password: &[u8],
    run: impl FnOnce(&mut Command) -> Result<T>,
) -> Result<T> {
    let reader = password_pipe(password).context("creating the password pipe")?;
    cmd.env(
        "ANSIBLE_VAULT_PASSWORD_FILE",
        format!("/dev/fd/{}", reader.as_raw_fd()),
    );
    inherit(cmd, reader.as_raw_fd());
    let result = run(cmd);
    drop(reader);
    result
}

/// A pipe whose read end yields `password` and then end of file. A password
/// is far smaller than a pipe's buffer, so writing it up front never blocks.
fn password_pipe(password: &[u8]) -> io::Result<PipeReader> {
    let (reader, mut writer) = io::pipe()?;
    writer.write_all(password)?;
    Ok(reader)
}

/// Lets the child inherit `fd`, which Rust opens close-on-exec.
fn inherit(cmd: &mut Command, fd: RawFd) {
    // SAFETY: the closure only calls the async-signal-safe fcntl(2).
    unsafe {
        cmd.pre_exec(move || {
            if libc::fcntl(fd, libc::F_SETFD, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

fn status(cmd: &mut Command, what: &str) -> Result<()> {
    let status = cmd.status().with_context(|| format!("running {what}"))?;
    if !status.success() {
        bail!("{what} failed ({status})");
    }
    Ok(())
}

fn output(cmd: &mut Command, what: &str) -> Result<Vec<u8>> {
    let out = cmd
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("running {what}"))?;
    if !out.status.success() {
        bail!("{what} failed ({})", out.status);
    }
    Ok(out.stdout)
}

fn feed(cmd: &mut Command, input: &[u8], what: &str) -> Result<()> {
    let mut child = cmd
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("running {what}"))?;
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(input)
        .with_context(|| format!("writing to {what}"))?;
    let status = child.wait()?;
    if !status.success() {
        bail!("{what} failed ({status})");
    }
    Ok(())
}

/// Converts the decrypted YAML to JSON, resolving `<<` merge keys.
pub fn yaml_to_json(yaml: &str) -> Result<serde_json::Value> {
    let mut value: serde_yaml::Value = serde_yaml::from_str(yaml).context("parsing the secrets")?;
    value.apply_merge().context("resolving merge keys")?;
    serde_json::to_value(value).context("converting the secrets to JSON")
}

/// The value at a dotted path such as `hetzner.prod`, like jq's `.hetzner.prod`.
pub fn field<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    path.trim_start_matches('.')
        .split('.')
        .filter(|key| !key.is_empty())
        .try_fold(value, |value, key| match value {
            serde_json::Value::Array(items) => items.get(key.parse::<usize>().ok()?),
            _ => value.get(key),
        })
}

/// A value as `jq -r` prints it: strings bare, everything else as JSON.
pub fn raw(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_the_default_and_named_stores() {
        let d = Store::new(Path::new("/s"), "");
        assert_eq!(d.secret_file, PathBuf::from("/s/secrets.yaml"));
        assert_eq!(d.vault_name, "vault");
        assert_eq!(d.vault_asc("vault"), PathBuf::from("/s/vault.asc"));
        let n = Store::new(Path::new("/s"), "evgnomon_usecode");
        assert_eq!(n.secret_file, PathBuf::from("/s/evgnomon_usecode.yaml"));
        assert_eq!(n.vault_name, "evgnomon_usecode.vault");
    }

    #[test]
    fn converts_yaml_keeping_order_and_merges() {
        let json =
            yaml_to_json("b: 1\na:\n  x: \"s\"\nbase: &b {k: v}\nc:\n  <<: *b\n  z: 2\n").unwrap();
        assert_eq!(
            json.to_string(),
            r#"{"b":1,"a":{"x":"s"},"base":{"k":"v"},"c":{"z":2,"k":"v"}}"#
        );
    }

    #[test]
    fn picks_fields_like_jq() {
        let v = json!({"hetzner": {"prod": "tok"}, "list": [1, {"a": true}], "n": 3});
        assert_eq!(raw(field(&v, ".hetzner.prod").unwrap()), "tok");
        assert_eq!(raw(field(&v, "list.1.a").unwrap()), "true");
        assert_eq!(raw(field(&v, "n").unwrap()), "3");
        assert_eq!(field(&v, "hetzner.dev"), None);
        assert_eq!(field(&v, ".").unwrap(), &v);
    }

    #[test]
    fn a_password_pipe_reaches_a_child_through_dev_fd() {
        let mut cat = Command::new("sh");
        cat.args(["-c", "cat \"$ANSIBLE_VAULT_PASSWORD_FILE\""]);
        let out = with_password(&mut cat, b"hunter2\n", |cmd| output(cmd, "cat")).unwrap();
        assert_eq!(out, b"hunter2\n");
    }

    #[test]
    fn new_passwords_look_like_the_old_ones() {
        let p = new_password().unwrap();
        assert_eq!(p.len(), 65);
        assert_eq!(p[64], b'\n');
        assert!(p[..64].iter().all(u8::is_ascii_alphanumeric));
    }
}
