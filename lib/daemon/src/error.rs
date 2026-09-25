// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! A single error type for the whole program. uc daemon's errors are read
//! by people, not matched on by callers, so an error is just a message
//! with whatever context the layer that produced it could add.

use std::fmt;

#[derive(Debug)]
pub struct Error(pub String);

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

/// Build an [`Error`] the way `fmt::format` builds a string.
#[macro_export]
macro_rules! err {
    ($($arg:tt)*) => { $crate::error::Error(format!($($arg)*)) };
}

/// Return early with a formatted [`Error`].
#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => { return Err($crate::err!($($arg)*)) };
}

/// Context wraps a failure in the sentence that explains what uc daemon
/// was doing, the way Go's `fmt.Errorf("...: %w", err)` chains read.
pub trait Context<T> {
    fn ctx(self, msg: impl fmt::Display) -> Result<T>;
    fn with_ctx(self, f: impl FnOnce() -> String) -> Result<T>;
}

impl<T, E: fmt::Display> Context<T> for std::result::Result<T, E> {
    fn ctx(self, msg: impl fmt::Display) -> Result<T> {
        self.map_err(|e| Error(format!("{msg}: {e}")))
    }

    fn with_ctx(self, f: impl FnOnce() -> String) -> Result<T> {
        self.map_err(|e| Error(format!("{}: {e}", f())))
    }
}
