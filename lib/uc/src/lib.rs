// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Shared implementation behind the `uc` command family.
//!
//! [`chat`] is `uc` itself, run with no command: the coding agent.
//! [`crypto`] carries the OpenSSL-compatible `aes-256-cbc` container format,
//! [`cli`] the option parsing and terminal helpers shared by `uc-secret-encrypt` and
//! `uc-secret-decrypt`, [`configure`] the parallel machine configurator behind
//! `uc-sync`, [`kube`] the same configurator for a Kubernetes cluster
//! behind `uc-kube-sync`, [`registry`] the tunnelled image transfer behind
//! `uc-image-push` and `uc-image-pull`, and [`password`], [`secret`] and [`repo`] the
//! generator and ansible-vault secret stores behind `uc-secret`, with
//! [`backup`] its SSH, GPG and kubeconfig backup.
//!
//! [`flag`] reads a flag from the command line, a `UC_` variable or the
//! user config.
//!
//! [`group`] and [`groups`] are the command groups (`uc image`,
//! `uc secret cert`, ...) that hand over to the standalone tools, found through [`dispatch`].
//! [`vm`] is `uc vm`, which adds the
//! cloud providers to the local machines of `vm`, and [`mail`] is `uc mail`,
//! which looks after the mail server on a cluster, and [`fs`] is `uc fs`,
//! buckets and objects on the cluster's S3 object storage, and [`logs`] is
//! `uc kube logs`, the container logs of the whole cluster, [`usage`] is
//! `uc kube usage`, its CPU, memory, disk and network, [`openrouter`] is
//! `uc agent usage`, the OpenRouter usage of the coding agent's own key, and
//! [`workspace`] is `uc workspace` (`uc ws`), dev containers on the cluster.

pub mod backup;
pub mod chat;
pub mod cli;
pub mod configure;
pub mod crypto;
pub mod dispatch;
pub mod dns;
pub mod flag;
pub mod forward;
pub mod fs;
pub mod group;
pub mod groups;
pub mod http;
pub mod kube;
pub mod logs;
pub mod mail;
pub mod openrouter;
pub mod password;
pub mod profile;
pub mod registry;
pub mod secret;
pub mod usage;
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
