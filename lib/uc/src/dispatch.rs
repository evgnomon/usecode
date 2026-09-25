// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Finding and running the executables the `uc` commands hand over to.
//!
//! A program is looked up next to the running executable first, so a build
//! tree finds its own `uc-*` commands before an installed copy, then on
//! `PATH`.

use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Directories a program may live in, in search order.
pub fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        dirs.push(dir.to_path_buf());
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs
}

pub fn is_executable(path: &Path) -> bool {
    path.metadata()
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// The first executable called `program` in [`search_dirs`].
pub fn resolve(program: &str) -> Option<PathBuf> {
    search_dirs()
        .into_iter()
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
}

/// Replaces this process with `program`, found by [`resolve`], run with
/// `args`. It only returns when that fails, with the exit code to use: 127
/// when the program is not installed, as a shell would, 126 otherwise.
pub fn exec<S: AsRef<OsStr>>(program: &str, args: impl IntoIterator<Item = S>) -> ExitCode {
    let Some(path) = resolve(program) else {
        eprintln!("uc: '{program}' is not installed (not found next to uc or on PATH)");
        return ExitCode::from(127);
    };
    // Replacing the process leaves signals, exit status and the terminal to
    // the program, exactly as if it had been run directly; arg0 keeps its
    // own name in its messages.
    let err = Command::new(&path).arg0(program).args(args).exec();
    eprintln!("uc: failed to run '{}': {err}", path.display());
    ExitCode::from(126)
}
