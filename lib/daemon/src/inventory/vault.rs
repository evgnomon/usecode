// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The ansible-vault encrypted secrets file.

use std::cell::OnceCell;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use ansible_vault::password;

use crate::error::{Context, Result};
use crate::inventory::Inventory;

/// The single mapping, host name -> WireGuard private key, that the
/// vault file holds. Keeping every host in one file (rather than one
/// vaulted file per host) is what lets `uc daemon install` resolve any
/// host's key by name.
pub const PRIVATE_KEYS_VAR: &str = "usecode_private_keys";

/// uc net mesh reads and writes the ansible-vault format itself, so
/// ansible doesn't have to be installed, but the file is still exactly
/// what `ansible-vault edit` expects, and the password comes from
/// wherever ansible would find it (--vault-password-file,
/// ANSIBLE_VAULT_PASSWORD_FILE, vault_password_file in ansible.cfg, or a
/// prompt).
#[derive(Debug, Default)]
pub struct Vault {
    /// The secrets file, encrypted in place.
    pub path: PathBuf,
    /// The directory whose ansible.cfg (with e.g. vault_password_file)
    /// applies, as if ansible-vault were run there.
    pub dir: PathBuf,
    /// When set, used in place of any configured password file.
    pub password_file: String,
    password: Password,
}

/// The vault password, resolved at most once per run so a put (decrypt,
/// then encrypt) asks for it once. Never printed.
#[derive(Default)]
struct Password(OnceCell<String>);

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(..)")
    }
}

impl Vault {
    /// The vault for `inv`'s secrets file.
    pub fn new(inv: &Inventory, password_file: &str) -> Vault {
        Vault {
            path: inv.secrets_path(),
            dir: inv.root.clone(),
            password_file: password_file.to_string(),
            ..Vault::default()
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

    /// Every host's private key, by host name. Decrypting needs the
    /// vault password, but only once there is a secrets file at all.
    pub fn private_keys(&self) -> Result<BTreeMap<String, String>> {
        self.load()
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
        let plaintext = if ansible_vault::is_encrypted(&raw) {
            let text = String::from_utf8_lossy(&raw);
            let plain = ansible_vault::decrypt(&text, self.password(false)?.as_bytes())
                .with_ctx(|| format!("decrypt {}", self.path.display()))?;
            String::from_utf8_lossy(&plain).into_owned()
        } else {
            String::from_utf8_lossy(&raw).into_owned()
        };

        let doc: BTreeMap<String, BTreeMap<String, String>> = serde_yaml::from_str(&plaintext)
            .with_ctx(|| format!("parse {}", self.path.display()))?;
        Ok(doc.get(PRIVATE_KEYS_VAR).cloned().unwrap_or_default())
    }

    /// Write `secrets` back, encrypted. The plaintext never touches the
    /// disk, and the file is replaced in one rename.
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

        let new = !fs::read(&self.path).is_ok_and(|raw| ansible_vault::is_encrypted(&raw));
        let password = self.password(new)?;
        ansible_vault::encrypt_file(&self.path, (header + &body).as_bytes(), password.as_bytes())
            .ctx("save the vault")
    }

    /// The vault password. `new` is for encrypting a file that has no
    /// password yet: a prompt then asks twice.
    fn password(&self, new: bool) -> Result<&str> {
        if let Some(pw) = self.password.0.get() {
            return Ok(pw);
        }
        let dir = if self.dir.as_os_str().is_empty() {
            Path::new(".")
        } else {
            &self.dir
        };
        // deploy/vault-pass.sh hands out a placeholder while there is no
        // secrets file; this makes it produce the real password.
        let env = [("USECODE_VAULT_WRITE", "1")];
        let pw = if self.password_file.is_empty() {
            password::resolve(dir, &env, new)
        } else {
            password::read_file(Path::new(&self.password_file), dir, &env)
        }
        .ctx("vault password")?;
        Ok(self.password.0.get_or_init(|| pw))
    }
}

fn base_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    // A put writes a file `load` reads back with the same password, and
    // the password is asked for once.
    #[test]
    fn put_round_trips_through_a_password_file() {
        let tmp = tempfile::tempdir().unwrap();
        let pw = tmp.path().join("pw");
        fs::write(&pw, "s3cret\n").unwrap();
        let vault = Vault {
            path: tmp.path().join("secrets.yml"),
            password_file: pw.to_string_lossy().into_owned(),
            ..Vault::default()
        };

        vault.put("edge", "key-a").unwrap();
        vault.put("phone", "key-b").unwrap();

        let raw = fs::read_to_string(&vault.path).unwrap();
        assert!(raw.starts_with(ansible_vault::HEADER), "{raw}");

        let fresh = Vault {
            path: vault.path.clone(),
            password_file: vault.password_file.clone(),
            ..Vault::default()
        };
        let keys = fresh.private_keys().unwrap();
        assert_eq!(keys["edge"], "key-a");
        assert_eq!(keys["phone"], "key-b");
        assert!(vault.put("edge", "again").is_err());
    }
}
