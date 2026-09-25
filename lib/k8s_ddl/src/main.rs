// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Create the Kubernetes `resources` table schema using `./pg`.
//! Stops at the first failing step and exits with its status.

use std::os::unix::process::ExitStatusExt;
use std::process::{Command, exit};

const PG: &str = "./pg";

const STEPS: &[&[&str]] = &[
    &["db", "add", "z"],
    &["tab", "add", "resources"],
    // Normalized columns - frequently queried scalar fields
    &["col", "add", "resources", "api_version", "string"],
    &["col", "add", "resources", "kind", "string"],
    &["col", "add", "resources", "name", "string"],
    &[
        "col",
        "add",
        "resources",
        "namespace",
        "string",
        "--nullable",
    ],
    &["col", "add", "resources", "tenant", "string", "--nullable"],
    // JSONB columns - arbitrary key/value pairs and deeply nested structures
    &["col", "add", "resources", "labels", "jsonb", "--nullable"],
    &[
        "col",
        "add",
        "resources",
        "annotations",
        "jsonb",
        "--nullable",
    ],
    &["col", "add", "resources", "spec", "jsonb", "--nullable"],
    // Indexes
    &[
        "idx",
        "add",
        "resources",
        "api_version",
        "kind",
        "name",
        "namespace",
        "tenant",
        "--unique",
    ],
    &["idx", "add", "resources", "kind"],
    &["idx", "add", "resources", "labels"],
    &["idx", "add", "resources", "annotations"],
];

fn main() {
    for step in STEPS {
        let rc = match Command::new(PG).args(*step).status() {
            Ok(s) => s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0)),
            Err(e) => {
                eprintln!("k8s_ddl: {PG}: {e}");
                if e.kind() == std::io::ErrorKind::NotFound {
                    127
                } else {
                    126
                }
            }
        };
        if rc != 0 {
            exit(rc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::STEPS;

    #[test]
    fn starts_with_db_and_table() {
        assert_eq!(STEPS[0], ["db", "add", "z"]);
        assert_eq!(STEPS[1], ["tab", "add", "resources"]);
        assert_eq!(STEPS.len(), 14);
    }
}
