// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Builds the files under `roles/` into the binary, so `uc sync` needs
//! nothing but uc itself: `$OUT_DIR/roles.rs` holds every file as its path
//! relative to `roles/` and its bytes.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("roles");
    let mut files = Vec::new();
    walk(&root, &root, &mut files);
    files.sort();

    let mut out = String::from("pub static ROLES: &[(&str, &[u8])] = &[\n");
    for (rel, abs) in &files {
        writeln!(out, "    ({rel:?}, include_bytes!({abs:?})),").unwrap();
    }
    out.push_str("];\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("roles.rs");
    std::fs::write(dest, out).unwrap();
}

/// Every file under `dir`, as (path relative to `root`, absolute path).
/// Directories are watched, so adding or removing a file rebuilds.
fn walk(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) {
    println!("cargo:rerun-if-changed={}", dir.display());
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let meta = path.symlink_metadata().unwrap();
        if meta.file_type().is_symlink() {
            panic!(
                "{} is a symlink; put the file itself under roles/",
                path.display()
            );
        } else if meta.is_dir() {
            walk(root, &path, files);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            files.push((rel, path.to_str().unwrap().to_string()));
        }
    }
}
