// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// A libvirt call failed; carries the operation and libvirt's own message when available.
    Libvirt(String),
    /// A VM level failure (bad name, already exists, not running, ...).
    Vm(String),
    /// Cloud-init ISO generation failed with the given exit code.
    Iso(i32),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn vm(msg: impl Into<String>) -> Self {
        Error::Vm(msg.into())
    }

    pub fn libvirt(msg: impl Into<String>) -> Self {
        Error::Libvirt(msg.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(err) => write!(f, "{err}"),
            Error::Libvirt(msg) => write!(f, "{msg}"),
            Error::Vm(msg) => write!(f, "{msg}"),
            Error::Iso(code) => write!(f, "cloud-init ISO creation failed: exit code {code}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err)
    }
}
