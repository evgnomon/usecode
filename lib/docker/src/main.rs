// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Docker wrapper: uses `$HOME/.docker` as config and falls back to sudo
//! only when the daemon's unix socket is not writable by the current user.

use std::env;
use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, exit};

const DOCKER_BIN: &str = "/usr/bin/docker";

/// Socket path derived from DOCKER_HOST: only `unix://` hosts yield a path.
fn socket_from_host(host: &str) -> Option<&str> {
    host.strip_prefix("unix://").filter(|s| !s.is_empty())
}

fn is_socket(p: &str) -> bool {
    std::fs::metadata(p)
        .map(|m| m.file_type().is_socket())
        .unwrap_or(false)
}

fn writable(p: &str) -> bool {
    let Ok(c) = CString::new(Path::new(p).as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `c` is a valid NUL-terminated string for the duration of the call.
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

fn main() {
    let host = env::var("DOCKER_HOST").unwrap_or_default();
    let socket = socket_from_host(&host).map(str::to_string).or_else(|| {
        ["/var/run/docker.sock", "/run/docker.sock"]
            .into_iter()
            .find(|p| is_socket(p))
            .map(str::to_string)
    });
    // SAFETY: geteuid has no preconditions.
    let euid = unsafe { libc::geteuid() };
    let config = format!("{}/.docker", env::var("HOME").unwrap_or_default());

    let use_sudo = euid != 0 && socket.as_deref().is_some_and(|s| !writable(s));
    let mut cmd = if use_sudo {
        let mut c = Command::new("sudo");
        c.arg(DOCKER_BIN);
        c
    } else {
        Command::new(DOCKER_BIN)
    };
    let err = cmd
        .arg("--config")
        .arg(config)
        .args(env::args_os().skip(1))
        .exec();
    eprintln!("docker: {}: {err}", cmd.get_program().to_string_lossy());
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}

#[cfg(test)]
mod tests {
    use super::socket_from_host;

    #[test]
    fn docker_host_parsing() {
        assert_eq!(socket_from_host("unix:///x.sock"), Some("/x.sock"));
        assert_eq!(socket_from_host("tcp://1.2.3.4:2375"), None);
        assert_eq!(socket_from_host(""), None);
    }
}
