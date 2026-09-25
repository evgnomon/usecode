// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Run the repository's `playbooks/main.yaml` (or $INPUT_PLAYBOOK) with
//! ansible-playbook, handing it the vault password and decrypted-at-runtime
//! secret file through `/dev/fd` pipes.

mod repofqn;
mod vault;

use std::env;
use std::fs;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::{Command, Stdio, exit};

/// Like `$(cmd)`: stdout with trailing newlines removed, stderr passed through.
fn capture(cmd: &mut Command) -> Vec<u8> {
    let mut out = match cmd
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
    {
        Ok(o) => o.stdout,
        Err(e) => {
            vault::spawn_failed(&cmd.get_program().to_string_lossy(), &e);
            Vec::new()
        }
    };
    while out.last() == Some(&b'\n') {
        out.pop();
    }
    out
}

fn git_ref_name() -> String {
    let out = capture(Command::new("git").args(["symbolic-ref", "--short", "HEAD"]));
    String::from_utf8_lossy(&out).into_owned()
}

fn nonempty(key: &str) -> Option<String> {
    env::var(key).ok().filter(|v| !v.is_empty())
}

fn main() {
    if !Path::new("playbooks/main.yaml").is_file() {
        // The original script tried to run the message as a command.
        eprintln!("playbooks/main.yaml not found");
        exit(1);
    }

    // SAFETY: no other threads exist yet.
    unsafe {
        env::set_var("EDITOR", "vi");
        env::set_var("ANSIBLE_STDOUT_CALLBACK", "yaml");
    }

    let addr = repofqn::repofqn();
    let secret_file = format!("{}/{addr}.yaml", vault::secrets_dir());
    let vault_file = format!("{addr}.vault");

    if nonempty("YACHT_EVENT_NAME").is_none() {
        // SAFETY: no other threads exist yet.
        unsafe { env::set_var("YACHT_EVENT_NAME", "push") };
    }
    if nonempty("YACHT_REF_NAME").is_none() {
        let r = git_ref_name();
        // SAFETY: no other threads exist yet.
        unsafe { env::set_var("YACHT_REF_NAME", r) };
    }

    vault::ensure(&vault_file, &secret_file);

    let pass = capture(Command::new("vault").args(["-d", &vault_file]));
    let mut secrets = fs::read(&secret_file).unwrap_or_else(|e| {
        eprintln!("cat: {secret_file}: {e}");
        Vec::new()
    });
    while secrets.last() == Some(&b'\n') {
        secrets.pop();
    }
    let playbook = nonempty("INPUT_PLAYBOOK").unwrap_or_else(|| "playbooks/main.yaml".into());
    let ref_name = git_ref_name();
    let collections = format!(
        "{}/.ansible/collections/ansible_collections:{}",
        env::var("HOME").unwrap_or_default(),
        env::var("ANSIBLE_COLLECTIONS_PATH").unwrap_or_default()
    );

    let pipes = vault::data_sub(pass).and_then(|p| Ok((p, vault::data_sub(secrets)?)));
    let ((pass_thread, pass_r), (vault_thread, vault_r)) = match pipes {
        Ok(x) => x,
        Err(e) => {
            eprintln!("y: pipe: {e}");
            exit(1);
        }
    };

    let mut cmd = Command::new("ansible-playbook");
    cmd.env("YACHT_REF_NAME", ref_name)
        .env("ANSIBLE_COLLECTIONS_PATH", collections)
        .env("ANSIBLE_VAULT_PASSWORD_FILE", vault::fd_path(&pass_r))
        .env("INPUT_VAULT_FILE", vault::fd_path(&vault_r))
        .arg("--ask-become-pass")
        .arg(&playbook)
        .args(env::args_os().skip(1));
    vault::pass_fds(&mut cmd, vec![pass_r.as_raw_fd(), vault_r.as_raw_fd()]);
    let rc = vault::status(&mut cmd);
    drop(cmd);
    drop(pass_r);
    drop(vault_r);
    let _ = pass_thread.join();
    let _ = vault_thread.join();
    exit(rc);
}
