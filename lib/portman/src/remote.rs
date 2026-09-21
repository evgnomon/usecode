//! Runs portman commands on another host over ssh: it copies the local
//! portman binary to the target and executes it there with the given
//! arguments, so `portman --host NAME up` behaves like running
//! `sudo portman up` after `ssh NAME`.
//!
//! All of that takes several ssh invocations (copy, run, clean up), but
//! the user should only ever authenticate once. So the first thing
//! [`run`] does is open a master connection with stdio attached - any
//! key passphrase, password, or 2FA prompt happens there, in the
//! terminal - and every later invocation rides that same connection
//! through ssh's control socket instead of authenticating again.
//!
//! Nothing dispatches to this yet - the `--host` flag it exists for is
//! not wired into the command table - so it is allowed to sit unused
//! rather than be deleted and rewritten later.

#![allow(dead_code)]

use std::fs;
use std::io::copy;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tempfile::TempDir;

use crate::error::{Context, Result};

/// Copy the currently running portman binary to `host` (an ssh
/// destination, e.g. an entry in ~/.ssh/config) and execute it there
/// with `args`, connecting stdio so interactive prompts (like a sudo
/// password) work as they would locally.
pub fn run(host: &str, args: &[String]) -> Result<()> {
    let mut self_path = std::env::current_exe().ctx("locate portman executable")?;
    if let Ok(resolved) = fs::canonicalize(&self_path) {
        self_path = resolved;
    }

    let session = Session::dial(host)?;

    let binary = session
        .copy_binary(&self_path)
        .with_ctx(|| format!("copy portman to {host}"))?;
    let staging = Path::new(&binary)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();

    let result = session
        .exec(&binary, args)
        .with_ctx(|| format!("run on {host}"));

    session.remove_all(&staging);
    result
}

/// A single authenticated ssh connection to a host, shared by every
/// command [`run`] needs to issue there.
struct Session {
    host: String,
    ctl: PathBuf,
    // Held for its lifetime: dropping it removes the control socket
    // directory once the connection has been torn down.
    _dir: TempDir,
}

impl Session {
    /// Open the master connection. It runs in the foreground until
    /// authentication finishes (so prompts reach the terminal), then
    /// puts itself in the background with no remote command (-N -f) and
    /// waits for commands to be multiplexed onto it.
    fn dial(host: &str) -> Result<Session> {
        let dir = tempfile::Builder::new()
            .prefix("portman-ssh-")
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

        Ok(Session {
            host: host.to_string(),
            ctl,
            _dir: dir,
        })
    }

    /// Build an ssh invocation that reuses the master connection rather
    /// than opening (and authenticating) a new one. `opts` go before the
    /// host and `remote` (the command to run there, if any) after it,
    /// which is the order ssh requires - anything preceding the
    /// destination that is not a recognized option is taken as the
    /// destination itself.
    fn ssh(&self, opts: &[&str], remote: Option<&str>) -> Command {
        let mut cmd = Command::new("ssh");
        cmd.args([
            "-o",
            "ControlMaster=no",
            "-o",
            &format!("ControlPath={}", self.ctl.display()),
        ]);
        cmd.args(opts);
        cmd.arg(&self.host);
        if let Some(remote) = remote {
            cmd.arg(remote);
        }
        cmd
    }

    /// Stream the local binary over ssh's stdin into a fresh private
    /// directory on the host, and return its remote path. The directory
    /// is created by mktemp rather than at a fixed path because the
    /// binary is about to be run under sudo: a predictable name under
    /// /tmp could be pre-created or symlinked by another local user.
    fn copy_binary(&self, local_path: &Path) -> Result<String> {
        let mut f =
            fs::File::open(local_path).with_ctx(|| format!("open {}", local_path.display()))?;

        const STAGE: &str = concat!(
            r#"d=$(mktemp -d "${TMPDIR:-/tmp}/portman.XXXXXXXX") && "#,
            r#"cat > "$d/portman" && chmod 0700 "$d/portman" && printf %s "$d""#
        );

        let mut child = self
            .ssh(&[], Some(STAGE))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        {
            let mut stdin = child.stdin.take().expect("piped stdin");
            copy(&mut f, &mut stdin)?;
        }
        let out = child.wait_with_output()?;
        if !out.status.success() {
            bail!("ssh exited with {}", out.status);
        }

        let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if dir.is_empty() {
            bail!("remote did not report a staging directory");
        }
        Ok(format!("{dir}/portman"))
    }

    fn remove_all(&self, dir: &str) {
        if dir.is_empty() || dir == "/" {
            return;
        }
        let _ = self
            .ssh(&[], Some(&format!("rm -rf {}", shell_quote(dir))))
            .status();
    }

    /// Run the staged binary on the host. Commands that touch WireGuard,
    /// iptables, or /etc need root, so unless the ssh login is already
    /// root they go through sudo; -t allocates a tty, which is what lets
    /// sudo prompt for a password here on the local terminal.
    fn exec(&self, binary: &str, args: &[String]) -> Result<()> {
        let mut remote = format!("{} {}", shell_quote(binary), quote_args(args));
        if needs_root(args) {
            remote = format!(
                r#"if [ "$(id -u)" -eq 0 ]; then exec {remote}; else exec sudo -- {remote}; fi"#
            );
        }

        let status = self.ssh(&["-t"], Some(&remote)).status()?;
        if !status.success() {
            bail!("portman exited with {status}");
        }
        Ok(())
    }
}

impl Drop for Session {
    /// Tear down the master connection; the socket directory goes with
    /// the TempDir this holds.
    fn drop(&mut self) {
        let _ = self
            .ssh(&["-O", "exit"], None)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Whether a portman subcommand has to run as root on the target.
/// Nearly everything does: even the read-only commands parse the config,
/// which lives root-owned and mode 0600. Only the ones that touch no
/// state at all are exempt.
fn needs_root(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        None => false,
        Some("version" | "-v" | "--version" | "help" | "-h" | "--help") => false,
        Some(_) => true,
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
    fn only_the_stateless_commands_skip_sudo() {
        assert!(!needs_root(&["version".to_string()]));
        assert!(!needs_root(&[]));
        assert!(needs_root(&["status".to_string()]));
    }
}
