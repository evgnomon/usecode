// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Per-host entries under /etc/ssh/ssh_config.d so VMs are reachable by name.

use std::path::{Path, PathBuf};

use crate::error::Result;

const SSH_CONFIG_D: &str = "/etc/ssh/ssh_config.d";

#[derive(Debug, Clone)]
pub struct SshConfig<'a> {
    pub host: &'a str,
    pub hostname: &'a str,
    pub user: &'a str,
    pub port: u16,
    pub identity_file: &'a str,
}

impl<'a> SshConfig<'a> {
    pub fn new(host: &'a str, hostname: &'a str) -> Self {
        SshConfig {
            host,
            hostname,
            user: "root",
            port: 22,
            identity_file: "~/.ssh/id_ed25519",
        }
    }
}

fn host_config_path(host: &str) -> PathBuf {
    Path::new(SSH_CONFIG_D).join(format!("{host}.conf"))
}

pub fn create_ssh_host_config(conf: &SshConfig<'_>) -> Result<()> {
    let entry = format!(
        "Host {}\n    HostName {}\n    User {}\n    Port {}\n    IdentityFile {}\n",
        conf.host, conf.hostname, conf.user, conf.port, conf.identity_file,
    );

    std::fs::write(host_config_path(conf.host), entry)?;
    Ok(())
}

pub fn remove_ssh_host_config(host: &str) -> Result<()> {
    match std::fs::remove_file(host_config_path(host)) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}
