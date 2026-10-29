// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc agent serve`: the coding agent in a web page, as on a terminal.
//!
//! uc runs on a pseudo-terminal of its own, exactly as it does in a terminal
//! window, and `/` is a page with a terminal for it: restty
//! (<https://github.com/wiedymi/restty>), libghostty-vt in WebAssembly drawn
//! with WebGPU or WebGL2. The page and the server speak restty's PTY protocol
//! over a WebSocket at `/ws`: what uc prints goes out as binary frames, and
//! the page sends JSON text frames,
//!
//! - `{"type": "input", "data": "..."}`: keys and pastes, as a terminal sends;
//! - `{"type": "resize", "cols": 120, "rows": 32}`: the page's size.
//!
//! The one uc is shared: a client connecting later (a page reloaded, a second
//! device) gets the output so far first, keys from all of them go to it, and
//! the last to resize sets the size. uc goes on when the page closes, and
//! starts again if it exits.
//!
//! It listens on 127.0.0.1:4777 unless `--listen` (`UC_LISTEN`, `listen:`
//! under `uc:`) says otherwise. Anyone who can reach the port drives a shell
//! as this user, so on any address but loopback it wants a token,
//! `UC_SERVE_TOKEN`, sent as `Authorization: Bearer <token>` or `?token=`.
//! `/health` says ok; `/fonts/` has the page's font, Cascadia Mono Nerd Font.

use anyhow::{Context, Result, bail};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const DEFAULT_LISTEN: &str = "127.0.0.1:4777";
const PAGE: &str = include_str!("serve.html");
/// Cascadia Mono Nerd Font (CaskaydiaMono NF Mono, SIL OFL), the console's
/// own copy: the letters in three weights and the Nerd Font icons. They are
/// WOFF2 with no glyf transform (fontTools' `transform_tables=()`): restty
/// 0.3.0 decodes the transformed glyf table wrong, and glyphs reaching past
/// the font's usual box, like ₿, come out twice their size.
const FONTS: &[(&str, &[u8])] = &[
    (
        "CaskaydiaMonoNF-Regular.woff2",
        include_bytes!("../../../console/src/fonts/CaskaydiaMonoNF-Regular.woff2"),
    ),
    (
        "CaskaydiaMonoNF-SemiBold.woff2",
        include_bytes!("../../../console/src/fonts/CaskaydiaMonoNF-SemiBold.woff2"),
    ),
    (
        "CaskaydiaMonoNF-Bold.woff2",
        include_bytes!("../../../console/src/fonts/CaskaydiaMonoNF-Bold.woff2"),
    ),
    (
        "CaskaydiaMonoNF-icons.woff2",
        include_bytes!("../../../console/src/fonts/CaskaydiaMonoNF-icons.woff2"),
    ),
    (
        "OFL.txt",
        include_bytes!("../../../console/src/fonts/OFL.txt"),
    ),
];
const USAGE: &str = "usage: uc agent serve [--listen ADDR] [--ask] [--output] [--commands] \
                     [--no-shell] [--parallel|--exa|--duckduckgo]";
/// The flags handed on to the uc on the terminal.
const PASSED: &[&str] = &[
    "--ask",
    "--output",
    "--commands",
    "--no-shell",
    "--parallel",
    "--exa",
    "--duckduckgo",
];
/// The output kept for clients that connect later, up to about this many bytes.
const MAX_LOG: usize = 2 << 20;

/// What the clients send the terminal.
enum Input {
    Bytes(Vec<u8>),
    Resize(u16, u16),
    /// The server is going: uc is stopped.
    Quit,
}

/// The output so far and the clients to send what comes next to.
#[derive(Default)]
struct Hub {
    log: Vec<u8>,
    clients: Vec<mpsc::UnboundedSender<Vec<u8>>>,
}

impl Hub {
    fn push(&mut self, bytes: &[u8]) {
        self.clients.retain(|tx| tx.send(bytes.to_vec()).is_ok());
        self.log.extend_from_slice(bytes);
        if self.log.len() > MAX_LOG {
            // Down to three quarters, from the start of a line, so a replay
            // rarely begins in the middle of an escape sequence.
            let mut cut = self.log.len() - MAX_LOG * 3 / 4;
            if let Some(nl) = self.log[cut..].iter().position(|&b| b == b'\n') {
                cut += nl + 1;
            }
            self.log.drain(..cut);
        }
    }
}

/// What the handlers share.
struct Shared {
    input: Mutex<Sender<Input>>,
    hub: Arc<Mutex<Hub>>,
    token: Option<String>,
}

impl Shared {
    fn send(&self, input: Input) {
        let tx = self.input.lock().unwrap_or_else(|e| e.into_inner());
        drop(tx.send(input));
    }
}

pub(crate) fn main(args: &[OsString]) -> ExitCode {
    let args: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}\n");
        println!("Runs uc on a terminal of its own, for a web page: open the address in a");
        println!("browser and it is uc, as in a terminal window, drawn by restty. It is one");
        println!("uc for every page, going on when they close. It listens on {DEFAULT_LISTEN};");
        println!("anywhere else it wants UC_SERVE_TOKEN set, and the page opened with");
        println!("?token=<token>. The other flags are uc's own, for the uc it runs.");
        return ExitCode::SUCCESS;
    }
    let mut given = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--listen" => given = rest.next().cloned(),
            arg if arg.starts_with("--listen=") => given = Some(arg["--listen=".len()..].into()),
            arg if PASSED.contains(&arg) => {}
            other => {
                eprintln!("uc agent serve: unknown argument '{other}'\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let listen = crate::flag::value(given, "--listen").unwrap_or_else(|| DEFAULT_LISTEN.into());
    // Given here or set in the environment or config: the uc on the terminal
    // reads the same environment and config, so only the command line's go on.
    let passed: Vec<String> = PASSED
        .iter()
        .filter(|f| args.iter().any(|a| a == *f))
        .map(|f| f.to_string())
        .collect();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");
    match runtime.block_on(run(&listen, passed)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc agent serve: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(listen: &str, passed: Vec<String>) -> Result<()> {
    let addr: SocketAddr = listen
        .parse()
        .with_context(|| format!("--listen {listen}: not an address like {DEFAULT_LISTEN}"))?;
    let token = std::env::var("UC_SERVE_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());
    if token.is_none() && !addr.ip().is_loopback() {
        bail!("listening on {addr} lets anyone there run commands: set UC_SERVE_TOKEN");
    }
    // `uc agent serve` is uc-agent; the chat is `uc` itself, next to it.
    let exe = crate::dispatch::resolve("uc")
        .context("'uc' is not installed (not found next to uc-agent or on PATH)")?;
    // The first uc is started before listening, so a broken one shows here.
    let first = Terminal::open(&exe, &passed, (24, 80))?;
    let hub = Arc::new(Mutex::new(Hub::default()));
    let (input, inputs) = std::sync::mpsc::channel();
    let supervisor = {
        let hub = hub.clone();
        std::thread::spawn(move || supervise(first, exe, passed, inputs, hub))
    };
    let shared = Arc::new(Shared {
        input: Mutex::new(input),
        hub,
        token,
    });
    let app = axum::Router::new()
        .route("/", get(page))
        .route("/health", get(|| async { "ok" }))
        .route("/fonts/{name}", get(font))
        .route("/ws", get(socket))
        .with_state(shared.clone());
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("listening on {addr}"))?;
    eprintln!("uc agent serve: listening on http://{addr}");
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            eprintln!("uc agent serve: {err}");
            std::process::exit(1);
        }
    });
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .context("watching for SIGTERM")?;
    let mut hangup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
        .context("watching for SIGHUP")?;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
        _ = hangup.recv() => {}
    }
    shared.send(Input::Quit);
    drop(tokio::task::spawn_blocking(move || supervisor.join()).await);
    Ok(())
}

/// uc on a pseudo-terminal.
struct Terminal {
    master: File,
    child: Child,
    /// The reader is done: uc and everything it started closed the terminal.
    closed: Receiver<()>,
    started: Instant,
}

impl Terminal {
    /// Starts uc on a new terminal of `rows` by `cols`; what it prints goes
    /// to `hub` once [`Terminal::read_into`] is called.
    fn open(exe: &std::path::Path, args: &[String], (rows, cols): (u16, u16)) -> Result<Self> {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let (mut master, mut slave) = (-1, -1);
        // SAFETY: openpty writes the two descriptors it opens; the name and
        // termios pointers may be null.
        let opened = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        };
        if opened != 0 {
            return Err(std::io::Error::last_os_error()).context("opening a pseudo-terminal");
        }
        // SAFETY: both were just opened and nothing else owns them.
        let (master, slave) =
            unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
        // SAFETY: setting close-on-exec on a descriptor we own, so uc's own
        // commands don't hold the terminal open.
        unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) };
        let mut command = Command::new(exe);
        command
            .arg0("uc")
            .args(args)
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .env_remove("UC_SERVE_TOKEN")
            .stdin(Stdio::from(slave.try_clone()?))
            .stdout(Stdio::from(slave.try_clone()?))
            .stderr(Stdio::from(slave));
        // SAFETY: only async-signal-safe calls between fork and exec. uc
        // leads a session of its own with the terminal as its controlling
        // one, so Ctrl-C and resizes reach it as on any terminal.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().context("starting uc")?;
        drop(command);
        let (_, closed) = std::sync::mpsc::channel();
        Ok(Terminal {
            master: File::from(master),
            child,
            closed,
            started: Instant::now(),
        })
    }

    /// Reads what uc prints into `hub`, on a thread of its own.
    fn read_into(&mut self, hub: Arc<Mutex<Hub>>) -> Result<()> {
        let mut reader = self.master.try_clone()?;
        let (done, closed) = std::sync::mpsc::channel();
        self.closed = closed;
        std::thread::spawn(move || {
            let mut buf = vec![0; 64 << 10];
            // EIO once the last process with the terminal open is gone.
            while let Ok(n @ 1..) = reader.read(&mut buf) {
                hub.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(&buf[..n]);
            }
            let _ = done.send(());
        });
        Ok(())
    }

    fn resize(&self, rows: u16, cols: u16) {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: TIOCSWINSZ reads a winsize; the kernel tells uc with SIGWINCH.
        unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &size) };
    }

    /// Whether uc is gone and the terminal has nothing more to read.
    fn finished(&mut self) -> bool {
        let exited = matches!(self.child.try_wait(), Ok(Some(_)));
        exited
            && !matches!(
                self.closed.recv_timeout(Duration::from_millis(200)),
                Err(RecvTimeoutError::Timeout)
            )
    }

    /// Stops uc as Ctrl-C would, which stops its jobs too, then for good.
    fn stop(mut self) {
        let pid = self.child.id() as libc::pid_t;
        for signal in [libc::SIGINT, libc::SIGINT, libc::SIGKILL] {
            // SAFETY: signalling uc, which leads its own process group.
            unsafe {
                if signal == libc::SIGKILL {
                    libc::kill(-pid, signal);
                } else {
                    libc::kill(pid, signal);
                }
            }
            let deadline = Instant::now() + Duration::from_secs(1);
            while Instant::now() < deadline {
                if matches!(self.child.try_wait(), Ok(Some(_))) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        drop(self.child.wait());
    }
}

/// Keeps uc running: hands it the clients' keys and sizes, and starts it
/// again when it exits, waiting longer each time it exits straight away.
fn supervise(
    mut terminal: Terminal,
    exe: std::path::PathBuf,
    args: Vec<String>,
    inputs: Receiver<Input>,
    hub: Arc<Mutex<Hub>>,
) {
    let mut size = (24, 80);
    let mut pause = Duration::from_secs(1);
    if let Err(err) = terminal.read_into(hub.clone()) {
        eprintln!("uc agent serve: reading the terminal: {err:#}");
    }
    loop {
        match inputs.recv_timeout(Duration::from_millis(250)) {
            Ok(Input::Bytes(bytes)) => {
                // A full terminal blocks until uc reads; keys can wait too.
                drop(terminal.master.write_all(&bytes));
            }
            Ok(Input::Resize(rows, cols)) if rows > 0 && cols > 0 => {
                size = (rows, cols);
                terminal.resize(rows, cols);
            }
            Ok(Input::Resize(..)) | Err(RecvTimeoutError::Timeout) => {}
            Ok(Input::Quit) | Err(RecvTimeoutError::Disconnected) => {
                terminal.stop();
                return;
            }
        }
        if !terminal.finished() {
            continue;
        }
        if terminal.started.elapsed() > Duration::from_secs(30) {
            pause = Duration::from_secs(1);
        }
        hub.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(b"\r\n\x1b[2m\xc2\xb7 uc exited; starting it again\x1b[0m\r\n");
        std::thread::sleep(pause);
        pause = (pause * 2).min(Duration::from_secs(30));
        match Terminal::open(&exe, &args, size) {
            Ok(mut next) => {
                if let Err(err) = next.read_into(hub.clone()) {
                    eprintln!("uc agent serve: reading the terminal: {err:#}");
                }
                terminal = next;
            }
            Err(err) => eprintln!("uc agent serve: {err:#}"),
        }
    }
}

/// The page, its font URLs versioned by [`FONTS`]' hash.
async fn page() -> Html<&'static str> {
    static FILLED: std::sync::LazyLock<String> =
        std::sync::LazyLock::new(|| PAGE.replace("{{FONTS_VERSION}}", &fonts_version()));
    Html(FILLED.as_str())
}

/// A short hash of all of [`FONTS`]: restty keeps fonts by URL in IndexedDB
/// for a week, so the URLs change whenever the fonts do.
fn fonts_version() -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for (name, bytes) in FONTS {
        hash.update(name.as_bytes());
        hash.update(bytes);
    }
    hash.finalize()[..6].iter().map(|b| format!("{b:02x}")).collect()
}

/// One of [`FONTS`]; they never change within a build.
async fn font(Path(name): Path<String>) -> Response {
    let Some((_, bytes)) = FONTS.iter().find(|(n, _)| *n == name) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let kind = if name.ends_with(".woff2") {
        "font/woff2"
    } else {
        "text/plain; charset=utf-8"
    };
    (
        [
            (header::CONTENT_TYPE, kind),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        *bytes,
    )
        .into_response()
}

/// Whether the request carries the token, when there is one.
fn allowed(shared: &Shared, headers: &HeaderMap, query: &HashMap<String, String>) -> bool {
    let Some(token) = &shared.token else {
        return true;
    };
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let given = bearer.or(query.get("token").map(String::as_str));
    given.is_some_and(|given| same(given.as_bytes(), token.as_bytes()))
}

/// Compares in a time that doesn't tell how much of a guess was right.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn socket(
    State(shared): State<Arc<Shared>>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !allowed(&shared, &headers, &query) {
        return (StatusCode::UNAUTHORIZED, "a token is needed\n").into_response();
    }
    upgrade.on_upgrade(move |ws| client(shared, ws))
}

/// What a client's frame asks of the terminal, in restty's PTY protocol.
fn input_of(message: Message) -> Option<Input> {
    let text = match message {
        Message::Binary(bytes) => return Some(Input::Bytes(bytes.to_vec())),
        Message::Text(text) => text,
        _ => return None,
    };
    let value: Value = serde_json::from_str(&text).ok()?;
    match value["type"].as_str()? {
        "input" => Some(Input::Bytes(value["data"].as_str()?.as_bytes().to_vec())),
        "resize" => {
            let dim = |k: &str| value[k].as_u64().map(|n| n.min(u16::MAX as u64) as u16);
            Some(Input::Resize(dim("rows")?, dim("cols")?))
        }
        _ => None,
    }
}

/// One client: the output so far, then as it comes, goes out to it; its keys
/// and size come in.
async fn client(shared: Arc<Shared>, ws: WebSocket) {
    let (mut sink, mut stream) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    {
        let mut hub = shared.hub.lock().unwrap_or_else(|e| e.into_inner());
        if !hub.log.is_empty() {
            drop(tx.send(hub.log.clone()));
        }
        hub.clients.push(tx);
    }
    let writer = tokio::spawn(async move {
        while let Some(bytes) = rx.recv().await {
            if sink.send(Message::Binary(bytes.into())).await.is_err() {
                break;
            }
        }
    });
    while let Some(Ok(message)) = stream.next().await {
        if matches!(message, Message::Close(_)) {
            break;
        }
        if let Some(input) = input_of(message) {
            shared.send(input);
        }
    }
    writer.abort();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared(token: Option<&str>) -> Shared {
        Shared {
            input: Mutex::new(std::sync::mpsc::channel().0),
            hub: Arc::default(),
            token: token.map(String::from),
        }
    }

    #[test]
    fn the_token_is_wanted_when_set() {
        let none = HashMap::new();
        let headers = HeaderMap::new();
        assert!(allowed(&shared(None), &headers, &none));
        let with = shared(Some("s3cret"));
        assert!(!allowed(&with, &headers, &none));
        let query = HashMap::from([("token".to_string(), "s3cret".to_string())]);
        assert!(allowed(&with, &headers, &query));
        let wrong = HashMap::from([("token".to_string(), "s3creT".to_string())]);
        assert!(!allowed(&with, &headers, &wrong));
        let mut bearer = HeaderMap::new();
        bearer.insert(header::AUTHORIZATION, "Bearer s3cret".parse().unwrap());
        assert!(allowed(&with, &bearer, &none));
    }

    #[test]
    fn restty_frames_are_keys_and_sizes() {
        let text = |s: &str| Message::Text(s.into());
        match input_of(text(r#"{"type":"input","data":"ls\r"}"#)) {
            Some(Input::Bytes(b)) => assert_eq!(b, b"ls\r"),
            _ => panic!("input"),
        }
        match input_of(text(
            r#"{"type":"resize","cols":120,"rows":32,"widthPx":960}"#,
        )) {
            Some(Input::Resize(32, 120)) => {}
            _ => panic!("resize"),
        }
        assert!(input_of(text("not json")).is_none());
        assert!(input_of(text(r#"{"type":"resize","cols":1}"#)).is_none());
    }

    #[test]
    fn the_log_keeps_the_end_from_a_line_start() {
        let mut hub = Hub::default();
        let line = [b'x'; 1023];
        for _ in 0..(MAX_LOG / 1024 + 10) {
            hub.push(&line);
            hub.push(b"\n");
        }
        assert!(hub.log.len() <= MAX_LOG);
        assert!(hub.log.len() >= MAX_LOG / 2);
        assert_eq!(hub.log[0], b'x');
        assert_eq!(hub.log[hub.log.len() - 1], b'\n');
    }

    #[test]
    fn the_program_gets_a_terminal_of_the_size_asked() {
        let hub = Arc::new(Mutex::new(Hub::default()));
        let sh = std::path::Path::new("/bin/sh");
        let args = [
            "-c".to_string(),
            "stty size; [ -t 0 ] && echo tty".to_string(),
        ];
        let mut terminal = Terminal::open(sh, &args, (30, 100)).unwrap();
        terminal.read_into(hub.clone()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !terminal.finished() {
            assert!(Instant::now() < deadline, "sh didn't finish");
        }
        let out = String::from_utf8_lossy(&hub.lock().unwrap().log).into_owned();
        assert!(out.contains("30 100"), "{out:?}");
        assert!(out.contains("tty"), "{out:?}");
    }
}
