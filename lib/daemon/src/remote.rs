// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Runs things on another host: a [`Target`] stages files in a private
//! directory there and runs commands against them, as root when they
//! need it. `uc daemon install` uses it to copy usecoded to a host
//! and run `usecoded setup` there; `uc net mesh apply` to hand a
//! member its bundle and run `usecoded join`.
//!
//! A target reached over ssh takes several ssh invocations (probe, copy,
//! run, clean up), but the user should only ever authenticate once. So
//! [`Target::ssh`] first opens a master connection with stdio attached -
//! any key passphrase, password, or 2FA prompt happens there, in the
//! terminal - and every later invocation rides that same connection
//! through ssh's control socket instead of authenticating again.
//!
//! The control node is a host too. [`Target::Local`] runs the same shell
//! commands through `sh -c`, so both kinds go through one code path.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tempfile::TempDir;

use crate::error::{Context, Result};
use crate::inventory::{self, Host, Inventory};

pub enum Target {
    Local,
    Ssh(Session),
}

/// A single authenticated ssh connection to a host, shared by every
/// command issued there.
pub struct Session {
    host: String,
    ctl: PathBuf,
    // Held for its lifetime: dropping it removes the control socket
    // directory once the connection has been torn down.
    _dir: TempDir,
}

/// `name`'s entry in the inventory, if there is an inventory around and
/// it lists the host. An inventory is a convenience, not a requirement:
/// outside a checkout every name is just an ssh destination.
pub fn inventory_host(name: &str) -> Option<Host> {
    inventory::find()
        .ok()
        .and_then(|dir| Inventory::load(&dir).ok())
        .and_then(|inv| inv.host(name).cloned())
}

impl Target {
    /// How to reach `name` the way `uc daemon reload` does: through the
    /// inventory if it is there, as an ssh destination if not.
    pub fn reach(name: &str) -> Result<Target> {
        match inventory_host(name) {
            Some(host) => Target::for_host(&host),
            None => Target::ssh(name),
        }
    }

    /// How to reach an inventory host, the way ansible would: a local
    /// connection for the control node, ssh otherwise.
    pub fn for_host(host: &Host) -> Result<Target> {
        if host.ansible_connection == "local" {
            return Ok(Target::Local);
        }
        let addr = match host.ansible_host.as_str() {
            "" => host.name.as_str(),
            a => a,
        };
        match host.ansible_user.as_str() {
            "" => Target::ssh(addr),
            user => Target::ssh(&format!("{user}@{addr}")),
        }
    }

    /// Open the master connection to `host`, an ssh destination
    /// ([user@]host, or an alias from ~/.ssh/config). It runs in the
    /// foreground until authentication finishes (so prompts reach the
    /// terminal), then puts itself in the background with no remote
    /// command (-N -f) and waits for commands to be multiplexed onto it.
    pub fn ssh(host: &str) -> Result<Target> {
        let dir = tempfile::Builder::new()
            .prefix("uc-net-mesh-ssh-")
            .tempdir()
            .ctx("create control directory")?;
        let ctl = dir.path().join("ctl");

        let status = Command::new("ssh")
            .args([
                "-o",
                "ControlMaster=yes",
                "-o",
                &format!("ControlPath={}", ctl.display()),
                "-o",
                "ControlPersist=60",
                "-N",
                "-f",
                host,
            ])
            .status()
            .with_ctx(|| format!("connect to {host}"))?;
        if !status.success() {
            bail!("connect to {host}: ssh exited with {status}");
        }

        Ok(Target::Ssh(Session {
            host: host.to_string(),
            ctl,
            _dir: dir,
        }))
    }

    /// A command that runs the shell command line `remote` on the
    /// target. `tty` asks for a terminal, which is what lets sudo prompt
    /// for a password on the local terminal.
    fn shell(&self, remote: &str, tty: bool) -> Command {
        match self {
            Target::Local => {
                let mut cmd = Command::new("sh");
                cmd.args(["-c", remote]);
                cmd
            }
            Target::Ssh(s) => {
                let mut cmd = s.ssh(if tty { &["-t"] } else { &[] });
                cmd.arg(remote);
                cmd
            }
        }
    }

    /// Run `remote` and return what it printed, trimmed.
    pub fn output(&self, remote: &str) -> Result<String> {
        let out = self
            .shell(remote, false)
            .stderr(Stdio::inherit())
            .output()
            .with_ctx(|| format!("run {remote:?}"))?;
        if !out.status.success() {
            bail!("{remote:?} exited with {}", out.status);
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Run `argv` on the target and return what it printed, trimmed:
    /// directly if already root, otherwise through non-interactive sudo
    /// (so a prompt is a failure, not a hang). For commands whose output
    /// is the point, e.g. `uc net firewall status`.
    pub fn output_as_root(&self, argv: &[String]) -> Result<String> {
        let cmd = quote_args(argv);
        let remote =
            format!(r#"if [ "$(id -u)" -eq 0 ]; then {cmd}; else exec sudo -n -- {cmd}; fi"#);
        let out = self
            .shell(&remote, false)
            .stderr(Stdio::inherit())
            .output()
            .with_ctx(|| format!("run {:?}", argv.first().map_or("", String::as_str)))?;
        if !out.status.success() {
            bail!(
                "{} exited with {}",
                argv.first().map_or("", String::as_str),
                out.status
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Make a fresh private directory on the target and copy each
    /// `(name, local path)` into it, mode 0700. Returns the directory.
    /// It is created by mktemp rather than at a fixed path because what
    /// is staged is about to be run under sudo: a predictable name under
    /// /tmp could be pre-created or symlinked by another local user.
    pub fn stage(&self, files: &[(&str, &Path)]) -> Result<String> {
        let dir = self
            .output(r#"d=$(mktemp -d "${TMPDIR:-/tmp}/uc-net-mesh.XXXXXXXX") && printf %s "$d""#)?;
        if dir.is_empty() {
            bail!("the target did not report a staging directory");
        }
        for (name, local) in files {
            let staged = format!("{dir}/{name}");
            let mut child = self
                .shell(
                    &format!(
                        "umask 077 && cat > {q} && chmod 0700 {q}",
                        q = shell_quote(&staged)
                    ),
                    false,
                )
                .stdin(Stdio::piped())
                .spawn()?;
            {
                let data = fs::read(local).with_ctx(|| format!("read {}", local.display()))?;
                let mut stdin = child.stdin.take().expect("piped stdin");
                stdin.write_all(&data).with_ctx(|| format!("copy {name}"))?;
            }
            let status = child.wait()?;
            if !status.success() {
                self.remove_all(&dir);
                bail!("copy {name}: exited with {status}");
            }
        }
        Ok(dir)
    }

    /// Run `argv` on the target as root: directly if already root,
    /// otherwise through sudo, with stdio attached for its prompt.
    pub fn run_as_root(&self, argv: &[String]) -> Result<()> {
        let cmd = quote_args(argv);
        let remote =
            format!(r#"if [ "$(id -u)" -eq 0 ]; then exec {cmd}; else exec sudo -- {cmd}; fi"#);
        let status = self.shell(&remote, true).status()?;
        if !status.success() {
            bail!(
                "{} exited with {status}",
                argv.first().map_or("", String::as_str)
            );
        }
        Ok(())
    }

    pub fn remove_all(&self, dir: &str) {
        if dir.is_empty() || dir == "/" {
            return;
        }
        let _ = self
            .shell(&format!("rm -rf {}", shell_quote(dir)), false)
            .status();
    }
}

impl Session {
    /// An ssh invocation that reuses the master connection rather than
    /// opening (and authenticating) a new one. `opts` go before the host;
    /// the caller appends the remote command after it, which is the order
    /// ssh requires.
    fn ssh(&self, opts: &[&str]) -> Command {
        let mut cmd = Command::new("ssh");
        cmd.args([
            "-o",
            "ControlMaster=no",
            "-o",
            &format!("ControlPath={}", self.ctl.display()),
        ]);
        cmd.args(opts);
        cmd.arg(&self.host);
        cmd
    }
}

impl Drop for Session {
    /// Tear down the master connection; the socket directory goes with
    /// the TempDir this holds.
    fn drop(&mut self) {
        let _ = self
            .ssh(&["-O", "exit"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Join args into a single POSIX shell command line, since ssh
/// concatenates a multi-argument command with spaces and hands it to the
/// remote user's shell.
fn quote_args(args: &[String]) -> String {
    args.iter()
        .map(|a| shell_quote(a))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_survives_a_quote() {
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(
            quote_args(&["forward".into(), "a b".into()]),
            "'forward' 'a b'"
        );
    }

    #[test]
    fn a_local_target_stages_and_cleans_up() {
        let src = tempfile::NamedTempFile::new().unwrap();
        fs::write(src.path(), b"hello").unwrap();

        let target = Target::Local;
        let dir = target.stage(&[("greeting", src.path())]).unwrap();
        let staged = Path::new(&dir).join("greeting");
        assert_eq!(fs::read(&staged).unwrap(), b"hello");
        assert_eq!(
            target.output(&format!("cat {dir}/greeting")).unwrap(),
            "hello"
        );

        target.remove_all(&dir);
        assert!(!Path::new(&dir).exists());
    }
}
