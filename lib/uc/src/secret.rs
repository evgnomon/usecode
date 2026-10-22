// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The ansible-vault secret store behind `uc secret`.
//!
//! A store named `<name>` is two files in [`dir`]: `<name>.yaml`, encrypted
//! in the ansible-vault format, and `<name>.vault.asc`, that file's vault
//! password as encrypted by the `vault` command (`vault -e`/`vault -d`). The
//! unnamed store is `secrets.yaml` with `vault.asc`.
//!
//! With a [`Profile`] active - for a machine that can't open those because
//! the GPG key is somewhere else - the stores are its own, in `<profile>/`
//! and encrypted to its key. Without one they are the shared ones above.
//!
//! The stores are read and written here, without ansible, and stay
//! openable with `ansible-vault`. Vault passwords only live in memory; the
//! plaintext only touches a file while `$EDITOR` has it open.

use crate::password::{self, Charset};
use crate::profile::Profile;
use anyhow::{Context, Result, bail};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zeroize::Zeroizing;

/// `$HOME/src/github.com/$USER/<repo>`
pub(crate) fn user_repo(repo: &str) -> PathBuf {
    let var = |name| std::env::var(name).unwrap_or_default();
    PathBuf::from(format!(
        "{}/src/github.com/{}/{repo}",
        var("HOME"),
        var("USER")
    ))
}

/// `$HOME/src/github.com/$USER/config/secrets`
pub fn dir() -> PathBuf {
    user_repo("config/secrets")
}

/// The files making up one store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    /// The ansible-vault encrypted secrets.
    pub secret_file: PathBuf,
    /// The name `vault -e`/`vault -d` know the password by.
    pub vault_name: String,
    dir: PathBuf,
    /// Who `vault -e` encrypts to; `vault`'s own key when unset.
    gpg_key: Option<String>,
    /// Whether this is a profile's own store.
    profiled: bool,
}

impl Store {
    /// The store called `name` in `dir`; the empty name is the default store.
    pub fn new(dir: &Path, name: &str) -> Self {
        Store::on(&Profile::default(), dir, name)
    }

    /// The store called `name` as the active profile sees it: in [`dir`],
    /// or in the profile's own `<profile>/`.
    pub fn open(name: &str) -> Result<Self> {
        Ok(Store::on(&Profile::load()?, &dir(), name))
    }

    fn on(profile: &Profile, dir: &Path, name: &str) -> Self {
        let (file, vault_name) = match name {
            "" => ("secrets.yaml".to_string(), "vault".to_string()),
            name => (format!("{name}.yaml"), format!("{name}.vault")),
        };
        let prefix = profile
            .name
            .as_ref()
            .map_or(String::new(), |m| format!("{m}/"));
        Store {
            secret_file: dir.join(format!("{prefix}{file}")),
            vault_name: format!("{prefix}{vault_name}"),
            dir: dir.to_path_buf(),
            gpg_key: profile.name.as_ref().and(profile.gpg_key.clone()),
            profiled: profile.name.is_some(),
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
        let password = vault_decrypt(&self.vault_name);
        if self.profiled {
            return password;
        }
        password.context(
            "if this machine doesn't have the GPG key, `uc profile NAME` gives it stores of its own",
        )
    }

    /// `vault -e <name> [key]` with the password on stdin.
    fn vault_encrypt(&self, name: &str, password: &[u8]) -> Result<()> {
        if let Some(parent) = self.vault_asc(name).parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let mut cmd = Command::new("vault");
        cmd.args(["-e", name]).args(&self.gpg_key);
        feed(&mut cmd, password, "vault -e")
    }

    fn decrypt(&self, file: &Path) -> Result<Zeroizing<Vec<u8>>> {
        let password = self.password()?;
        Ok(Zeroizing::new(ansible_vault::decrypt_file(
            file,
            key(&password),
        )?))
    }

    /// The decrypted secrets as JSON, in file order.
    pub fn get(&self) -> Result<serde_json::Value> {
        self.require()?;
        let plain = self.decrypt(&self.secret_file)?;
        yaml_to_json(&String::from_utf8_lossy(&plain))
    }

    /// Opens the secrets in `$EDITOR`, creating the store first if needed.
    pub fn edit(&self) -> Result<()> {
        self.ensure_password()?;
        self.open_in_editor()
    }

    /// Creates whatever of the password and the secret file is missing.
    pub fn ensure(&self) -> Result<()> {
        self.ensure_password()?;
        if !self.secret_file.is_file() {
            self.open_in_editor()?;
        }
        Ok(())
    }

    /// `ansible-vault edit`, or `create` when there is no secret file yet:
    /// the plaintext goes to a private temporary file - in memory where
    /// /dev/shm exists - for `$EDITOR`, and is encrypted back if it changed.
    fn open_in_editor(&self) -> Result<()> {
        let existing = self.secret_file.is_file();
        let plain = if existing {
            self.decrypt(&self.secret_file)?
        } else {
            Zeroizing::new(Vec::new())
        };

        let mut builder = tempfile::Builder::new();
        builder.prefix("uc-secret-").suffix(".yaml");
        let shm = Path::new("/dev/shm");
        let mut tmp = if shm.is_dir() {
            builder.tempfile_in(shm)
        } else {
            builder.tempfile()
        }
        .context("creating a temporary file for the editor")?;
        tmp.write_all(&plain)
            .and_then(|_| tmp.flush())
            .context("writing the temporary file")?;

        let editor = std::env::var("EDITOR")
            .ok()
            .filter(|e| !e.trim().is_empty())
            .unwrap_or_else(|| "vi".to_string());
        // Through sh, so an EDITOR with arguments (`code -w`) works.
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "$UC_EDITOR \"$1\"", "sh"])
            .arg(tmp.path())
            .env("UC_EDITOR", &editor);
        status(&mut cmd, &editor)?;

        let edited =
            Zeroizing::new(fs::read(tmp.path()).context("reading back the temporary file")?);
        drop(tmp);
        if existing && *edited == *plain {
            return Ok(());
        }
        let password = self.password()?;
        ansible_vault::encrypt_file(&self.secret_file, &edited, key(&password))?;
        Ok(())
    }

    fn ensure_password(&self) -> Result<()> {
        if !self.has_password() {
            self.vault_encrypt(&self.vault_name, &new_password()?)?;
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
        self.vault_encrypt(&new_name, &new)?;

        let plain = self.decrypt(&bak)?;
        ansible_vault::encrypt_file(&self.secret_file, &plain, key(&new))?;

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

/// The vault key a stored password stands for: like ansible-vault reading
/// a password file, without the trailing newline.
fn key(password: &[u8]) -> &[u8] {
    password.trim_ascii()
}

/// `vault -d <name>`: the stored password.
fn vault_decrypt(name: &str) -> Result<Zeroizing<Vec<u8>>> {
    let mut cmd = Command::new("vault");
    cmd.args(["-d", name]);
    output(&mut cmd, "vault -d").map(Zeroizing::new)
}

pub(crate) fn status(cmd: &mut Command, what: &str) -> Result<()> {
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

        let profile = Profile {
            name: Some("wsl".into()),
            gpg_key: Some("ABCD".into()),
        };
        let m = Store::on(&profile, Path::new("/s"), "");
        assert_eq!(m.secret_file, PathBuf::from("/s/wsl/secrets.yaml"));
        assert_eq!(m.vault_name, "wsl/vault");
        assert_eq!(
            m.vault_asc(&m.vault_name),
            PathBuf::from("/s/wsl/vault.asc")
        );
        assert_eq!(m.gpg_key.as_deref(), Some("ABCD"));
        let shared = Store::new(Path::new("/s"), "");
        assert_eq!(shared.gpg_key, None);
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
    fn new_passwords_look_like_the_old_ones() {
        let p = new_password().unwrap();
        assert_eq!(p.len(), 65);
        assert_eq!(p[64], b'\n');
        assert!(p[..64].iter().all(u8::is_ascii_alphanumeric));
    }
}
