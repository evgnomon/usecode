// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! What any module needs to get a host into shape: tools and packages,
//! and root-owned files written only when they differ.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::process::Command;

use crate::error::{Context, Result};

/// The first of `tools` that isn't on PATH, if any.
pub fn missing<'a>(tools: &[&'a str]) -> Option<&'a str> {
    let paths = std::env::var_os("PATH").unwrap_or_default();
    tools
        .iter()
        .copied()
        .find(|tool| !std::env::split_paths(&paths).any(|dir| dir.join(tool).is_file()))
}

/// Make sure every `(tool, package)` pair's tool is on PATH, installing
/// the packages for the ones that aren't with apt-get. What it couldn't
/// do is the error, for the module to log and retry.
pub fn ensure_tools(tools: &[(&str, &str)]) -> Result<()> {
    let wanted: Vec<&str> = tools
        .iter()
        .filter(|(tool, _)| missing(&[tool]).is_some())
        .map(|(_, package)| *package)
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    if missing(&["apt-get"]).is_some() {
        bail!(
            "needs {} and there is no apt-get to install it with; install it by hand",
            wanted.join(" ")
        );
    }
    eprintln!("installing {}", wanted.join(" "));
    apt(&["update", "-qq"])?;
    apt(&[
        &["install", "-y", "-qq", "--no-install-recommends"],
        &wanted[..],
    ]
    .concat())
}

fn apt(args: &[&str]) -> Result<()> {
    let status = Command::new("apt-get")
        .args(args)
        .env("DEBIAN_FRONTEND", "noninteractive")
        .status()
        .ctx("run apt-get")?;
    if !status.success() {
        bail!("apt-get {} failed ({status})", args.join(" "));
    }
    Ok(())
}

/// Write `contents` to `path` with `mode`, owned by root, unless it is
/// already exactly that. Reports whether anything changed. The write
/// goes through a temporary file and a rename, so a running binary or a
/// reader of the file never sees half of it.
pub fn put(path: &str, contents: &[u8], mode: u32) -> Result<bool> {
    if let Ok(meta) = fs::metadata(path)
        && meta.permissions().mode() & 0o7777 == mode
        && std::os::unix::fs::MetadataExt::uid(&meta) == 0
        && fs::read(path).is_ok_and(|old| old == contents)
    {
        return Ok(false);
    }

    let tmp = format!("{path}.uc-tmp");
    let write = || -> std::io::Result<()> {
        let _ = fs::remove_file(&tmp);
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
        // The umask may have narrowed the mode on create.
        fs::set_permissions(&tmp, fs::Permissions::from_mode(mode))?;
        std::os::unix::fs::chown(&tmp, Some(0), Some(0))?;
        fs::rename(&tmp, path)
    };
    if let Err(e) = write() {
        let _ = fs::remove_file(&tmp);
        bail!("write {path}: {e}");
    }
    eprintln!("wrote {path}");
    Ok(true)
}

/// Create `path` if needed and give it `mode`, owned by root.
pub fn ensure_dir(path: &str, mode: u32) -> Result<()> {
    fs::create_dir_all(path)
        .and_then(|_| fs::set_permissions(path, fs::Permissions::from_mode(mode)))
        .and_then(|_| std::os::unix::fs::chown(path, Some(0), Some(0)))
        .with_ctx(|| format!("create {path}"))
}

pub fn systemctl(args: &[&str]) -> Result<()> {
    let status = Command::new("systemctl")
        .args(args)
        .status()
        .ctx("run systemctl")?;
    if !status.success() {
        bail!("systemctl {} failed ({status})", args.join(" "));
    }
    Ok(())
}
