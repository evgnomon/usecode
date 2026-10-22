// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Finding the vault password the way `ansible-vault` run in a given
//! directory would: ANSIBLE_VAULT_PASSWORD_FILE, then vault_password_file
//! in ansible.cfg, then a prompt. A password file that is executable is
//! run and its stdout taken, so the password can stay in a secret store.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{Result, bail, ctx};

/// The password `ansible-vault` run in `dir` would use. `env` is passed
/// to an executable password file. `new` is for encrypting a file that
/// has no password yet: a prompt then asks twice.
pub fn resolve(dir: &Path, env: &[(&str, &str)], new: bool) -> Result<String> {
    match configured_file(dir) {
        Some(file) => read_file(&file, dir, env),
        None => prompt(new),
    }
}

/// The password file ansible-vault run in `dir` would read, if any:
/// ANSIBLE_VAULT_PASSWORD_FILE, then vault_password_file in ansible.cfg.
pub fn configured_file(dir: &Path) -> Option<PathBuf> {
    if let Some(file) = std::env::var_os("ANSIBLE_VAULT_PASSWORD_FILE").filter(|f| !f.is_empty()) {
        return Some(expand_home(Path::new(&file)));
    }
    let cfg = ansible_cfg(dir)?;
    let text = std::fs::read_to_string(&cfg).ok()?;
    let value = ini_value(&text, "defaults", "vault_password_file")?;
    // ansible resolves a relative path against the config file's directory.
    let file = expand_home(Path::new(&value));
    Some(cfg.parent().unwrap_or(Path::new(".")).join(file))
}

/// Read a password file, or, like ansible, run it in `dir` and take its
/// stdout if it is executable. Surrounding whitespace is dropped, as
/// ansible does.
pub fn read_file(file: &Path, dir: &Path, env: &[(&str, &str)]) -> Result<String> {
    let meta = ctx(std::fs::metadata(file), || {
        format!("vault password file {}", file.display())
    })?;
    let pw = if meta.permissions().mode() & 0o111 == 0 {
        ctx(std::fs::read_to_string(file), || {
            format!("read vault password file {}", file.display())
        })?
    } else {
        let out = ctx(
            Command::new(file)
                .current_dir(dir)
                .envs(env.iter().copied())
                .stdin(Stdio::inherit())
                .stderr(Stdio::inherit())
                .output(),
            || format!("run vault password script {}", file.display()),
        )?;
        if !out.status.success() {
            bail!(
                "vault password script {} exited with {}",
                file.display(),
                out.status
            );
        }
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let pw = pw.trim().to_string();
    if pw.is_empty() {
        bail!("vault password file {} is empty", file.display());
    }
    Ok(pw)
}

/// Ask on the terminal, twice when `new`, as `ansible-vault encrypt` does.
pub fn prompt(new: bool) -> Result<String> {
    let ask = |p: &str| {
        ctx(rpassword::prompt_password(p), || {
            "read vault password".into()
        })
    };
    if !new {
        return ask("Vault password: ");
    }
    let pw = ask("New vault password: ")?;
    if pw != ask("Confirm new vault password: ")? {
        bail!("the vault passwords don't match");
    }
    if pw.is_empty() {
        bail!("the vault password is empty");
    }
    Ok(pw)
}

/// The one ansible.cfg ansible would use from `dir`: ANSIBLE_CONFIG,
/// then ansible.cfg there, then ~/.ansible.cfg, then
/// /etc/ansible/ansible.cfg. The first that exists wins outright; they
/// aren't merged.
fn ansible_cfg(dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(cfg) = std::env::var_os("ANSIBLE_CONFIG").filter(|c| !c.is_empty()) {
        candidates.push(expand_home(Path::new(&cfg)));
    }
    candidates.push(dir.join("ansible.cfg"));
    candidates.push(expand_home(Path::new("~/.ansible.cfg")));
    candidates.push(PathBuf::from("/etc/ansible/ansible.cfg"));
    candidates.into_iter().find(|c| c.is_file())
}

/// `key` in `[section]` of an INI file, the subset ansible.cfg uses.
fn ini_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut current = String::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = name.trim().to_string();
            continue;
        }
        if current != section {
            continue;
        }
        if let Some((k, v)) = line.split_once(['=', ':'])
            && k.trim() == key
        {
            return Some(v.trim().to_string()).filter(|v| !v.is_empty());
        }
    }
    None
}

fn expand_home(path: &Path) -> PathBuf {
    match (path.strip_prefix("~"), std::env::var_os("HOME")) {
        (Ok(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_vault_password_file_from_ansible_cfg() {
        let cfg = "[defaults]\n# where it is\ninventory = ~/x\nvault_password_file = deploy/vault-pass.sh\n\n[ssh]\nvault_password_file = nope\n";
        assert_eq!(
            ini_value(cfg, "defaults", "vault_password_file").as_deref(),
            Some("deploy/vault-pass.sh")
        );
        assert_eq!(ini_value(cfg, "defaults", "missing"), None);
    }

    #[test]
    fn reads_a_plain_password_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("pw");
        std::fs::write(&file, "s3cret\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(read_file(&file, tmp.path(), &[]).unwrap(), "s3cret");
    }

    // An executable password file is run, as ansible does with
    // deploy/vault-pass.sh.
    #[test]
    fn runs_an_executable_password_file() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("pass.sh");
        std::fs::write(&script, "#!/bin/sh\necho \"pw-$X\"\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            read_file(&script, tmp.path(), &[("X", "1")]).unwrap(),
            "pw-1"
        );
    }
}
