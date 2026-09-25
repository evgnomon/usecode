// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Shared helpers for the vault/ansible-vault secret workflow.

use std::env;
use std::fs::File;
use std::io::{self, PipeReader, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};

/// `$HOME/src/github.com/$USER/config/secrets`
pub fn secrets_dir() -> String {
    format!(
        "{}/src/github.com/{}/config/secrets",
        env::var("HOME").unwrap_or_default(),
        env::var("USER").unwrap_or_default()
    )
}

/// Shell-style exit code of a finished child.
pub fn code(s: ExitStatus) -> i32 {
    s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0))
}

/// Report a failed spawn like a shell would and return its exit code.
pub fn spawn_failed(prog: &str, e: &io::Error) -> i32 {
    if e.kind() == io::ErrorKind::NotFound {
        if prog.contains('/') {
            eprintln!("{prog}: No such file or directory");
        } else {
            eprintln!("{prog}: command not found");
        }
        127
    } else {
        eprintln!("{prog}: {e}");
        126
    }
}

fn prog(cmd: &Command) -> String {
    cmd.get_program().to_string_lossy().into_owned()
}

/// Run to completion and return the shell-style exit code.
pub fn status(cmd: &mut Command) -> i32 {
    match cmd.status() {
        Ok(s) => code(s),
        Err(e) => spawn_failed(&prog(cmd), &e),
    }
}

/// 64 random characters from [a-zA-Z0-9] followed by a newline
/// (replaces `cat /dev/random | tr -dc a-zA-Z0-9 | fold -w 64 | head -n 1`).
pub fn password() -> io::Result<String> {
    let mut f = File::open("/dev/urandom")?;
    let mut out = String::with_capacity(65);
    let mut buf = [0u8; 256];
    while out.len() < 64 {
        f.read_exact(&mut buf)?;
        for &b in buf.iter().filter(|b| b.is_ascii_alphanumeric()) {
            if out.len() == 64 {
                break;
            }
            out.push(char::from(b));
        }
    }
    out.push('\n');
    Ok(out)
}

/// Pipe `password` into a spawned command's stdin and wait for it.
fn feed(mut cmd: Command, data: &[u8]) -> i32 {
    let mut child = match cmd.stdin(Stdio::piped()).spawn() {
        Ok(c) => c,
        Err(e) => return spawn_failed(&prog(&cmd), &e),
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(data);
    }
    match child.wait() {
        Ok(s) => code(s),
        Err(e) => {
            eprintln!("{}: {e}", prog(&cmd));
            1
        }
    }
}

/// `<random password> | vault -e <name>`
pub fn vault_encrypt_new(name: &str) -> i32 {
    match password() {
        Ok(p) => {
            let mut cmd = Command::new("vault");
            cmd.args(["-e", name]);
            feed(cmd, p.as_bytes())
        }
        Err(e) => {
            eprintln!("/dev/urandom: {e}");
            1
        }
    }
}

/// Bash `<(producer)`: the producer's stdout goes into a pipe whose read end
/// is returned. The producer is dropped so the parent keeps no write end.
pub fn proc_sub(mut producer: Command) -> io::Result<(Option<Child>, PipeReader)> {
    let (r, w) = io::pipe()?;
    producer.stdout(w);
    let child = match producer.spawn() {
        Ok(c) => Some(c),
        Err(e) => {
            spawn_failed(&prog(&producer), &e);
            None
        }
    };
    drop(producer);
    Ok((child, r))
}

/// `/dev/fd/N` path for a pipe read end.
pub fn fd_path(r: &PipeReader) -> String {
    format!("/dev/fd/{}", r.as_raw_fd())
}

/// Let `cmd` inherit `fds` (clears FD_CLOEXEC in the child only).
pub fn pass_fds(cmd: &mut Command, fds: Vec<RawFd>) {
    // SAFETY: the closure only calls the async-signal-safe fcntl(2).
    unsafe {
        cmd.pre_exec(move || {
            for &fd in &fds {
                if libc::fcntl(fd, libc::F_SETFD, 0) == -1 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
}

/// `<(vault -d <name>)`
pub fn vault_decrypt_sub(name: &str) -> io::Result<(Option<Child>, PipeReader)> {
    let mut v = Command::new("vault");
    v.args(["-d", name]);
    proc_sub(v)
}

/// Wait for a process-substitution producer, if it was started.
pub fn reap(producer: Option<Child>) {
    if let Some(mut c) = producer {
        let _ = c.wait();
    }
}

/// `ANSIBLE_VAULT_PASSWORD_FILE=<(vault -d <name>) cmd`
pub fn run_with_vault_password(mut cmd: Command, name: &str) -> i32 {
    let (producer, r) = match vault_decrypt_sub(name) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("pipe: {e}");
            return 1;
        }
    };
    cmd.env("ANSIBLE_VAULT_PASSWORD_FILE", fd_path(&r));
    pass_fds(&mut cmd, vec![r.as_raw_fd()]);
    let rc = status(&mut cmd);
    drop(cmd);
    drop(r);
    reap(producer);
    rc
}

/// Create the vault password (`<name>.asc`) and the encrypted secret file
/// when they do not exist yet.
pub fn ensure(vault_file: &str, secret_file: &str) {
    if !Path::new(&format!("{}/{vault_file}.asc", secrets_dir())).is_file() {
        vault_encrypt_new(vault_file);
    }
    if !Path::new(secret_file).is_file() {
        let mut c = Command::new("ansible-vault");
        c.args(["create", secret_file]);
        run_with_vault_password(c, vault_file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_shape() {
        let p = password().unwrap();
        assert_eq!(p.len(), 65);
        assert!(p.ends_with('\n'));
        assert!(p[..64].bytes().all(|b| b.is_ascii_alphanumeric()));
        assert_ne!(p, password().unwrap());
    }

    #[test]
    fn proc_sub_reaches_child_via_dev_fd() {
        let mut producer = Command::new("printf");
        producer.arg("secret");
        let (p, r) = proc_sub(producer).unwrap();
        let mut cat = Command::new("cat");
        cat.arg(fd_path(&r));
        pass_fds(&mut cat, vec![r.as_raw_fd()]);
        let out = cat.output().unwrap();
        drop(cat);
        drop(r);
        reap(p);
        assert_eq!(out.stdout, b"secret");
    }
}
