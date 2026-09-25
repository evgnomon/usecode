// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Reusable building blocks modelled on Ansible's modules. Each call is
//! idempotent and reports whether it changed the machine; each honours
//! `--check`. Role tasks are written in terms of these.

pub mod apt;
pub mod command;
pub mod copy;
pub mod file;
pub mod git;
pub mod inflate;
pub mod system;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Every regular file under `root`, as paths relative to it, sorted.
/// Symlinks are neither listed nor followed, like Ansible's `find` and the
/// `state == 'file'` entries of the `filetree` lookup.
pub fn walk_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let dir = root.join(&rel);
        let entries =
            std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))?;
        for entry in entries {
            let entry = entry?;
            let rel = rel.join(entry.file_name());
            let kind = entry.file_type()?;
            if kind.is_dir() {
                stack.push(rel);
            } else if kind.is_file() {
                files.push(rel);
            }
        }
    }
    files.sort();
    Ok(files)
}
