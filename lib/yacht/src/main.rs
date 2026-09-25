// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Run the `yacht` container on the current repository with its vault
//! password and secret file passed in the environment.

mod repofqn;

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio, exit};

fn trim_newlines(mut v: Vec<u8>) -> OsString {
    while v.last() == Some(&b'\n') {
        v.pop();
    }
    OsString::from_vec(v)
}

fn docker_args(home: &str, user: &str, playbook: &str, args: &[String]) -> Vec<String> {
    vec![
        "run".into(),
        "-it".into(),
        "--rm".into(),
        "-v.:/github/workspace".into(),
        "-v".into(),
        format!("{home}/src/github.com/{user}/config:/root/.config/blueprint"),
        "-v".into(),
        format!("{home}/.ssh:/root/.ssh"),
        "-e".into(),
        "INPUT_VAULT_PASS".into(),
        "-e".into(),
        "INPUT_VAULT".into(),
        "-e".into(),
        format!("INPUT_PLAYBOOK={playbook}"),
        "-e".into(),
        format!("INPUT_ARGS={}", args.join(" ")),
        "--workdir".into(),
        "/github/workspace".into(),
        "yacht".into(),
    ]
}

fn main() {
    let home = env::var("HOME").unwrap_or_default();
    let user = env::var("USER").unwrap_or_default();
    let addr = repofqn::repofqn();
    let secret_file = format!("{home}/src/github.com/{user}/config/secrets/{addr}.yaml");
    let vault_file = format!("{addr}.vault");

    let pass = match Command::new("vault")
        .args(["-d", &vault_file])
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
    {
        Ok(o) => o.stdout,
        Err(e) => {
            eprintln!("vault: {e}");
            Vec::new()
        }
    };
    let secrets = fs::read(&secret_file).unwrap_or_else(|e| {
        eprintln!("cat: {secret_file}: {e}");
        Vec::new()
    });
    let playbook = env::var("INPUT_PLAYBOOK")
        .ok()
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "playbooks/main.yaml".into());
    let args: Vec<String> = env::args().skip(1).collect();

    let err = Command::new("docker")
        .env("INPUT_VAULT_PASS", trim_newlines(pass))
        .env("INPUT_VAULT", trim_newlines(secrets))
        .env("INPUT_PLAYBOOK", &playbook)
        .args(docker_args(&home, &user, &playbook, &args))
        .exec();
    eprintln!("yacht: docker: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}

#[cfg(test)]
mod tests {
    use super::docker_args;

    #[test]
    fn input_args_is_one_word() {
        let a = docker_args("/h", "u", "p.yaml", &["-t".into(), "x".into()]);
        assert!(a.contains(&"INPUT_ARGS=-t x".to_string()));
        assert!(a.contains(&"INPUT_PLAYBOOK=p.yaml".to_string()));
        assert_eq!(a.last().unwrap(), "yacht");
    }
}
