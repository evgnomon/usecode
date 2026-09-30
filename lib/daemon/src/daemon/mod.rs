// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `usecoded run`: the usecode daemon, the one long-running service on
//! every host (usecode.service).
//!
//! What it does is the sum of its [`Module`]s. Each module owns one
//! thing the host can do - today the WireGuard mesh - and knows both
//! what that thing needs and how to get the host there. The daemon only
//! drives them: it reconciles every module when it starts, again on
//! SIGHUP (`systemctl reload usecode`) or a `reload` on its control
//! socket ([`control`], `usecoded reload`, which also says how each
//! module did), and again every [`TICK`] so a module that was waiting on
//! something picks it up by itself. SIGTERM stops every module and exits.
//!
//! A module never takes the daemon down. What it can't do yet is a note
//! in the journal and another try on the next tick; a real error is
//! logged the same way. Adding a feature is adding a module here, not a
//! new service or a new binary.

pub mod control;
pub mod host;
pub mod mesh;

use std::os::unix::net::UnixStream;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use crate::error::Result;

/// How often every module is given another chance to converge.
const TICK: Duration = Duration::from_secs(60);

/// One thing the daemon does on a host.
pub trait Module {
    /// Short name, used to prefix everything the module logs.
    fn name(&self) -> &'static str;

    /// Bring the host towards what this module wants. `force` is set on
    /// start and on reload; otherwise a module may skip the work when
    /// nothing it depends on has changed since it last converged.
    fn reconcile(&mut self, force: bool) -> Result<()>;

    /// Undo what the module set up, when the daemon stops.
    fn stop(&mut self) -> Result<()>;
}

/// Every module, in the order they are reconciled.
fn modules() -> Vec<Box<dyn Module>> {
    vec![Box::new(mesh::Mesh::default())]
}

/// What wakes the loop up before the next tick.
pub enum Event {
    /// Converge every module now; answer the control-socket client, if
    /// the reload came from one, with how it went.
    Reload(Option<UnixStream>),
    Stop,
}

pub fn run() -> Result<()> {
    // Blocked before any thread starts, so every thread inherits it.
    let signals = Signals::block()?;
    let (events, rx) = mpsc::channel();
    signals.forward(events.clone());
    if let Err(e) = control::listen(control::SOCKET_PATH, events) {
        eprintln!("control: {e}; reload with SIGHUP only");
    }

    let mut modules = modules();
    eprintln!(
        "usecoded {} running: {}",
        env!("CARGO_PKG_VERSION"),
        modules
            .iter()
            .map(|m| m.name())
            .collect::<Vec<_>>()
            .join(", ")
    );

    let mut force = true;
    let mut clients = Vec::new();
    loop {
        let report = reconcile(&mut modules, force);
        for client in clients.drain(..) {
            control::reply(client, &report);
        }
        match rx.recv_timeout(TICK) {
            Ok(Event::Reload(client)) => {
                clients.extend(client);
                force = true;
            }
            Ok(Event::Stop) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => force = false,
        }
    }

    eprintln!("stopping");
    for m in modules.iter_mut().rev() {
        if let Err(e) = m.stop() {
            eprintln!("{}: stop: {e}", m.name());
        }
    }
    let _ = std::fs::remove_file(control::SOCKET_PATH);
    Ok(())
}

/// Give every module a turn, logging what went wrong. Returns one line
/// per module for a control-socket client.
fn reconcile(modules: &mut [Box<dyn Module>], force: bool) -> String {
    let mut report = String::new();
    for m in modules.iter_mut() {
        match m.reconcile(force) {
            Ok(()) => report.push_str(&format!("{}: ok\n", m.name())),
            Err(e) => {
                eprintln!("{}: {e}; trying again in {}s", m.name(), TICK.as_secs());
                report.push_str(&format!("{}: error: {e}\n", m.name()));
            }
        }
    }
    report
}

/// SIGHUP, SIGTERM and SIGINT, blocked so they queue up for
/// [`Signals::wait`] instead of killing the process. Child processes
/// start with an empty mask (std resets it), so apt-get and friends are
/// still interruptible.
struct Signals(libc::sigset_t);

impl Signals {
    fn block() -> Result<Signals> {
        // SAFETY: plain libc signal-set calls on a zeroed local set.
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            for sig in [libc::SIGHUP, libc::SIGTERM, libc::SIGINT] {
                libc::sigaddset(&mut set, sig);
            }
            if libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) != 0 {
                bail!("block signals: {}", std::io::Error::last_os_error());
            }
            Ok(Signals(set))
        }
    }

    /// The next signal to arrive within `timeout`, if any.
    fn wait(&self, timeout: Duration) -> Option<i32> {
        let ts = libc::timespec {
            tv_sec: timeout.as_secs() as _,
            tv_nsec: 0,
        };
        // SAFETY: the set was initialised in block(); siginfo may be null.
        let sig = unsafe { libc::sigtimedwait(&self.0, std::ptr::null_mut(), &ts) };
        (sig > 0).then_some(sig)
    }

    /// Turn signals into [`Event`]s from a thread of their own: SIGHUP
    /// is a reload, anything else a stop.
    fn forward(self, events: Sender<Event>) {
        std::thread::spawn(move || {
            loop {
                let event = match self.wait(TICK) {
                    None => continue,
                    Some(libc::SIGHUP) => {
                        eprintln!("reloading");
                        Event::Reload(None)
                    }
                    Some(_) => Event::Stop,
                };
                if events.send(event).is_err() {
                    return;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blocked_signal_is_waited_for_not_delivered() {
        let signals = Signals::block().unwrap();
        assert_eq!(signals.wait(Duration::from_secs(0)), None);
        // SAFETY: SIGHUP is blocked on this thread, so it only queues.
        unsafe { libc::raise(libc::SIGHUP) };
        assert_eq!(signals.wait(Duration::from_secs(1)), Some(libc::SIGHUP));
    }
}
