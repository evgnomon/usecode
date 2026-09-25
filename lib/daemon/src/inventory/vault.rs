// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The ansible-vault encrypted secrets file.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Context, Result};
use crate::inventory::Inventory;

/// The single mapping, host name -> WireGuard private key, that the
/// vault file holds. Keeping every host in one file (rather than one
/// vaulted file per host) is what lets a playbook resolve any host's key
/// by name: `usecode_private_keys[inventory_hostname]`.
pub const PRIVATE_KEYS_VAR: &str = "usecode_private_keys";

/// uc daemon shells out to ansible-vault rather than implementing the
/// format, so the file is exactly what `ansible-vault edit` and a
/// playbook expect, and the password comes from wherever ansible
/// normally finds it (a prompt, --vault-password-file,
/// ANSIBLE_VAULT_PASSWORD_FILE, or ansible.cfg in the directory the
/// playbooks run from).
#[derive(Debug, Default)]
pub struct Vault {
    /// The secrets file, encrypted in place.
    pub path: PathBuf,
    /// The working directory ansible-vault runs in, so that an
    /// ansible.cfg there (with e.g. vault_password_file) applies.
    pub dir: PathBuf,
    /// When set, passed as --vault-password-file.
    pub password_file: String,
}

impl Vault {
    /// The vault for `inv`'s secrets file.
    pub fn new(inv: &Inventory, password_file: &str) -> Vault {
        Vault {
            path: inv.secrets_path(),
            dir: inv.root.clone(),
            password_file: password_file.to_string(),
        }
    }

    /// Store one host's private key, leaving every other host's alone.
    /// If the file doesn't exist yet it is created and encrypted.
    pub fn put(&self, host: &str, private_key: &str) -> Result<()> {
        let mut secrets = self.load()?;
        if secrets.contains_key(host) {
            bail!(
                "{host} already has a private key in {}",
                self.path.display()
            );
        }
        secrets.insert(host.to_string(), private_key.to_string());
        self.save(&secrets)
    }

    /// Decrypt the secrets file and return the host -> private key
    /// mapping, or an empty mapping if the file doesn't exist yet.
    fn load(&self) -> Result<BTreeMap<String, String>> {
        let raw = match fs::read(&self.path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(e) => bail!("read {}: {e}", self.path.display()),
        };

        // A file that isn't encrypted yet is still readable - it gets
        // encrypted on the way back out, so a hand-created plaintext
        // secrets file converges to a vaulted one rather than erroring.
        let plaintext = if raw.starts_with(b"$ANSIBLE_VAULT")
            || String::from_utf8_lossy(&raw)
                .trim_start()
                .starts_with("$ANSIBLE_VAULT")
        {
            self.decrypt()?
        } else {
            String::from_utf8_lossy(&raw).into_owned()
        };

        let doc: BTreeMap<String, BTreeMap<String, String>> = serde_yaml::from_str(&plaintext)
            .with_ctx(|| format!("parse {}", self.path.display()))?;
        Ok(doc.get(PRIVATE_KEYS_VAR).cloned().unwrap_or_default())
    }

    fn decrypt(&self) -> Result<String> {
        let out = self
            .command(&["decrypt", "--output=-", &self.path.to_string_lossy()])
            .output()
            .with_ctx(|| format!("decrypt {}", self.path.display()))?;
        if !out.status.success() {
            bail!(
                "decrypt {}: ansible-vault exited with {}",
                self.path.display(),
                out.status
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Write `secrets` back, encrypted. The plaintext is staged in a
    /// sibling directory of the vault created mode 0700 rather than
    /// piped through stdin, because ansible-vault needs stdin free to
    /// prompt for the vault password.
    fn save(&self, secrets: &BTreeMap<String, String>) -> Result<()> {
        let body = serde_yaml::to_string(&BTreeMap::from([(PRIVATE_KEYS_VAR, secrets)]))
            .ctx("encode secrets")?;

        let names: Vec<&str> = secrets.keys().map(String::as_str).collect();
        let header = format!(
            "---\n\
             # ansible-vault encrypted: every mesh host's WireGuard private key, keyed\n\
             # by inventory hostname ({PRIVATE_KEYS_VAR}), so a play can resolve the key for\n\
             # the host it is running against. Currently: {}.\n\
             #\n\
             # Edit with: ansible-vault edit {}\n",
            names.join(", "),
            base_name(&self.path)
        );

        let dir = self.path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir).with_ctx(|| format!("create {}", dir.display()))?;

        let staging = tempfile::Builder::new()
            .prefix(".uc-daemon-vault-")
            .tempdir_in(dir)
            .ctx("create staging directory")?;
        let tmp = staging.path().join("secrets.yml");

        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .with_ctx(|| format!("write {}", tmp.display()))?;
        f.write_all(header.as_bytes())
            .and_then(|_| f.write_all(body.as_bytes()))
            .and_then(|_| f.sync_all())
            .with_ctx(|| format!("write {}", tmp.display()))?;
        drop(f);

        let status = self
            .command(&[
                "encrypt",
                &format!("--output={}", self.path.display()),
                &tmp.to_string_lossy(),
            ])
            // Progress messages, not data.
            .stdout(std::process::Stdio::inherit())
            .status()
            .with_ctx(|| format!("encrypt {}", self.path.display()))?;
        if !status.success() {
            bail!(
                "encrypt {}: ansible-vault exited with {status}",
                self.path.display()
            );
        }
        Ok(())
    }

    /// Build an ansible-vault invocation with stdin and stderr attached
    /// so a password prompt reaches the terminal.
    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new("ansible-vault");
        cmd.args(args);
        if !self.password_file.is_empty() {
            cmd.args(["--vault-password-file", &self.password_file]);
        }
        if self.dir.as_os_str().is_empty() {
            cmd.current_dir(".");
        } else {
            cmd.current_dir(&self.dir);
        }
        cmd
    }
}

fn base_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Report a friendly error if ansible-vault isn't installed, before
/// anything has been written.
pub fn check_available() -> Result<()> {
    let found = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join("ansible-vault").is_file()))
        .unwrap_or(false);
    if !found {
        bail!(
            "ansible-vault not found in PATH; install ansible on this machine (the private key \
             for a new host has to be stored in the vault)"
        );
    }
    Ok(())
}
