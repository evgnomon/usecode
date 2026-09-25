// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Scaffold an Ansible role under `roles/<name>` and write a `roles.yaml`
//! playbook that runs it (tagged `never` + `<name>`).

use std::env;
use std::fs;
use std::process::exit;

const SUBDIRS: &[&str] = &[
    "tasks",
    "handlers",
    "files",
    "templates",
    "vars",
    "defaults",
    "meta",
];

fn roles_yaml(role: &str) -> String {
    format!(
        "- hosts: localhost\n  gather_facts: no\n  roles:\n    - {role}\n  tags:\n    - never\n    - {role}\n"
    )
}

fn write(path: &str, content: &str) -> bool {
    match fs::write(path, content) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("mkarole: {path}: {e}");
            false
        }
    }
}

fn main() {
    let role = env::args().nth(1).unwrap_or_default();
    if role.is_empty() {
        println!("Usage: create_ansible_role <role_name>");
        exit(1);
    }

    for d in SUBDIRS {
        let dir = format!("roles/{role}/{d}");
        if let Err(e) = fs::create_dir_all(&dir) {
            eprintln!("mkdir: cannot create directory '{dir}': {e}");
        }
    }
    write(&format!("roles/{role}/tasks/main.yaml"), "---\n");
    if !write("roles.yaml", &roles_yaml(&role)) {
        exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::roles_yaml;

    #[test]
    fn playbook_text() {
        assert_eq!(
            roles_yaml("web"),
            "- hosts: localhost\n  gather_facts: no\n  roles:\n    - web\n  tags:\n    - never\n    - web\n"
        );
    }
}
