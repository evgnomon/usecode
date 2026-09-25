// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! On-disk layout under /var/secrets.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::vault;

pub const ROOT: &str = "/var/secrets";

/// Reject path segments that could escape ROOT. Tenant and resource group
/// must be a single component; secret names may nest with '/'.
fn check(what: &str, value: &str, nested: bool) -> Result<(), String> {
    let bad = |c: &str| c.is_empty() || c == "." || c == "..";
    let nests = !nested && value.contains('/');
    match nests || value.contains('\0') || value.split('/').any(bad) {
        true => Err(format!("Invalid {what}: {value:?}")),
        false => Ok(()),
    }
}

pub fn secret_path(tenant: &str, rg: &str, name: &str) -> Result<String, String> {
    check("tenant", tenant, false)?;
    check("resource_group", rg, false)?;
    check("name", name, true)?;
    Ok(format!("{ROOT}/{tenant}/{rg}/secrets/{name}"))
}

pub fn authorized_keys_path(tenant: &str, rg: &str) -> Result<String, String> {
    check("tenant", tenant, false)?;
    check("resource_group", rg, false)?;
    Ok(format!("{ROOT}/{tenant}/{rg}/authorized_keys"))
}

/// Encrypt `value` and write it to `path`, creating parent directories.
pub fn write_secret(path: &str, value: &str, password: &str) -> Result<()> {
    let p = Path::new(path);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    std::fs::write(p, vault::encrypt(value, password)).with_context(|| format!("writing {path}"))
}

pub fn exists(path: &str) -> bool {
    PathBuf::from(path).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal() {
        assert!(secret_path("t", "rg", "a/b").is_ok());
        for (t, rg, n) in [
            ("..", "rg", "n"),
            ("t", "a/b", "n"),
            ("t", "rg", "../x"),
            ("t", "rg", "a/../../x"),
            ("t", "rg", "/etc/passwd"),
            ("t", "rg", ""),
            ("", "rg", "n"),
        ] {
            assert!(secret_path(t, rg, n).is_err(), "{t} {rg} {n}");
        }
        assert!(authorized_keys_path("../..", "tmp").is_err());
    }
}
