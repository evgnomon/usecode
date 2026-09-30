// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The daemon's control socket, [`SOCKET_PATH`]: how a tool on the host,
//! or on the control node over ssh (`ssh HOST usecoded reload`), asks
//! the running daemon to do something and hears back how it went.
//!
//! The protocol is one line each way and a hang-up. The client sends a
//! command (`reload`); the daemon answers once it has done it, one line
//! per module - `mesh: ok`, `mesh: error: ...` - and closes. Anything
//! it doesn't know gets `error: ...`. The socket is root's alone, the
//! same as `systemctl reload usecode`, so being able to connect is the
//! authorisation.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::daemon::Event;
use crate::error::{Context, Result};

pub const SOCKET_PATH: &str = "/run/usecode/usecoded.sock";

/// How long a client gets to send its command, so one that connects and
/// says nothing can't hold up the next.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Listen on `path` in the background, handing each command to the
/// daemon's loop through `events`.
pub fn listen(path: &str, events: Sender<Event>) -> Result<()> {
    if let Some(dir) = Path::new(path).parent() {
        fs::create_dir_all(dir).with_ctx(|| format!("create {}", dir.display()))?;
    }
    // Left over from a daemon that didn't get to clean up.
    let _ = fs::remove_file(path);
    let listener = UnixListener::bind(path).with_ctx(|| format!("listen on {path}"))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_ctx(|| format!("chmod {path}"))?;

    std::thread::spawn(move || {
        for conn in listener.incoming() {
            match conn {
                Ok(stream) => accept(stream, &events),
                Err(e) => eprintln!("control: accept: {e}"),
            }
        }
    });
    Ok(())
}

fn accept(stream: UnixStream, events: &Sender<Event>) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let mut line = String::new();
    if let Err(e) = BufReader::new(&stream).take(1024).read_line(&mut line) {
        eprintln!("control: read: {e}");
        return;
    }
    match line.trim() {
        "reload" => {
            eprintln!("reloading (control socket)");
            let _ = events.send(Event::Reload(Some(stream)));
        }
        other => reply(stream, &format!("error: unknown command {other:?}\n")),
    }
}

/// Answer a client and hang up. A client that already went away is no
/// concern of the daemon's.
pub fn reply(mut stream: UnixStream, text: &str) {
    let _ = stream.write_all(text.as_bytes());
}

/// Send `command` to the daemon listening on `path` and return its
/// answer. Fails when nothing is listening there.
pub fn request(path: &str, command: &str) -> Result<String> {
    let mut stream = UnixStream::connect(path).with_ctx(|| format!("connect to {path}"))?;
    stream
        .write_all(format!("{command}\n").as_bytes())
        .and_then(|_| stream.shutdown(std::net::Shutdown::Write))
        .ctx("send to the daemon")?;
    let mut answer = String::new();
    stream
        .read_to_string(&mut answer)
        .ctx("read the daemon's answer")?;
    Ok(answer)
}

/// Whether an answer says something went wrong.
pub fn failed(answer: &str) -> bool {
    answer.is_empty()
        || answer
            .lines()
            .any(|l| l.starts_with("error:") || l.contains(": error:"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn a_reload_is_handed_to_the_loop_and_answered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run/usecoded.sock");
        let path = path.to_str().unwrap().to_string();
        let (tx, rx) = mpsc::channel();
        listen(&path, tx).unwrap();

        let client = {
            let path = path.clone();
            std::thread::spawn(move || request(&path, "reload").unwrap())
        };
        match rx.recv_timeout(Duration::from_secs(5)).unwrap() {
            Event::Reload(Some(stream)) => reply(stream, "mesh: ok\n"),
            _ => panic!("expected a reload with a client to answer"),
        }
        let answer = client.join().unwrap();
        assert_eq!(answer, "mesh: ok\n");
        assert!(!failed(&answer));
    }

    #[test]
    fn an_unknown_command_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usecoded.sock");
        let path = path.to_str().unwrap();
        let (tx, _rx) = mpsc::channel();
        listen(path, tx).unwrap();

        let answer = request(path, "dance").unwrap();
        assert!(failed(&answer), "{answer}");
    }

    #[test]
    fn a_module_error_fails_the_answer() {
        assert!(failed("mesh: error: no key\n"));
        assert!(failed(""));
    }
}
