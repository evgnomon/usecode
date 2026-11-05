// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Builds every `migrations/NNNN_name.sql` into the binary, in order, so
//! adding a migration is adding the file - there is no list to keep up.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let dir = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("migrations");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut by_number = BTreeMap::new();
    for entry in fs::read_dir(&dir).expect("read migrations/") {
        let path = entry.expect("read migrations/").path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(version) = name.strip_suffix(".sql") else {
            continue;
        };
        let (number, slug) = version.split_once('_').unwrap_or((version, ""));
        let well_formed = number.len() == 4
            && number.bytes().all(|b| b.is_ascii_digit())
            && !slug.is_empty()
            && slug
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if !well_formed {
            panic!("migrations/{name}: name it NNNN_lowercase_words.sql");
        }
        if let Some(other) =
            by_number.insert(number.to_string(), (version.to_string(), path.clone()))
        {
            panic!(
                "migrations/{name} and migrations/{}.sql have the same number",
                other.0
            );
        }
    }

    let mut out = String::from("&[\n");
    for (version, path) in by_number.values() {
        out.push_str(&format!(
            "    Migration {{ version: {version:?}, sql: include_str!({:?}) }},\n",
            path.display().to_string()
        ));
    }
    out.push_str("]\n");
    let dest = Path::new(&env::var("OUT_DIR").unwrap()).join("migrations.rs");
    fs::write(dest, out).expect("write migrations.rs");
}
