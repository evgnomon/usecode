// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Build and install a Poetry project and its Ansible collection
//! (`ansible_collections/$USER/<name>`), using name/version from
//! pyproject.toml's `[tool.poetry]`.

use std::env;
use std::fs;
use std::io;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, exit};

/// `yj -t | jq -r .tool.poetry.<key>`: raw string, "null" when missing,
/// empty when pyproject.toml cannot be read or parsed.
fn poetry_field(doc: Option<&toml::Table>, key: &str) -> String {
    let Some(doc) = doc else {
        return String::new();
    };
    match doc
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get(key))
    {
        None => "null".into(),
        Some(toml::Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    }
}

fn run(prog: &str, args: &[&str]) -> i32 {
    match Command::new(prog).args(args).status() {
        Ok(s) => s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            eprintln!("ansidist: {prog}: command not found");
            127
        }
        Err(e) => {
            eprintln!("ansidist: {prog}: {e}");
            126
        }
    }
}

fn main() {
    let doc = match fs::read_to_string("pyproject.toml") {
        Ok(s) => match s.parse::<toml::Table>() {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("ansidist: pyproject.toml: {e}");
                None
            }
        },
        Err(e) => {
            eprintln!("cat: pyproject.toml: {e}");
            None
        }
    };
    let version = poetry_field(doc.as_ref(), "version");
    let name = poetry_field(doc.as_ref(), "name");
    let user = env::var("USER").unwrap_or_default();

    run("poetry", &["build"]);
    run(
        "pip",
        &["install", &format!("dist/{name}-{version}.tar.gz")],
    );

    let back = env::current_dir().ok();
    let coll = format!("ansible_collections/{user}/{name}");
    let entered = match env::set_current_dir(&coll) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("ansidist: cd: {coll}: {e}");
            false
        }
    };
    run("ansible-galaxy", &["collection", "build", "--force"]);
    let rc = run(
        "ansible-galaxy",
        &[
            "collection",
            "install",
            &format!("{user}-{name}-{version}.tar.gz"),
            "--force",
        ],
    );
    // `cd -` prints the directory it returns to.
    if entered && let Some(back) = back {
        println!("{}", back.display());
    }
    exit(rc);
}

#[cfg(test)]
mod tests {
    use super::poetry_field;

    #[test]
    fn reads_poetry_fields() {
        let t: toml::Table = "[tool.poetry]\nname = \"x\"\nversion = \"1.2\"\n"
            .parse()
            .unwrap();
        assert_eq!(poetry_field(Some(&t), "name"), "x");
        assert_eq!(poetry_field(Some(&t), "version"), "1.2");
        assert_eq!(poetry_field(Some(&t), "missing"), "null");
        assert_eq!(poetry_field(None, "name"), "");
    }
}
