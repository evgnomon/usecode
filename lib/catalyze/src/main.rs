// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Creates `.github/workflows/yacht.yaml` with a valid Yacht workflow and a
//! default `playbooks/main.yaml` (if missing) in the current directory.
//!
//! The delete event has no branch filtering, so the job uses an `if:`
//! condition instead.

mod templates;

use std::fs;
use std::path::Path;
use std::process::exit;

use templates::{MAIN_PLAYBOOK, YACHT_WORKFLOW};

fn write_file(path: &str, content: &str) {
    if let Err(e) = fs::write(path, content) {
        eprintln!("catalyze: {path}: {e}");
        exit(1);
    }
}

fn main() {
    let _ = fs::create_dir_all(".github/workflows");
    write_file(".github/workflows/yacht.yaml", YACHT_WORKFLOW);

    let _ = fs::create_dir_all("playbooks");
    if !Path::new("playbooks/main.yaml").is_file() {
        write_file("playbooks/main.yaml", MAIN_PLAYBOOK);
    }

    println!("Enjoy smooth Yacht automation! 🛥️");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_end_with_newline() {
        assert!(YACHT_WORKFLOW.starts_with("name: Yacht\n"));
        assert!(YACHT_WORKFLOW.ends_with("github_token: ${{ secrets.GITHUB_TOKEN }}\n"));
        assert!(MAIN_PLAYBOOK.starts_with("- name: Build\n"));
        assert!(MAIN_PLAYBOOK.ends_with("    - role: z_galaxy_col\n"));
    }
}
