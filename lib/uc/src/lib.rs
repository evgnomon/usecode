// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Shared implementation behind the `uc` command family.
//!
//! [`crypto`] carries the OpenSSL-compatible `aes-256-cbc` container format,
//! [`cli`] the option parsing and terminal helpers shared by `uc-secret-encrypt` and
//! `uc-secret-decrypt`, [`configure`] the parallel machine configurator behind
//! `uc-configure`, [`kube`] the same configurator for a Kubernetes cluster
//! behind `uc-kube-configure`, [`registry`] the tunnelled image transfer behind
//! `uc-push` and `uc-pull`, and [`password`], [`secret`] and [`repo`] the
//! generator and ansible-vault secret stores behind `uc-secret`.
//!
//! [`group`] and [`groups`] are the command groups (`uc image`, `uc repo`,
//! ...) that hand over to the standalone tools, found through [`dispatch`],
//! and [`authors`] is `uc repo authors`. [`vm`] is `uc vm`, which adds the
//! cloud providers to the local machines of `vm`, and [`mail`] is `uc mail`,
//! which looks after the mail server on a cluster, and [`fs`] is `uc fs`,
//! buckets and objects on the cluster's S3 object storage, and [`logs`] is
//! `uc kube logs`, the container logs of the whole cluster, and
//! [`workspace`] is `uc workspace` (`uc ws`), dev containers on the cluster.

pub mod authors;
pub mod cli;
pub mod configure;
pub mod crypto;
pub mod dispatch;
pub mod dns;
pub mod forward;
pub mod fs;
pub mod group;
pub mod groups;
pub mod http;
pub mod kube;
pub mod logs;
pub mod mail;
pub mod password;
pub mod registry;
pub mod repo;
pub mod secret;
pub mod vm;
pub mod workspace;

use std::fmt;

/// Every failure the `uc` binaries report to the user.
#[derive(Debug)]
pub enum Error {
    /// Bad invocation; the caller prints its own usage text.
    Usage(String),
    /// Anything that went wrong while touching the filesystem.
    Io(String, std::io::Error),
    /// A file was not shaped the way the container format requires.
    Format(String),
    /// The password did not open the file.
    Decrypt,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage(msg) => write!(f, "{msg}"),
            Error::Io(what, err) => write!(f, "{what}: {err}"),
            Error::Format(msg) => write!(f, "{msg}"),
            Error::Decrypt => write!(
                f,
                "decryption failed (wrong password, or file not produced by uc-secret-encrypt)"
            ),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
