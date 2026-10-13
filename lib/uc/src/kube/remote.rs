// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! kubectl for the tools that look after what a role runs (`uc mail`,
//! `uc dns`), outside the configure engine: plain calls against a context,
//! and a `kubectl port-forward` to reach a server's API before, or without,
//! any public name for it.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// kubectl against `context`, returning its stdout.
pub async fn kubectl(context: &str, args: &[&str]) -> Result<String> {
    let out = Command::new("kubectl")
        .args(["--context", context])
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .context("running kubectl; is it installed?")?;
    if !out.status.success() {
        bail!(
            "`kubectl {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// kubectl against `context` with `input` on its stdin, returning its
/// stdout.
pub async fn kubectl_stdin(context: &str, args: &[&str], input: &str) -> Result<String> {
    use tokio::io::AsyncWriteExt;
    let mut child = Command::new("kubectl")
        .args(["--context", context])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("running kubectl; is it installed?")?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    stdin.write_all(input.as_bytes()).await?;
    drop(stdin);
    let out = child.wait_with_output().await?;
    if !out.status.success() {
        bail!(
            "`kubectl {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// kubectl against `context`, returning its stdout as JSON.
pub async fn kubectl_json(context: &str, args: &[&str]) -> Result<Value> {
    let mut args = args.to_vec();
    args.extend(["-o", "json"]);
    Ok(serde_json::from_str(&kubectl(context, &args).await?)?)
}

/// A running `kubectl port-forward`; dropping it ends it.
pub struct Forward {
    /// `http://127.0.0.1:<local port>`.
    pub url: String,
    _child: Child,
}

/// Forwards a free local port to `port` of `target` (`svc/stalwart`,
/// `deployment/pdns-1`, ...) in `namespace`.
pub async fn port_forward(
    context: &str,
    namespace: &str,
    target: &str,
    port: u16,
) -> Result<Forward> {
    let mut child = Command::new("kubectl")
        .args([
            "--context",
            context,
            "-n",
            namespace,
            "port-forward",
            target,
        ])
        .arg(format!(":{port}"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("running kubectl port-forward; is kubectl installed?")?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let mut lines = BufReader::new(stdout).lines();
    // "Forwarding from 127.0.0.1:41235 -> 8080"
    let local = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while let Some(line) = lines.next_line().await? {
            if let Some(addr) = line
                .strip_prefix("Forwarding from ")
                .and_then(|l| l.split_whitespace().next())
                .filter(|a| a.starts_with("127.0.0.1:"))
            {
                return Ok(addr.to_string());
            }
        }
        Err(anyhow!("kubectl port-forward ended"))
    })
    .await
    .map_err(|_| anyhow!("kubectl port-forward to {target} did not start in 30s"));
    let local = match local.and_then(|r| r) {
        Ok(local) => local,
        Err(err) => {
            let _ = child.start_kill();
            let out = child.wait_with_output().await?;
            bail!("{err:#}: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
    };
    // Keep reading so kubectl never blocks on a full pipe.
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    Ok(Forward {
        url: format!("http://{local}"),
        _child: child,
    })
}
