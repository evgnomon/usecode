// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Shells into this cluster's workspaces (`uc ws`), from the console.
//!
//! A browser can't reach the cluster, but a controller can reach the
//! usecode API. So a registered controller keeps one WebSocket open to it
//! (`/hosts/connect`, with the host's own secret), says on it which
//! workspaces there are, and every shell the console opens rides it: the
//! API sends `open`, `input`, `resize` and `close` for a session as JSON,
//! and the daemon answers with the terminal's output in binary frames (the
//! session's number in four bytes, big endian, then the bytes) and
//! `closed` when the shell ends. The API's side is in lib/api's relay.rs.
//!
//! A shell is `k3s kubectl exec -it` into the workspace's pod on a
//! pseudo-terminal of the daemon's, logged in as the workspace's user, as
//! `ssh` would. Nothing listens on the host and nothing is exposed: the
//! socket goes out, as the heartbeat does.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{HandshakeError, Message, WebSocket};

use crate::callhome::State;
use crate::error::{Context, Result};

/// k3s, which has kubectl in it.
const K3S: &str = "/usr/local/bin/k3s";
/// The workspaces' namespace and the label on their objects, as `uc ws`
/// makes them.
const NAMESPACE: &str = "workspace";
const LABEL: &str = "usecode.dev/workspace";

/// The most shells this host carries at once.
pub const MAX_SESSIONS: usize = 16;

/// How often the socket is pinged, and three of them silent is a dead one.
const PING: Duration = Duration::from_secs(30);
/// How often the workspaces are looked at again.
const LIST_EVERY: Duration = Duration::from_secs(30);
/// How long a read waits: short while a shell is open, so its output goes
/// out without a wait, longer while none is.
const BUSY: Duration = Duration::from_millis(15);
const IDLE: Duration = Duration::from_millis(500);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// One workspace, as the API is told.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Workspace {
    pub name: String,
    /// running, starting or stopped.
    pub state: String,
    /// Who you are in there.
    pub user: String,
    pub image: String,
}

/// Whether this host is a Kubernetes controller, which sees its cluster's
/// workspaces: k3s is here and wrote its admin kubeconfig.
pub fn controller() -> bool {
    Path::new(crate::kube::KUBECONFIG_PATH).exists() && Path::new(K3S).exists()
}

fn kubectl(args: &[&str]) -> Result<String> {
    let out = Command::new(K3S)
        .arg("kubectl")
        .args(args)
        .arg("--request-timeout=10s")
        .stdin(Stdio::null())
        .output()
        .with_ctx(|| format!("run {K3S} kubectl"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("kubectl {}: {}", args.join(" "), err.trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The cluster's id (kube-system's uid, the same from every controller)
/// and its workspaces.
pub fn workspaces() -> Result<(String, Vec<Workspace>)> {
    let cluster = kubectl(&[
        "get",
        "namespace",
        "kube-system",
        "-o",
        "jsonpath={.metadata.uid}",
    ])?;
    let list = kubectl(&[
        "get",
        "deployments",
        "-n",
        NAMESPACE,
        "-l",
        LABEL,
        "-o",
        "json",
    ])?;
    let list: Value = serde_json::from_str(&list).ctx("read the workspaces")?;
    Ok((cluster.trim().to_string(), parse_workspaces(&list)))
}

/// For the heartbeat: the workspaces, when this host is a controller and
/// Kubernetes answers.
pub fn report() -> Option<(String, Vec<Workspace>)> {
    if !controller() {
        return None;
    }
    workspaces()
        .map_err(|e| eprintln!("relay: listing the workspaces: {e}"))
        .ok()
}

/// The workspaces in a `kubectl get deployments -o json` list.
pub fn parse_workspaces(list: &Value) -> Vec<Workspace> {
    let mut all: Vec<Workspace> = list["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(workspace_of)
        .collect();
    all.sort_by(|a, b| a.name.cmp(&b.name));
    all
}

fn workspace_of(deployment: &Value) -> Option<Workspace> {
    let name = deployment["metadata"]["labels"][LABEL].as_str()?;
    if !valid_name(name) {
        return None;
    }
    let want = deployment["spec"]["replicas"].as_u64().unwrap_or(1);
    let ready = deployment["status"]["readyReplicas"].as_u64().unwrap_or(0);
    let state = match (want, ready) {
        (0, _) => "stopped",
        (_, 0) => "starting",
        _ => "running",
    };
    let container = &deployment["spec"]["template"]["spec"]["containers"][0];
    let user = container["env"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| e["name"] == "USECODE_USER")
        .and_then(|e| e["value"].as_str())
        .filter(|u| valid_user(u))
        // What the image's entrypoint makes without one.
        .unwrap_or("dev");
    Some(Workspace {
        name: name.to_string(),
        state: state.to_string(),
        user: user.to_string(),
        image: container["image"].as_str().unwrap_or_default().to_string(),
    })
}

/// A workspace name `uc ws` takes: a DNS label once `ws-` is in front.
fn valid_name(name: &str) -> bool {
    (1..=60).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

/// A login name useradd takes.
fn valid_user(name: &str) -> bool {
    let mut bytes = name.bytes();
    (1..=32).contains(&name.len())
        && bytes
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b == b'_')
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// What the shells and the lister tell the socket loop.
enum Up {
    Output(u32, Vec<u8>),
    Closed { session: u32, code: i32 },
    Workspaces(Value),
}

/// One shell: kubectl exec on a terminal of its own.
struct Session {
    master: Arc<File>,
    input: Sender<Vec<u8>>,
    pid: libc::pid_t,
    /// Set once kubectl exited and was reaped, so its pid isn't signalled.
    done: Arc<AtomicBool>,
}

impl Session {
    fn open(session: u32, name: &str, (cols, rows): (u16, u16), up: Sender<Up>) -> Result<Session> {
        if !valid_name(name) {
            bail!("{name:?} isn't a workspace name");
        }
        let object = format!("ws-{name}");
        let deployment = kubectl(&["get", "deployment", &object, "-n", NAMESPACE, "-o", "json"])
            .map_err(|_| err!("there's no workspace {name} on this cluster"))?;
        let deployment: Value = serde_json::from_str(&deployment).ctx("read the workspace")?;
        let workspace = workspace_of(&deployment)
            .ok_or_else(|| err!("there's no workspace {name} on this cluster"))?;
        if workspace.state == "stopped" {
            bail!("{name} is stopped; `uc ws start {name}` starts it");
        }

        let size = libc::winsize {
            ws_row: rows.max(1),
            ws_col: cols.max(1),
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let (mut master, mut slave) = (0, 0);
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
            bail!(
                "open a pseudo-terminal: {}",
                std::io::Error::last_os_error()
            );
        }
        // SAFETY: both were just opened and nothing else owns them.
        let (master, slave) =
            unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
        // SAFETY: close-on-exec on a descriptor we own, so other shells'
        // kubectl don't hold this terminal open.
        unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) };

        let pod = format!("deployment/{object}");
        let mut command = Command::new(K3S);
        command
            .args([
                "kubectl",
                "exec",
                "-i",
                "-t",
                "-n",
                NAMESPACE,
                &pod,
                "-c",
                "workspace",
            ])
            .args([
                "--",
                "env",
                "TERM=xterm-256color",
                "su",
                "-l",
                &workspace.user,
            ])
            .env("TERM", "xterm-256color")
            .stdin(Stdio::from(slave.try_clone()?))
            .stdout(Stdio::from(slave.try_clone()?))
            .stderr(Stdio::from(slave));
        // SAFETY: only async-signal-safe calls between fork and exec. kubectl
        // leads a session of its own with the terminal as its controlling
        // one, so it sees resizes as on any terminal.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().ctx("start kubectl exec")?;
        // Our ends of the terminal go with the command, so the master reads
        // EOF once kubectl is gone.
        drop(command);

        let master = Arc::new(File::from(master));
        let pid = child.id() as libc::pid_t;
        let done = Arc::new(AtomicBool::new(false));

        let (input, keys) = mpsc::channel::<Vec<u8>>();
        let writer = master.clone();
        std::thread::spawn(move || {
            for bytes in keys {
                if (&*writer).write_all(&bytes).is_err() {
                    break;
                }
            }
        });

        let reader = master.clone();
        let reaped = done.clone();
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                match (&*reader).read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if up.send(Up::Output(session, buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                }
            }
            let code = child
                .wait()
                .ok()
                .and_then(|status| status.code())
                .unwrap_or(-1);
            reaped.store(true, Ordering::SeqCst);
            let _ = up.send(Up::Closed { session, code });
        });

        Ok(Session {
            master,
            input,
            pid,
            done,
        })
    }

    fn write(&self, bytes: Vec<u8>) {
        let _ = self.input.send(bytes);
    }

    fn resize(&self, cols: u16, rows: u16) {
        let size = libc::winsize {
            ws_row: rows.max(1),
            ws_col: cols.max(1),
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: TIOCSWINSZ reads a winsize; the kernel tells kubectl with
        // SIGWINCH, and kubectl tells the pod.
        unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &size) };
    }

    /// Hang up on the shell, and kill kubectl if that doesn't end it.
    fn end(&self) {
        let (pid, done) = (self.pid, self.done.clone());
        let signal = move |sig| {
            if !done.load(Ordering::SeqCst) {
                // SAFETY: kubectl leads its own process group (setsid above).
                unsafe { libc::kill(-pid, sig) };
            }
        };
        signal(libc::SIGHUP);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(3));
            signal(libc::SIGKILL);
        });
    }
}

/// How a connection ended.
pub enum Ended {
    /// Asked to stop.
    Stopped,
    /// The API turned the host's secret down.
    Refused(String),
}

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

/// The relay socket's address, from the API's.
fn socket_url(endpoint: &str) -> String {
    let endpoint = endpoint.trim_end_matches('/');
    let url = match endpoint.split_once("://") {
        Some(("http", rest)) => format!("ws://{rest}"),
        Some((_, rest)) => format!("wss://{rest}"),
        None => format!("wss://{endpoint}"),
    };
    format!("{url}/hosts/connect")
}

fn connect(state: &State) -> Result<std::result::Result<Socket, String>> {
    crate::tls::install();
    let url = socket_url(&state.endpoint);
    let mut request = url
        .as_str()
        .into_client_request()
        .with_ctx(|| format!("relay address {url}"))?;
    let headers = request.headers_mut();
    let auth = format!("Bearer {}", state.secret);
    headers.insert("authorization", auth.parse().ctx("host secret")?);
    headers.insert(
        "user-agent",
        concat!("usecoded/", env!("CARGO_PKG_VERSION"))
            .parse()
            .ctx("user agent")?,
    );
    let host = request
        .uri()
        .host()
        .ok_or_else(|| err!("no host in {url}"))?
        .trim_matches(['[', ']'])
        .to_string();
    let port = request
        .uri()
        .port_u16()
        .unwrap_or(if url.starts_with("wss:") { 443 } else { 80 });
    let addrs: Vec<_> = (host.as_str(), port)
        .to_socket_addrs()
        .with_ctx(|| format!("look {host} up"))?
        .collect();
    let mut last = None;
    let mut stream = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) => last = Some(e),
        }
    }
    let stream = stream.ok_or_else(|| {
        err!(
            "reach {host}:{port}: {}",
            last.map(|e| e.to_string())
                .unwrap_or_else(|| "no address".into())
        )
    })?;
    stream.set_read_timeout(Some(CONNECT_TIMEOUT))?;
    stream.set_nodelay(true)?;
    match tungstenite::client_tls(request, stream) {
        Ok((socket, _)) => Ok(Ok(socket)),
        Err(HandshakeError::Failure(tungstenite::Error::Http(answer)))
            if answer.status() == 401 =>
        {
            let body = answer.body().as_deref().unwrap_or_default();
            let detail: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
            Ok(Err(detail["detail"].as_str().unwrap_or("401").to_string()))
        }
        Err(HandshakeError::Failure(e)) => bail!("{url}: {e}"),
        Err(HandshakeError::Interrupted(_)) => bail!("{url}: the handshake timed out"),
    }
}

fn tcp(socket: &Socket) -> Option<&TcpStream> {
    #[allow(unreachable_patterns)]
    match socket.get_ref() {
        MaybeTlsStream::Plain(s) => Some(s),
        MaybeTlsStream::Rustls(s) => Some(s.get_ref()),
        _ => None,
    }
}

fn timed_out(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(e)
        if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut))
}

/// Connect, then carry the shells until the socket drops (an error), the
/// API turns the host down, or `stop` is set. `connected` is called once
/// the socket is open.
pub fn serve(state: &State, stop: &AtomicBool, connected: impl FnOnce()) -> Result<Ended> {
    let mut socket = match connect(state)? {
        Ok(socket) => socket,
        Err(detail) => return Ok(Ended::Refused(detail)),
    };
    eprintln!("relay: connected to {}", state.endpoint);
    connected();

    let (up, ups) = mpsc::channel::<Up>();
    let lister_stop = Arc::new(AtomicBool::new(false));
    spawn_lister(up.clone(), lister_stop.clone());
    let result = carry(&mut socket, stop, up, ups);
    lister_stop.store(true, Ordering::SeqCst);
    let _ = socket.close(None);
    let _ = socket.flush();
    result
}

/// Look at the workspaces every [`LIST_EVERY`] until told to stop.
fn spawn_lister(up: Sender<Up>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let mut failing = false;
        while !stop.load(Ordering::SeqCst) {
            match workspaces() {
                Ok((cluster, list)) => {
                    failing = false;
                    let message = json!({
                        "type": "workspaces", "cluster_id": cluster, "workspaces": list,
                    });
                    if up.send(Up::Workspaces(message)).is_err() {
                        return;
                    }
                }
                Err(e) if !failing => {
                    failing = true;
                    eprintln!("relay: listing the workspaces: {e}");
                }
                Err(_) => {}
            }
            let until = Instant::now() + LIST_EVERY;
            while Instant::now() < until && !stop.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    });
}

fn carry(
    socket: &mut Socket,
    stop: &AtomicBool,
    up: Sender<Up>,
    ups: Receiver<Up>,
) -> Result<Ended> {
    let mut sessions = HashMap::new();
    let result = carry_shells(socket, stop, &up, &ups, &mut sessions);
    for (_, shell) in sessions.drain() {
        shell.end();
    }
    result
}

fn carry_shells(
    socket: &mut Socket,
    stop: &AtomicBool,
    up: &Sender<Up>,
    ups: &Receiver<Up>,
    sessions: &mut HashMap<u32, Session>,
) -> Result<Ended> {
    let mut heard = Instant::now();
    let mut pinged = Instant::now();
    let mut listed: Option<Value> = None;
    let mut busy: Option<bool> = None;
    loop {
        if stop.load(Ordering::SeqCst) {
            return Ok(Ended::Stopped);
        }
        let open = !sessions.is_empty();
        if busy != Some(open) {
            if let Some(tcp) = tcp(socket) {
                tcp.set_read_timeout(Some(if open { BUSY } else { IDLE }))?;
            }
            busy = Some(open);
        }

        match socket.read() {
            Ok(Message::Text(text)) => {
                heard = Instant::now();
                if let Some(answer) = from_api(sessions, up, text.as_str()) {
                    socket
                        .send(Message::Text(answer.to_string().into()))
                        .ctx("relay")?;
                }
            }
            Ok(Message::Close(_)) => bail!("the API closed the relay"),
            Ok(_) => heard = Instant::now(),
            Err(e) if timed_out(&e) => {}
            Err(e) => bail!("relay: {e}"),
        }

        while let Ok(next) = ups.try_recv() {
            let message = match next {
                Up::Output(session, bytes) => {
                    if !sessions.contains_key(&session) {
                        continue;
                    }
                    let mut frame = Vec::with_capacity(4 + bytes.len());
                    frame.extend_from_slice(&session.to_be_bytes());
                    frame.extend_from_slice(&bytes);
                    Message::Binary(frame.into())
                }
                Up::Closed { session, code } => {
                    if sessions.remove(&session).is_none() {
                        continue;
                    }
                    let closed = json!({"type": "closed", "session": session, "code": code});
                    Message::Text(closed.to_string().into())
                }
                Up::Workspaces(list) => {
                    if listed.as_ref() == Some(&list) {
                        continue;
                    }
                    let text = list.to_string();
                    listed = Some(list);
                    Message::Text(text.into())
                }
            };
            socket.send(message).ctx("relay")?;
        }

        if pinged.elapsed() >= PING {
            socket.send(Message::Ping(Vec::new().into())).ctx("relay")?;
            pinged = Instant::now();
        }
        if heard.elapsed() > PING * 3 {
            bail!("the API went quiet");
        }
    }
}

/// Do what a text frame from the API asks; the answer to send back, if any.
fn from_api(sessions: &mut HashMap<u32, Session>, up: &Sender<Up>, text: &str) -> Option<Value> {
    let message: Value = serde_json::from_str(text).ok()?;
    let session = message["session"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())?;
    let dim = |k: &str, default: u16| {
        message[k]
            .as_u64()
            .map(|n| n.clamp(1, 1000) as u16)
            .unwrap_or(default)
    };
    match message["type"].as_str()? {
        "open" => {
            let refuse =
                |reason: String| json!({"type": "closed", "session": session, "reason": reason});
            if sessions.contains_key(&session) {
                return None;
            }
            if sessions.len() >= MAX_SESSIONS {
                return Some(refuse(format!(
                    "This host carries {MAX_SESSIONS} shells already; close one first."
                )));
            }
            let name = message["workspace"].as_str().unwrap_or_default();
            let size = (dim("cols", 80), dim("rows", 24));
            match Session::open(session, name, size, up.clone()) {
                Ok(shell) => {
                    sessions.insert(session, shell);
                    None
                }
                Err(e) => Some(refuse(e.to_string())),
            }
        }
        "input" => {
            let data = message["data"].as_str()?;
            sessions.get(&session)?.write(data.as_bytes().to_vec());
            None
        }
        "resize" => {
            sessions
                .get(&session)?
                .resize(dim("cols", 80), dim("rows", 24));
            None
        }
        "close" => {
            if let Some(shell) = sessions.remove(&session) {
                shell.end();
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_socket_is_next_to_the_api() {
        assert_eq!(
            socket_url("https://api.usecode.dev/"),
            "wss://api.usecode.dev/hosts/connect"
        );
        assert_eq!(
            socket_url("http://localhost:8000"),
            "ws://localhost:8000/hosts/connect"
        );
    }

    #[test]
    fn names_and_users_are_what_uc_ws_makes() {
        assert!(valid_name("dev"));
        assert!(!valid_name("../dev"));
        assert!(!valid_name("-x"));
        assert!(valid_user("ada"));
        assert!(valid_user("_svc-1"));
        assert!(!valid_user("root;rm"));
        assert!(!valid_user("-l"));
        assert!(!valid_user(""));
    }

    #[test]
    fn reads_the_workspaces_off_their_deployments() {
        let list = json!({"items": [
            {"metadata": {"labels": {LABEL: "web"}},
             "spec": {"replicas": 0, "template": {"spec": {"containers": [{"image": "i"}]}}},
             "status": {}},
            {"metadata": {"labels": {LABEL: "dev"}},
             "spec": {"replicas": 1, "template": {"spec": {"containers": [{"image": "base:1",
                "env": [{"name": "USECODE_USER", "value": "ada"}]}]}}},
             "status": {"readyReplicas": 1}},
            {"metadata": {"labels": {LABEL: "odd"}},
             "spec": {"template": {"spec": {"containers": [{"image": "i",
                "env": [{"name": "USECODE_USER", "value": "-oops"}]}]}}},
             "status": {}},
            {"metadata": {"labels": {}}, "spec": {}},
        ]});
        let w = |name: &str, state: &str, user: &str, image: &str| Workspace {
            name: name.into(),
            state: state.into(),
            user: user.into(),
            image: image.into(),
        };
        assert_eq!(
            parse_workspaces(&list),
            [
                w("dev", "running", "ada", "base:1"),
                w("odd", "starting", "dev", "i"),
                w("web", "stopped", "dev", "i"),
            ]
        );
    }

    /// Against a running usecode API, when UC_CALLHOME_TEST_API and
    /// UC_CALLHOME_TEST_KEY (an API key there) are set; skipped otherwise.
    /// The page's side asks for a shell in a workspace this machine (no
    /// k3s) hasn't got, and hears so from the daemon, through the API.
    #[test]
    fn against_a_live_api() {
        use crate::callhome::{self, Answer, Facts, Settings};

        let (Ok(endpoint), Ok(key)) = (
            std::env::var("UC_CALLHOME_TEST_API"),
            std::env::var("UC_CALLHOME_TEST_KEY"),
        ) else {
            return;
        };
        let settings = Settings {
            token: callhome::enroll(&endpoint, &key).unwrap(),
            endpoint: endpoint.clone(),
            name: "relay-e2e".into(),
        };
        let facts = Facts {
            name: "relay-e2e".into(),
            machine_id: "relay-e2e".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            arch: std::env::consts::ARCH.into(),
            os: "test".into(),
        };
        let Answer::Ok(state) = callhome::register(&settings, &facts).unwrap() else {
            panic!("a fresh token was turned down");
        };
        let dev = Workspace {
            name: "dev".into(),
            state: "running".into(),
            user: "ada".into(),
            image: "base".into(),
        };
        callhome::heartbeat(&state, &[], Some(("relay-e2e".into(), vec![dev]))).unwrap();

        let stop = Arc::new(AtomicBool::new(false));
        let relay = {
            let (state, stop) = (state.clone(), stop.clone());
            std::thread::spawn(move || serve(&state, &stop, || {}).map(|_| ()))
        };

        let client = callhome::client().unwrap();
        let url = |path: &str| format!("{}{path}", endpoint.trim_end_matches('/'));
        let ticket = (0..50)
            .find_map(|_| {
                let answer = client
                    .post(url(&format!("/workspaces/{}/dev/shell", state.id)))
                    .header("x-api-key", &key)
                    .send()
                    .unwrap();
                if answer.status().is_success() {
                    let body: Value = answer.json().unwrap();
                    return body["ticket"].as_str().map(str::to_string);
                }
                std::thread::sleep(Duration::from_millis(100));
                None
            })
            .expect("the host never connected");

        let page_url = socket_url(&endpoint).replace("/hosts/connect", "/workspaces/shell");
        let (mut page, _) = tungstenite::connect(format!("{page_url}?ticket={ticket}")).unwrap();
        let resize = json!({"type": "resize", "cols": 90, "rows": 20}).to_string();
        page.send(Message::Text(resize.into())).unwrap();
        let said = loop {
            match page.read().unwrap() {
                Message::Text(text) => break serde_json::from_str::<Value>(text.as_str()).unwrap(),
                Message::Close(_) => panic!("closed without a word"),
                _ => {}
            }
        };
        assert_eq!(said["type"], "error", "{said}");
        assert!(
            said["message"]
                .as_str()
                .unwrap()
                .contains("no workspace dev"),
            "{said}"
        );

        stop.store(true, Ordering::SeqCst);
        relay.join().unwrap().unwrap();
        let removed = client
            .delete(url(&format!("/hosts/{}", state.id)))
            .header("x-api-key", &key)
            .send()
            .unwrap();
        assert!(removed.status().is_success());
    }

    #[test]
    fn the_api_is_answered_for_what_cant_open() {
        let (up, _ups) = mpsc::channel();
        let mut sessions = HashMap::new();
        let open = json!({"type": "open", "session": 7, "workspace": "../etc"}).to_string();
        let answer = from_api(&mut sessions, &up, &open).unwrap();
        assert_eq!(answer["type"], "closed");
        assert_eq!(answer["session"], 7);
        assert!(
            answer["reason"]
                .as_str()
                .unwrap()
                .contains("workspace name")
        );
        let input = json!({"type": "input", "session": 7, "data": "x"}).to_string();
        assert!(from_api(&mut sessions, &up, &input).is_none());
    }
}
