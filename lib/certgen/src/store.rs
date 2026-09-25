// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Location of the certificate store (~/.x509) and the openssl runner.

use std::fs::DirBuilder;
use std::io::ErrorKind;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{Context, Result, bail};

/// Paths of the certificate store.
pub struct Store {
    pub dir: PathBuf,
    pub ca_key: PathBuf,
    pub ca_cert: PathBuf,
    pub ca_serial: PathBuf,
}

impl Store {
    /// Store rooted at `$HOME/.x509`.
    pub fn from_home() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .or_else(std::env::home_dir)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self::new(home.join(".x509"))
    }

    pub fn new(dir: PathBuf) -> Self {
        Self {
            ca_key: dir.join("ca.key"),
            ca_cert: dir.join("ca.pub"),
            ca_serial: dir.join("ca.srl"),
            dir,
        }
    }

    /// Path of `<name>.<ext>` inside the store.
    pub fn file(&self, name: &str, ext: &str) -> PathBuf {
        self.dir.join(format!("{name}.{ext}"))
    }

    /// Create the store directory (mode 0700) if it doesn't exist.
    pub fn ensure_dir(&self) -> Result<()> {
        match DirBuilder::new().mode(0o700).create(&self.dir) {
            Err(e) if e.kind() != ErrorKind::AlreadyExists => {
                Err(e).with_context(|| format!("creating {}", self.dir.display()))
            }
            _ => Ok(()),
        }
    }

    pub fn ca_exists(&self) -> bool {
        self.ca_key.exists() && self.ca_cert.exists()
    }
}

/// Run `openssl <args>` capturing its output. With `check`, a non-zero exit
/// status is an error.
pub fn openssl<S: AsRef<std::ffi::OsStr>>(args: &[S], check: bool) -> Result<Output> {
    let out = Command::new("openssl")
        .args(args)
        .output()
        .context("running openssl")?;
    if check && !out.status.success() {
        let argv: Vec<String> = args
            .iter()
            .map(|a| a.as_ref().to_string_lossy().into_owned())
            .collect();
        bail!(
            "openssl {} failed ({}): {}",
            argv.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim_end()
        );
    }
    Ok(out)
}

/// Convenience for turning a path into an owned string argument.
pub fn arg(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}
