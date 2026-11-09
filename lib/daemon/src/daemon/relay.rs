// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The relay, as a daemon module: on a registered Kubernetes controller,
//! the socket to the usecode API that shells into the cluster's
//! workspaces ride, from the console. See [`crate::relay`].
//!
//! It waits for both: the host registered (calling home) and k3s serving
//! here. The socket lives on a thread of its own, connecting again by
//! itself when it drops; a reconcile only checks it is there for the
//! registration it has, and says how it last went.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::callhome::{self, State};
use crate::daemon::Module;
use crate::error::Result;
use crate::relay::{self, Ended};

/// The wait between tries, doubling from the first to the last.
const RETRY_FIRST: Duration = Duration::from_secs(2);
const RETRY_LAST: Duration = Duration::from_secs(60);
/// The wait after the API turned the host's secret down; calling home
/// drops the registration meanwhile, if it is gone for good.
const REFUSED_WAIT: Duration = Duration::from_secs(300);

#[derive(Default)]
pub struct Relay {
    running: Option<Running>,
}

struct Running {
    state: State,
    stop: Arc<AtomicBool>,
    /// What went wrong last, until the socket connects again.
    trouble: Arc<Mutex<Option<String>>>,
    thread: JoinHandle<()>,
}

impl Running {
    fn start(state: State) -> Running {
        let stop = Arc::new(AtomicBool::new(false));
        let trouble = Arc::new(Mutex::new(None));
        let thread = {
            let (state, stop, trouble) = (state.clone(), stop.clone(), trouble.clone());
            std::thread::spawn(move || keep_connected(&state, &stop, &trouble))
        };
        Running {
            state,
            stop,
            trouble,
            thread,
        }
    }

    /// Ask the thread to stop; it does within a read's wait, or once a
    /// connection that is under way gives up.
    fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Module for Relay {
    fn name(&self) -> &'static str {
        "relay"
    }

    fn reconcile(&mut self, force: bool) -> Result<()> {
        let state = State::load(callhome::STATE_PATH)?;
        let want = state.filter(|_| relay::controller());
        match (want, self.running.take()) {
            (Some(state), Some(running))
                if running.state == state && !running.thread.is_finished() =>
            {
                self.running = Some(running);
            }
            (Some(state), old) => {
                if let Some(old) = old {
                    old.stop();
                }
                eprintln!(
                    "relay: carrying shells into this cluster's workspaces from {}",
                    state.endpoint
                );
                self.running = Some(Running::start(state));
            }
            (None, Some(old)) => {
                old.stop();
                eprintln!("relay: stopped; this host isn't a registered controller any more");
            }
            (None, None) => {
                if force {
                    eprintln!(
                        "relay: off; a Kubernetes controller registered with `uc daemon register` \
                         carries shells into its workspaces from the console"
                    );
                }
            }
        }
        let trouble = self
            .running
            .as_ref()
            .and_then(|r| r.trouble.lock().ok().and_then(|t| t.clone()));
        match trouble {
            Some(trouble) => Err(crate::error::Error(trouble)),
            None => Ok(()),
        }
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(running) = self.running.take() {
            running.stop();
        }
        Ok(())
    }
}

fn keep_connected(state: &State, stop: &AtomicBool, trouble: &Mutex<Option<String>>) {
    let note = |what: Option<String>| {
        if let Ok(mut t) = trouble.lock() {
            *t = what;
        }
    };
    let mut wait = RETRY_FIRST;
    while !stop.load(Ordering::SeqCst) {
        let started = Instant::now();
        let pause = match relay::serve(state, stop, || note(None)) {
            Ok(Ended::Stopped) => return,
            Ok(Ended::Refused(detail)) => {
                let why = format!("{} turned the host down ({detail})", state.endpoint);
                eprintln!("relay: {why}; trying again in {}s", REFUSED_WAIT.as_secs());
                note(Some(why));
                REFUSED_WAIT
            }
            Err(e) => {
                // A connection that held for a while starts the waits over.
                if started.elapsed() > RETRY_LAST {
                    wait = RETRY_FIRST;
                }
                eprintln!("relay: {e}; connecting again in {}s", wait.as_secs());
                note(Some(e.to_string()));
                let this = wait;
                wait = (wait * 2).min(RETRY_LAST);
                this
            }
        };
        let until = Instant::now() + pause;
        while Instant::now() < until && !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}
