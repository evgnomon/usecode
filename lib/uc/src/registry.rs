// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Push and pull container images through the registry deployed by
//! `deploy/playbooks/registry.yaml` (`lib/roles/container-registry`).
//!
//! The registry host is a "protected-servers" node and is only reachable
//! through the bastion, so a [`Session`] opens a local SSH tunnel to the
//! registry's port and logs the container CLI in before any image moves.

use anyhow::{Context, Result, bail};
use clap::Args;
use std::io::Write;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::Duration;
use zeroize::Zeroizing;

/// The vault key holding the registry password.
const PASSWORD_KEY: &str = "vault_container_registry_password";

/// How many one second attempts the tunnel gets to start listening.
const TUNNEL_ATTEMPTS: u32 = 30;

/// Where the registry is and how to reach it; shared by `uc push` and
/// `uc pull`.
#[derive(Args, Debug)]
pub struct Config {
    /// Container CLI used to log in, tag, push and pull.
    #[arg(long, env = "CONTAINER_CLI", default_value = "podman")]
    pub container_cli: String,

    /// SSH host (or ssh_config alias) of the bastion to jump through.
    #[arg(long, env = "BASTION_HOST", default_value = "shadow")]
    pub bastion_host: String,

    /// Address of the registry host behind the bastion.
    #[arg(long, env = "REGISTRY_HOST", default_value = "167.233.79.126")]
    pub registry_host: String,

    /// SSH port of the registry host.
    #[arg(long, env = "REGISTRY_SSH_PORT", default_value_t = 2657)]
    pub registry_ssh_port: u16,

    /// SSH user on the registry host.
    #[arg(long, env = "REGISTRY_SSH_USER", default_value = "root")]
    pub registry_ssh_user: String,

    /// Port the registry listens on, on the registry host.
    #[arg(long, env = "REGISTRY_PORT", default_value_t = 5000)]
    pub registry_port: u16,

    /// Local end of the tunnel [default: the registry port].
    #[arg(long, env = "LOCAL_PORT")]
    pub local_port: Option<u16>,

    /// Registry user to log in as.
    #[arg(long, env = "REGISTRY_USERNAME", default_value = "registry")]
    pub registry_username: String,

    /// Registry password [default: read from the vault file].
    #[arg(long, env = "REGISTRY_PASSWORD", hide_env_values = true)]
    pub registry_password: Option<String>,

    /// Ansible vault holding the registry password
    /// [default: deploy/playbooks/vault.yaml in this git checkout].
    #[arg(long, env = "REGISTRY_VAULT_FILE", value_name = "FILE")]
    pub vault_file: Option<PathBuf>,
}

impl Config {
    fn local_port(&self) -> u16 {
        self.local_port.unwrap_or(self.registry_port)
    }

    /// The registry as the container CLI sees it: the local tunnel end.
    pub fn addr(&self) -> String {
        format!("localhost:{}", self.local_port())
    }

    /// The registry is plain HTTP (no TLS configured by the container-registry
    /// role), so podman needs `--tls-verify=false` to talk to it over the
    /// tunnel.
    fn tls_args(&self) -> &'static [&'static str] {
        match self.container_cli.as_str() {
            "podman" => &["--tls-verify=false"],
            _ => &[],
        }
    }

    fn vault_file(&self) -> Result<PathBuf> {
        if let Some(file) = &self.vault_file {
            return Ok(file.clone());
        }
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .stderr(Stdio::null())
            .output()
            .context("running git to find the checkout")?;
        if !output.status.success() {
            bail!("not inside a git checkout; set --vault-file or REGISTRY_PASSWORD");
        }
        let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(PathBuf::from(root).join("deploy/playbooks/vault.yaml"))
    }

    fn password(&self) -> Result<Zeroizing<String>> {
        if let Some(password) = self.registry_password.as_deref().filter(|p| !p.is_empty()) {
            return Ok(Zeroizing::new(password.to_string()));
        }
        let file = self.vault_file()?;
        let output = Command::new("ansible-vault")
            .arg("view")
            .arg(&file)
            .stderr(Stdio::inherit())
            .output()
            .context("running ansible-vault")?;
        if !output.status.success() {
            bail!("ansible-vault could not open {}", file.display());
        }
        let plain = Zeroizing::new(String::from_utf8_lossy(&output.stdout).into_owned());
        parse_password(&plain).with_context(|| {
            format!(
                "could not determine registry password; set REGISTRY_PASSWORD or check {}",
                file.display()
            )
        })
    }
}

/// Picks the registry password out of the decrypted vault.
fn parse_password(vault: &str) -> Result<Zeroizing<String>> {
    let doc: serde_yaml::Value = serde_yaml::from_str(vault).context("parsing the vault")?;
    match doc.get(PASSWORD_KEY) {
        Some(serde_yaml::Value::String(p)) if !p.is_empty() => Ok(Zeroizing::new(p.clone())),
        Some(_) => bail!("{PASSWORD_KEY} is empty or not a string"),
        None => bail!("{PASSWORD_KEY} not found"),
    }
}

/// An open tunnel with the container CLI logged in to the registry at its
/// end. Dropping it closes the tunnel.
pub struct Session<'a> {
    config: &'a Config,
    tunnel: Child,
}

impl<'a> Session<'a> {
    pub fn open(config: &'a Config) -> Result<Self> {
        let password = config.password()?;
        let port = config.local_port();
        eprintln!(
            "Opening SSH tunnel to {}:{} via bastion {}...",
            config.registry_host, config.registry_port, config.bastion_host
        );
        let tunnel = Command::new("ssh")
            .args(["-4", "-N", "-o", "ExitOnForwardFailure=yes", "-J"])
            .arg(&config.bastion_host)
            .arg("-p")
            .arg(config.registry_ssh_port.to_string())
            .arg("-L")
            .arg(format!(
                "127.0.0.1:{port}:localhost:{}",
                config.registry_port
            ))
            .arg(format!(
                "{}@{}",
                config.registry_ssh_user, config.registry_host
            ))
            .stdin(Stdio::null())
            .spawn()
            .context("starting ssh")?;
        let mut session = Session { config, tunnel };
        session.wait_for_tunnel(port)?;
        session.login(&password)?;
        Ok(session)
    }

    fn wait_for_tunnel(&mut self, port: u16) -> Result<()> {
        for _ in 0..TUNNEL_ATTEMPTS {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Ok(());
            }
            if self.tunnel.try_wait()?.is_some() {
                bail!("SSH tunnel exited unexpectedly");
            }
            sleep(Duration::from_secs(1));
        }
        bail!("SSH tunnel did not open 127.0.0.1:{port} within {TUNNEL_ATTEMPTS}s")
    }

    fn login(&self, password: &str) -> Result<()> {
        let mut child = self
            .cli()
            .arg("login")
            .args(self.config.tls_args())
            .arg(self.config.addr())
            .args(["-u", &self.config.registry_username, "--password-stdin"])
            .stdin(Stdio::piped())
            .spawn()
            .with_context(|| format!("starting {}", self.config.container_cli))?;
        child
            .stdin
            .take()
            .expect("stdin is piped")
            .write_all(format!("{password}\n").as_bytes())
            .context("sending the password to login")?;
        check(child.wait()?, "login")
    }

    fn cli(&self) -> Command {
        Command::new(&self.config.container_cli)
    }

    fn run(&self, what: &str, args: &[&str]) -> Result<()> {
        let status = self
            .cli()
            .args(args)
            .status()
            .with_context(|| format!("starting {}", self.config.container_cli))?;
        check(status, what)
    }

    /// Tags each local image under the registry and pushes it.
    pub fn push(&self, images: &[String]) -> Result<()> {
        let tls = self.config.tls_args();
        for image in images {
            let target = format!("{}/{image}", self.config.addr());
            eprintln!("Tagging {image} -> {target}");
            self.run("tag", &["tag", image, &target])?;
            eprintln!("Pushing {target}");
            self.run("push", &[&["push"], tls, &[&target]].concat())?;
        }
        Ok(())
    }

    /// Pulls each image from the registry, also tagging it without the
    /// registry prefix when `strip_host` is set.
    pub fn pull(&self, images: &[String], strip_host: bool) -> Result<()> {
        let tls = self.config.tls_args();
        for image in images {
            let source = format!("{}/{image}", self.config.addr());
            eprintln!("Pulling {source}");
            self.run("pull", &[&["pull"], tls, &[&source]].concat())?;
            if strip_host {
                eprintln!("Tagging {source} -> {image}");
                self.run("tag", &["tag", &source, image])?;
            }
        }
        Ok(())
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        let _ = self.tunnel.kill();
        let _ = self.tunnel.wait();
    }
}

fn check(status: std::process::ExitStatus, what: &str) -> Result<()> {
    if !status.success() {
        bail!("{what} failed ({status})");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_quoted_and_bare_passwords() {
        let quoted = "a: 1\nvault_container_registry_password: \"s3cr:et\"\n";
        assert_eq!(parse_password(quoted).unwrap().as_str(), "s3cr:et");
        let bare = "vault_container_registry_password: hunter2\n";
        assert_eq!(parse_password(bare).unwrap().as_str(), "hunter2");
    }

    #[test]
    fn rejects_a_missing_or_empty_password() {
        assert!(parse_password("other: x\n").is_err());
        assert!(parse_password("vault_container_registry_password: \"\"\n").is_err());
    }
}
