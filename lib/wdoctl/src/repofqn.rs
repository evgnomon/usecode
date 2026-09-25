// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Inlined `repofqn`: "<parent-dir-basename>_<cwd-basename>".

use std::env;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

/// Logical working directory like `pwd`: $PWD when it is absolute, free of
/// `.`/`..` and names the current directory; otherwise the physical path.
fn pwd() -> PathBuf {
    let phys = env::current_dir().unwrap_or_default();
    if let Some(p) = env::var_os("PWD").map(PathBuf::from)
        && p.is_absolute()
        && p.components()
            .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
        && let (Ok(a), Ok(b)) = (p.metadata(), Path::new(".").metadata())
        && a.dev() == b.dev()
        && a.ino() == b.ino()
    {
        return p;
    }
    phys
}

/// POSIX `basename`.
fn basename(p: &str) -> &str {
    let t = p.trim_end_matches('/');
    if t.is_empty() {
        return if p.is_empty() { "" } else { "/" };
    }
    t.rsplit('/').next().unwrap_or(t)
}

/// POSIX `dirname`.
fn dirname(p: &str) -> &str {
    let t = p.trim_end_matches('/');
    if t.is_empty() {
        return if p.is_empty() { "." } else { "/" };
    }
    match t.rfind('/') {
        None => ".",
        Some(i) => {
            let d = t[..i].trim_end_matches('/');
            if d.is_empty() { "/" } else { d }
        }
    }
}

fn fqn(path: &str) -> String {
    format!("{}_{}", basename(dirname(path)), basename(path))
}

/// Equivalent of `$(repofqn)` for the current directory.
pub fn repofqn() -> String {
    fqn(&pwd().to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::fqn;

    #[test]
    fn org_and_repo() {
        assert_eq!(
            fqn("/home/u/src/github.com/evgnomon/usecode"),
            "evgnomon_usecode"
        );
        assert_eq!(fqn("/a"), "/_a");
        assert_eq!(fqn("/"), "/_/");
    }
}
