// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Shared helpers for the vault/ansible-vault secret workflow.

use std::env;
use std::io::{self, PipeReader};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, ExitStatus};

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

#[cfg(test)]
mod tests {
    use super::*;

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
