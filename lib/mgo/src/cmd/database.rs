// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;
use std::process::{Command, Stdio, exit};

use mongodb::bson::doc;

use crate::config;
use crate::db::{self, fail};
use crate::pyfmt::{as_f64, py_str};

pub fn list() {
    let client = db::client();
    let run = || -> Result<(), mongodb::error::Error> {
        let mut dbs = client.list_database_names().run()?;
        if dbs.is_empty() {
            eprintln!("No databases found.");
            return Ok(());
        }
        eprintln!("Databases:");
        dbs.sort();
        for name in dbs {
            let stats = client
                .database(&name)
                .run_command(doc! {"dbStats": 1})
                .run()?;
            let size_mb = stats.get("dataSize").and_then(as_f64).unwrap_or(0.0) / (1024.0 * 1024.0);
            let collections = stats
                .get("collections")
                .map(py_str)
                .unwrap_or_else(|| "0".into());
            eprintln!("  • {name} ({collections} collections, {size_mb:.2} MB)");
        }
        Ok(())
    };
    if let Err(e) = run() {
        eprintln!("Error: {e}");
    }
}

pub fn add(name: &str) {
    let client = db::client();
    let dbs = client
        .list_database_names()
        .run()
        .unwrap_or_else(|e| fail(e));
    if dbs.iter().any(|d| d == name) {
        eprintln!("Database '{name}' already exists.");
        return;
    }
    // MongoDB creates databases lazily; create a placeholder collection.
    client
        .database(name)
        .create_collection("_init")
        .run()
        .unwrap_or_else(|e| fail(e));
    eprintln!("Database '{name}' created (with placeholder collection '_init').");
}

pub fn drop(name: &str) {
    let client = db::client();
    client
        .database(name)
        .drop()
        .run()
        .unwrap_or_else(|e| fail(e));
    eprintln!("Database '{name}' dropped.");
}

pub fn save(name: &str, output: Option<String>) {
    let output = output.unwrap_or_else(|| format!("{name}.archive.gz"));
    let file = File::create(&output).unwrap_or_else(|e| fail(e));
    let res = Command::new("mongodump")
        .args([
            "--uri",
            &config::connection_string(),
            "--db",
            name,
            "--archive",
            "--gzip",
        ])
        .stdout(file)
        .stderr(Stdio::piped())
        .output();
    match res {
        Err(e) if e.kind() == ErrorKind::NotFound => {
            eprintln!("Error: mongodump not found. Ensure MongoDB tools are installed.");
            exit(1);
        }
        Err(e) => fail(e),
        Ok(out) if !out.status.success() => {
            eprintln!(
                "Error: mongodump failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            exit(1);
        }
        Ok(_) => eprintln!("Database '{name}' saved to '{output}'."),
    }
}

/// Database name inferred from an archive filename (`name.archive.gz` → `name`).
fn infer_name(input: &Path) -> String {
    let base = input
        .file_name()
        .map(|b| b.to_string_lossy().into_owned())
        .unwrap_or_default();
    base.split('.').next().unwrap_or_default().to_string()
}

pub fn restore(input: &Path, name: Option<String>) {
    let name = name.unwrap_or_else(|| {
        let n = infer_name(input);
        if n.is_empty() {
            eprintln!("Error: Could not infer database name from filename. Use --name.");
            exit(1);
        }
        n
    });
    // The original passed `--archive <file>`, which mongorestore parses as
    // "read the archive from stdin"; the value must be attached with `=`.
    let archive = format!("--archive={}", input.to_string_lossy());
    let res = Command::new("mongorestore")
        .args([
            "--uri",
            &config::connection_string(),
            "--db",
            &name,
            &archive,
            "--gzip",
        ])
        .output();
    match res {
        Err(e) if e.kind() == ErrorKind::NotFound => {
            eprintln!("Error: mongorestore not found. Ensure MongoDB tools are installed.");
            exit(1);
        }
        Err(e) => fail(e),
        Ok(out) if !out.status.success() => {
            eprintln!(
                "Error: mongorestore failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            exit(1);
        }
        Ok(_) => eprintln!("Database '{name}' restored from '{}'.", input.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(infer_name(Path::new("/x/foo.archive.gz")), "foo");
        assert_eq!(infer_name(Path::new(".hidden")), "");
    }
}
