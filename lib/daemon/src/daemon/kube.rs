// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Kubernetes, as a daemon module: the host as a k3s server (controller)
//! or agent (worker).
//!
//! The module reads [`crate::kube::SETTINGS_PATH`] (written by `uc kube`)
//! and, while it says the host is a node, renders k3s's config,
//! downloads and installs k3s with its official installer when it isn't
//! there (or isn't the version asked for), and keeps `k3s.service` (or
//! `k3s-agent.service` on a worker) enabled and running - restarting it when its config changes. Turned
//! off, it stops k3s and leaves its data, or uninstalls it entirely when
//! asked to purge.
//!
//! k3s runs as its own unit, not as a child of the daemon: restarting or
//! upgrading the daemon never takes the cluster down, so stopping the
//! daemon leaves k3s alone.

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::SystemTime;

use crate::config::{self, Config};
use crate::daemon::{Module, host};
use crate::error::{Context, Result};
use crate::kube::{self, Settings};

/// Where the installer puts k3s and its helper scripts.
const K3S_BIN: &str = "/usr/local/bin/k3s";
const KILLALL: &str = "/usr/local/bin/k3s-killall.sh";

/// How the installer sets k3s up for one role: its unit and the script
/// that removes it again.
struct Role {
    exec: &'static str,
    unit: &'static str,
    unit_path: &'static str,
    uninstall: &'static str,
}

const SERVER: Role = Role {
    exec: "server",
    unit: "k3s",
    unit_path: "/etc/systemd/system/k3s.service",
    uninstall: "/usr/local/bin/k3s-uninstall.sh",
};

const AGENT: Role = Role {
    exec: "agent",
    unit: "k3s-agent",
    unit_path: "/etc/systemd/system/k3s-agent.service",
    uninstall: "/usr/local/bin/k3s-agent-uninstall.sh",
};

impl Role {
    fn of(settings: &Settings) -> (&'static Role, &'static Role) {
        if settings.agent {
            (&AGENT, &SERVER)
        } else {
            (&SERVER, &AGENT)
        }
    }

    fn installed(&self) -> bool {
        Path::new(self.unit_path).exists()
    }

    fn active(&self) -> bool {
        systemctl_ok(&["is-active", "--quiet", self.unit])
    }

    fn enabled(&self) -> bool {
        systemctl_ok(&["is-enabled", "--quiet", self.unit])
    }
}

/// The official k3s installer.
const INSTALLER_URL: &str = "https://get.k3s.io";

#[derive(Default)]
pub struct Kube {
    /// The settings and mesh config as they were when the module last
    /// converged; `None` while it hasn't, so every tick tries again.
    converged: Option<Stamp>,
}

impl Module for Kube {
    fn name(&self) -> &'static str {
        "kube"
    }

    fn reconcile(&mut self, force: bool) -> Result<()> {
        let stamp = Stamp::now();
        if !force && self.converged.as_ref() == Some(&stamp) {
            return Ok(());
        }
        self.converged = None;

        let settings = Settings::load(kube::SETTINGS_PATH)?;
        if settings.enabled {
            run(&settings)?;
        } else {
            off(&settings, force)?;
        }
        self.converged = Some(stamp);
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        // k3s is its own unit and outlives the daemon on purpose.
        Ok(())
    }
}

/// Make the host the node `settings` describes.
fn run(settings: &Settings) -> Result<()> {
    let (role, other) = Role::of(settings);
    if other.installed() {
        bail!(
            "k3s is set up here as a {}, not a {}; `uc kube disable HOST --purge` first",
            other.exec,
            role.exec
        );
    }
    let cfg = Config::load(config::DEFAULT_PATH)?;
    let body = kube::k3s_config(settings, &cfg)?;
    host::ensure_dir("/etc/rancher/k3s", 0o755)?;
    let mut restart = host::put(kube::K3S_CONFIG_PATH, body.as_bytes(), 0o600)?;

    if let Some(why) = needs_install(settings, role) {
        eprintln!("kube: {why}; installing k3s");
        install(settings, role)?;
        restart = true;
    }

    let action = if restart {
        "restart"
    } else if !role.active() {
        "start"
    } else {
        eprintln!("kube: k3s {} running", role.exec);
        return Ok(());
    };
    host::systemctl(&["enable", "--quiet", role.unit])?;
    // k3s only reports ready once it has its datastore, which for a
    // controller joining others can take a while: don't wait for it here,
    // or the daemon (and whoever asked it to reload) waits with it.
    host::systemctl(&["--no-block", action, role.unit])?;
    eprintln!(
        "kube: k3s {} {}{}",
        role.exec,
        if action == "restart" {
            "(re)starting"
        } else {
            "starting"
        },
        place(settings)
    );
    Ok(())
}

/// How the node fits in its cluster, for the log.
fn place(settings: &Settings) -> String {
    if settings.cluster_init {
        " (first of an HA control plane)".into()
    } else if !settings.server.is_empty() {
        format!(" (joining {})", settings.server)
    } else {
        String::new()
    }
}

/// Stop k3s, and remove it entirely when the settings say purge. Nothing
/// to do on a host that never had it.
fn off(settings: &Settings, force: bool) -> Result<()> {
    // Whichever role k3s was installed as: purging wipes the settings'
    // role along with everything else.
    let Some(role) = [&SERVER, &AGENT].into_iter().find(|r| r.installed()) else {
        if force && Path::new(kube::SETTINGS_PATH).exists() {
            eprintln!("kube: off");
        }
        return Ok(());
    };
    if settings.purge {
        if Path::new(role.uninstall).exists() {
            sh(role.uninstall, &[])?;
        }
        let _ = fs::remove_file(kube::K3S_CONFIG_PATH);
        eprintln!("kube: k3s uninstalled");
        return Ok(());
    }
    if role.active() || role.enabled() {
        host::systemctl(&["disable", "--now", role.unit])?;
        // Stopping the unit leaves the pods' containers running; this is
        // what takes them down too.
        if Path::new(KILLALL).exists() {
            sh(KILLALL, &[])?;
        }
        eprintln!("kube: k3s stopped (its data is kept; `uc kube enable` brings it back)");
    }
    Ok(())
}

/// Why k3s has to be (re)installed, if it does.
fn needs_install(settings: &Settings, role: &Role) -> Option<String> {
    if !Path::new(K3S_BIN).exists() || !role.installed() {
        return Some("k3s is not installed".into());
    }
    if settings.version.is_empty() {
        return None;
    }
    match installed_version() {
        Some(v) if v == settings.version => None,
        Some(v) => Some(format!("k3s {v} is installed, {} wanted", settings.version)),
        None => Some("can't tell which k3s is installed".into()),
    }
}

/// The installed k3s version, from `k3s --version` ("k3s version
/// v1.31.4+k3s1 (a1b2c3d4)").
fn installed_version() -> Option<String> {
    let out = Command::new(K3S_BIN).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace()
        .skip_while(|w| *w != "version")
        .nth(1)
        .map(str::to_string)
}

/// Download the official installer and run it as a server or agent
/// install. It puts the binary, kubectl/crictl links and the unit in
/// place; the config it reads is already written.
fn install(settings: &Settings, role: &Role) -> Result<()> {
    host::ensure_tools(&[("curl", "curl")])?;
    let dir = tempfile::Builder::new()
        .prefix("usecoded-k3s-")
        .tempdir()
        .ctx("create a download directory")?;
    let script = dir.path().join("install.sh");

    let status = Command::new("curl")
        .args(["-sfL", "--retry", "3", "-o"])
        .arg(&script)
        .arg(INSTALLER_URL)
        .status()
        .ctx("run curl")?;
    if !status.success() {
        bail!("download {INSTALLER_URL} failed ({status})");
    }

    let mut cmd = Command::new("sh");
    cmd.arg(&script)
        .env("INSTALL_K3S_EXEC", role.exec)
        .env("INSTALL_K3S_SKIP_START", "true")
        .stdout(Stdio::null());
    if settings.version.is_empty() {
        cmd.env("INSTALL_K3S_CHANNEL", "stable");
    } else {
        cmd.env("INSTALL_K3S_VERSION", &settings.version);
    }
    let status = cmd.status().ctx("run the k3s installer")?;
    if !status.success() {
        bail!("the k3s installer failed ({status})");
    }
    eprintln!(
        "kube: installed k3s {}",
        installed_version().unwrap_or_default()
    );
    Ok(())
}

fn sh(script: &str, args: &[&str]) -> Result<()> {
    let status = Command::new("sh")
        .arg(script)
        .args(args)
        .stdout(Stdio::null())
        .status()
        .with_ctx(|| format!("run {script}"))?;
    if !status.success() {
        bail!("{script} failed ({status})");
    }
    Ok(())
}

fn systemctl_ok(args: &[&str]) -> bool {
    Command::new("systemctl")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// When the files the module depends on last changed, plus whether k3s
/// is running - so a k3s someone stopped by hand is started again on the
/// next tick.
#[derive(PartialEq, Eq)]
struct Stamp([Option<SystemTime>; 2], bool);

impl Stamp {
    fn now() -> Stamp {
        let mtime = |p: &str| fs::metadata(p).and_then(|m| m.modified()).ok();
        Stamp(
            [mtime(kube::SETTINGS_PATH), mtime(config::DEFAULT_PATH)],
            SERVER.active() || AGENT.active(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_module_is_called_kube() {
        assert_eq!(Kube::default().name(), "kube");
    }

    #[test]
    fn a_founder_and_a_joiner_say_so_in_the_log() {
        let founder = Settings {
            cluster_init: true,
            ..Settings::default()
        };
        assert!(place(&founder).contains("first"));
        let joiner = Settings {
            server: "https://10.10.0.2:6443".into(),
            ..Settings::default()
        };
        assert!(place(&joiner).contains("joining https://10.10.0.2:6443"));
        assert_eq!(place(&Settings::default()), "");
    }
}
