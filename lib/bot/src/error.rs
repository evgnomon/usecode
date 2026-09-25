// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Crate-level errors for the paths that fail before any tool call: building
//! the HTTP client and reading settings. Failures at the stdio boundary stop
//! the process, so `main` boxes them with anyhow instead.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("could not build the HTTP client: {0}")]
    Http(#[from] reqwest::Error),

    #[error("malformed setting: {0}")]
    Setting(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
