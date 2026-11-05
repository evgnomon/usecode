// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Calling home, as a daemon module: registering the host with
//! usecode.dev once `uc daemon register` delivered a token, then a
//! heartbeat every few minutes with the daemon's version, the uptime and
//! how every other module did. See [`crate::callhome`].
//!
//! Without a token or a registration it does nothing. A token the API
//! turns down (used, or it ran out) is dropped, and so is the
//! registration of a host that was removed in the console, so a host never
//! keeps knocking on a door that's closed. The API being out of reach is
//! only a note in the journal and another try on the next tick.

use std::fs;
use std::time::Instant;

use crate::callhome::{self, Answer, Facts, Settings, State};
use crate::daemon::{Module, host};
use crate::error::{Context, Result};

#[derive(Default)]
pub struct CallHome {
    /// When the next heartbeat is due; `None` for right away.
    next: Option<Instant>,
}

impl Module for CallHome {
    fn name(&self) -> &'static str {
        "callhome"
    }

    fn reconcile(&mut self, force: bool) -> Result<()> {
        if let Some(settings) = Settings::load(callhome::SETTINGS_PATH)? {
            register(&settings)?;
            self.next = None;
        }

        let Some(state) = State::load(callhome::STATE_PATH)? else {
            if force {
                eprintln!("callhome: off; `uc daemon register` turns it on");
            }
            return Ok(());
        };
        if !force && self.next.is_some_and(|next| Instant::now() < next) {
            return Ok(());
        }

        match callhome::heartbeat(&state, &super::statuses())? {
            Answer::Ok(seconds) => {
                let changed = seconds != state.heartbeat_seconds;
                let state = State {
                    heartbeat_seconds: seconds,
                    ..state
                };
                if force {
                    eprintln!(
                        "callhome: calling {} every {}s",
                        state.endpoint,
                        state.interval().as_secs()
                    );
                }
                self.next = Some(Instant::now() + state.interval());
                if changed {
                    save(&state)?;
                }
            }
            Answer::Refused(detail) => {
                forget(callhome::STATE_PATH)?;
                eprintln!(
                    "callhome: {} doesn't know this host any more ({detail}); stopped calling. \
                     `uc daemon register` registers it again",
                    state.endpoint
                );
            }
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Register with the delivered token and drop it: kept on success, a
/// token is no use again, and one the API turned down never will be.
fn register(settings: &Settings) -> Result<()> {
    let facts = Facts::gather(&settings.name)?;
    match callhome::register(settings, &facts)? {
        Answer::Ok(state) => {
            save(&state)?;
            eprintln!(
                "callhome: registered {} with {} as {}",
                facts.name, state.endpoint, state.id
            );
        }
        Answer::Refused(detail) => eprintln!(
            "callhome: {} turned the enrollment token down ({detail}); \
             `uc daemon register` hands out a new one",
            settings.endpoint
        ),
    }
    forget(callhome::SETTINGS_PATH)
}

fn save(state: &State) -> Result<()> {
    host::ensure_dir(callhome::STATE_DIR, 0o700)?;
    host::put(callhome::STATE_PATH, state.encode()?.as_bytes(), 0o600)?;
    Ok(())
}

fn forget(path: &str) -> Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(e).with_ctx(|| format!("remove {path}"))
        }
        _ => Ok(()),
    }
}
