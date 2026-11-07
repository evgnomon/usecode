// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Shells into workspaces, relayed: the browser can't reach a cluster, but
//! the usecode daemon on its controller can reach us.
//!
//! The daemon keeps one WebSocket open to an API instance
//! (`GET /hosts/connect`, with the host's secret) and every shell into a
//! workspace of that cluster rides it. On it, the API sends text frames
//!
//! - `{"type": "open", "session": 7, "workspace": "dev", "cols": 120, "rows": 32}`
//! - `{"type": "input", "session": 7, "data": "ls\r"}`
//! - `{"type": "resize", "session": 7, "cols": 100, "rows": 30}`
//! - `{"type": "close", "session": 7}`
//!
//! and the daemon answers with binary frames, the session's number in four
//! bytes (big endian) and then what the terminal printed, and text frames
//!
//! - `{"type": "closed", "session": 7, "reason": "..."}`: the shell ended;
//! - `{"type": "workspaces", "cluster_id": "...", "workspaces": [...]}`.
//!
//! The browser's side is restty's own PTY protocol, as `uc agent serve`
//! speaks it: output in binary frames, keys and sizes as JSON text.
//!
//! A shell is paid for with a [`Ticket`]: the console asks for one with the
//! session cookie (`POST /workspaces/{host}/{name}/shell`) and opens the
//! socket with it. It names the user, the host and the workspace, is good
//! for a minute and only once, and is signed with the API's secret key, so
//! any instance can check it. The instance the browser lands on hands the
//! socket to the one the host's relay socket is on, if that's another.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::config::Settings;

/// How long a ticket is good for.
pub const TICKET_SECONDS: i64 = 60;

/// The most shells one host carries at once.
pub const MAX_SESSIONS: usize = 16;

/// What a shell's browser side is told by its host.
pub enum Down {
    /// What the terminal printed.
    Output(Bytes),
    /// The shell exited, with its exit status.
    Exited(i32),
    /// The shell ended for another reason: why.
    Closed(String),
}

/// One host's relay socket, as this instance holds it.
pub struct HostLink {
    /// Which socket this is, so a newer one isn't torn down by an old one.
    pub generation: u64,
    pub user_id: Uuid,
    /// Text frames for the daemon.
    pub tx: mpsc::UnboundedSender<String>,
    /// The shells riding it, by session number.
    pub sessions: Mutex<HashMap<u32, mpsc::Sender<Down>>>,
}

impl HostLink {
    /// Tell the daemon `message`; false when the socket is gone.
    pub fn send(&self, message: serde_json::Value) -> bool {
        self.tx.send(message.to_string()).is_ok()
    }

    /// Take shell `session` on, unless the host carries [`MAX_SESSIONS`].
    pub fn add_session(&self, session: u32, tx: mpsc::Sender<Down>) -> bool {
        let mut sessions = lock(&self.sessions);
        if sessions.len() >= MAX_SESSIONS {
            return false;
        }
        sessions.insert(session, tx);
        true
    }

    /// Where shell `session`'s output goes, while it is open.
    pub fn session(&self, session: u32) -> Option<mpsc::Sender<Down>> {
        lock(&self.sessions).get(&session).cloned()
    }

    /// Let go of shell `session`; where its output went, if it was open.
    pub fn end_session(&self, session: u32) -> Option<mpsc::Sender<Down>> {
        lock(&self.sessions).remove(&session)
    }
}

/// The relay sockets and shells on this instance.
pub struct Hub {
    /// Where the other instances reach this one.
    pub internal_url: String,
    hosts: Mutex<HashMap<Uuid, Arc<HostLink>>>,
    /// The tickets used, with when they run out, so each is good once.
    used: Mutex<HashMap<String, i64>>,
    generation: AtomicU64,
    session: AtomicU32,
}

impl Hub {
    pub fn new(internal_url: String) -> Hub {
        Hub {
            internal_url,
            hosts: Mutex::default(),
            used: Mutex::default(),
            generation: AtomicU64::new(1),
            session: AtomicU32::new(1),
        }
    }

    /// Take a new relay socket for `host_id`, in place of any older one.
    pub fn attach(
        &self,
        host_id: Uuid,
        user_id: Uuid,
        tx: mpsc::UnboundedSender<String>,
    ) -> Arc<HostLink> {
        let link = Arc::new(HostLink {
            generation: self.generation.fetch_add(1, Ordering::Relaxed),
            user_id,
            tx,
            sessions: Mutex::default(),
        });
        let old = lock(&self.hosts).insert(host_id, link.clone());
        if let Some(old) = old {
            close_all(&old, "The host connected again; open the shell again.");
        }
        link
    }

    /// Let go of `link`, and end its shells; whether it was the host's
    /// current socket here.
    pub fn detach(&self, host_id: &Uuid, link: &HostLink) -> bool {
        let mut hosts = lock(&self.hosts);
        let current = hosts
            .get(host_id)
            .is_some_and(|l| l.generation == link.generation);
        if current {
            hosts.remove(host_id);
        }
        drop(hosts);
        close_all(link, "The host went away.");
        current
    }

    pub fn link(&self, host_id: &Uuid) -> Option<Arc<HostLink>> {
        lock(&self.hosts).get(host_id).cloned()
    }

    pub fn next_session(&self) -> u32 {
        self.session.fetch_add(1, Ordering::Relaxed)
    }

    /// Use `ticket` up; false when it was used before.
    pub fn use_ticket(&self, ticket: &Ticket) -> bool {
        let now = Utc::now().timestamp();
        let mut used = lock(&self.used);
        used.retain(|_, expires| *expires > now);
        used.insert(ticket.nonce.clone(), ticket.expires_at)
            .is_none()
    }
}

fn close_all(link: &HostLink, why: &str) {
    for (_, tx) in lock(&link.sessions).drain() {
        let _ = tx.try_send(Down::Closed(why.into()));
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// What a ticket is good for.
#[derive(Debug, Clone, PartialEq)]
pub struct Ticket {
    pub user_id: Uuid,
    pub host_id: Uuid,
    pub workspace: String,
    pub expires_at: i64,
    pub nonce: String,
}

fn mac(secret_key: &str) -> Hmac<Sha256> {
    Hmac::<Sha256>::new_from_slice(format!("usecode shell ticket:{secret_key}").as_bytes())
        .expect("HMAC takes a key of any size")
}

impl Ticket {
    pub fn new(user_id: Uuid, host_id: Uuid, workspace: &str) -> Ticket {
        let mut nonce = [0u8; 16];
        rand::rng().fill_bytes(&mut nonce);
        Ticket {
            user_id,
            host_id,
            workspace: workspace.to_string(),
            expires_at: Utc::now().timestamp() + TICKET_SECONDS,
            nonce: URL_SAFE_NO_PAD.encode(nonce),
        }
    }

    /// `user.host.workspace.expires.nonce.signature`: no field has a dot
    /// in it (a workspace name is lowercase letters, digits and dashes).
    pub fn encode(&self, secret_key: &str) -> String {
        let body = format!(
            "{}.{}.{}.{}.{}",
            self.user_id, self.host_id, self.workspace, self.expires_at, self.nonce
        );
        let mut mac = mac(secret_key);
        mac.update(body.as_bytes());
        let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        format!("{body}.{sig}")
    }

    /// The ticket `raw` is, if it is signed with `secret_key` and still good.
    pub fn decode(raw: &str, secret_key: &str) -> Option<Ticket> {
        let (body, sig) = raw.rsplit_once('.')?;
        let sig = URL_SAFE_NO_PAD.decode(sig).ok()?;
        let mut mac = mac(secret_key);
        mac.update(body.as_bytes());
        let want = mac.finalize().into_bytes();
        if !bool::from(want.as_slice().ct_eq(&sig)) {
            return None;
        }
        let parts: Vec<&str> = body.split('.').collect();
        let [user, host, workspace, expires, nonce] = parts.as_slice() else {
            return None;
        };
        let ticket = Ticket {
            user_id: Uuid::parse_str(user).ok()?,
            host_id: Uuid::parse_str(host).ok()?,
            workspace: workspace.to_string(),
            expires_at: expires.parse().ok()?,
            nonce: nonce.to_string(),
        };
        (ticket.expires_at > Utc::now().timestamp()).then_some(ticket)
    }
}

/// Where the other instances reach this one: `USECODE_AGENT_INTERNAL_URL`,
/// else this pod's address (what its own hostname resolves to, as
/// Kubernetes writes it in /etc/hosts) with `bind`'s port.
pub async fn internal_url(settings: &Settings) -> String {
    if let Some(url) = &settings.internal_url {
        return url.clone();
    }
    let port = settings
        .bind
        .rsplit_once(':')
        .and_then(|(_, p)| p.parse::<u16>().ok())
        .unwrap_or(8000);
    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty());
    if let Some(hostname) = hostname
        && let Ok(addrs) = tokio::net::lookup_host((hostname.as_str(), port)).await
    {
        let addrs: Vec<_> = addrs.collect();
        let pick = addrs
            .iter()
            .find(|a| a.is_ipv4() && !a.ip().is_loopback())
            .or_else(|| addrs.iter().find(|a| !a.ip().is_loopback()));
        if let Some(addr) = pick {
            return format!("http://{addr}");
        }
    }
    format!("http://127.0.0.1:{port}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ticket_is_good_with_its_signature_only() {
        let t = Ticket::new(Uuid::new_v4(), Uuid::new_v4(), "dev");
        let raw = t.encode("key");
        assert_eq!(Ticket::decode(&raw, "key"), Some(t.clone()));
        assert_eq!(Ticket::decode(&raw, "other key"), None);
        let forged = raw.replacen(".dev.", ".prod.", 1);
        assert_eq!(Ticket::decode(&forged, "key"), None);
        assert_eq!(Ticket::decode("nonsense", "key"), None);
    }

    #[test]
    fn a_ticket_runs_out_and_is_good_once() {
        let mut t = Ticket::new(Uuid::new_v4(), Uuid::new_v4(), "dev");
        t.expires_at = Utc::now().timestamp() - 1;
        assert_eq!(Ticket::decode(&t.encode("key"), "key"), None);

        let hub = Hub::new("http://127.0.0.1:8000".into());
        let t = Ticket::new(Uuid::new_v4(), Uuid::new_v4(), "dev");
        assert!(hub.use_ticket(&t));
        assert!(!hub.use_ticket(&t));
    }

    #[test]
    fn a_new_socket_takes_over_and_the_old_one_lets_go_quietly() {
        let hub = Hub::new("http://127.0.0.1:8000".into());
        let (host, user) = (Uuid::new_v4(), Uuid::new_v4());
        let (tx, _rx) = mpsc::unbounded_channel();
        let old = hub.attach(host, user, tx.clone());
        let new = hub.attach(host, user, tx);
        assert!(!hub.detach(&host, &old));
        assert_eq!(hub.link(&host).unwrap().generation, new.generation);
        assert!(hub.detach(&host, &new));
        assert!(hub.link(&host).is_none());
    }
}
