// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Stalwart's management API: JMAP calls on `urn:stalwart:jmap` objects
//! (`x:Domain`, `x:DkimSignature`, ...), which replaced the REST API in
//! 0.16. They go through a `kubectl port-forward` to its HTTP listener, so
//! they work before the mail name resolves or has a certificate.

use crate::kube::cloudflare::{client, send};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

const USING: [&str; 2] = ["urn:ietf:params:jmap:core", "urn:stalwart:jmap"];

pub struct Stalwart {
    url: String,
    user: String,
    password: String,
    account: String,
    /// The port-forward; dropping it ends it.
    _forward: Child,
}

impl Stalwart {
    /// Forwards a local port to `service`'s `port` in `namespace` and logs
    /// in as `user`.
    pub async fn connect(
        context: &str,
        namespace: &str,
        service: &str,
        port: u16,
        user: &str,
        password: &str,
    ) -> Result<Stalwart> {
        let mut forward = Command::new("kubectl")
            .args(["--context", context, "-n", namespace, "port-forward"])
            .arg(format!("svc/{service}"))
            .arg(format!(":{port}"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("running kubectl port-forward; is kubectl installed?")?;
        let stdout = forward.stdout.take().expect("stdout is piped");
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
        .map_err(|_| anyhow!("kubectl port-forward to svc/{service} did not start in 30s"));
        let local = match local.and_then(|r| r) {
            Ok(local) => local,
            Err(err) => {
                let _ = forward.start_kill();
                let out = forward.wait_with_output().await?;
                bail!("{err:#}: {}", String::from_utf8_lossy(&out.stderr).trim());
            }
        };
        // Keep reading so kubectl never blocks on a full pipe.
        tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });

        let url = format!("http://{local}");
        let session: Value = send("Stalwart's JMAP session", || {
            client()
                .get(format!("{url}/jmap/session"))
                .basic_auth(user, Some(password))
        })
        .await?
        .error_for_status()
        .map_err(|e| anyhow!("logging in to Stalwart as {user}: {e}"))?
        .json()
        .await?;
        let account = session["primaryAccounts"]["urn:stalwart:jmap"]
            .as_str()
            .ok_or_else(|| {
                anyhow!(
                    "{user} can't manage Stalwart: the session has no urn:stalwart:jmap account"
                )
            })?
            .to_string();
        Ok(Stalwart {
            url,
            user: user.to_string(),
            password: password.to_string(),
            account,
            _forward: forward,
        })
    }

    /// Calls `method` with `args` (the account is added) and returns its
    /// response.
    pub async fn call(&self, method: &str, mut args: Value) -> Result<Value> {
        args["accountId"] = json!(self.account);
        let body = json!({"using": USING, "methodCalls": [[method, args, "0"]]});
        let reply: Value = send(&format!("Stalwart {method}"), || {
            client()
                .post(format!("{}/jmap/", self.url))
                .basic_auth(&self.user, Some(&self.password))
                .json(&body)
        })
        .await?
        .error_for_status()
        .map_err(|e| anyhow!("Stalwart {method}: {e}"))?
        .json()
        .await?;
        let [name, result, _] = &reply["methodResponses"][0]
            .as_array()
            .cloned()
            .and_then(|r| <[Value; 3]>::try_from(r).ok())
            .ok_or_else(|| anyhow!("Stalwart {method}: no response in {reply}"))?;
        if name == "error" {
            bail!("Stalwart {method}: {result}");
        }
        Ok(result.clone())
    }

    /// Every object of `kind` (`x:Domain`, ...).
    pub async fn get(&self, kind: &str) -> Result<Vec<Value>> {
        let result = self.call(&format!("{kind}/get"), json!({})).await?;
        Ok(result["list"].as_array().cloned().unwrap_or_default())
    }

    /// The one object of a singleton `kind` (`x:SystemSettings`).
    pub async fn singleton(&self, kind: &str) -> Result<Value> {
        let result = self
            .call(&format!("{kind}/get"), json!({"ids": ["singleton"]}))
            .await?;
        Ok(result["list"][0].clone())
    }

    /// Changes object `id` of `kind` to have `patch`.
    pub async fn update(&self, kind: &str, id: &str, patch: Value) -> Result<()> {
        let result = self
            .call(&format!("{kind}/set"), json!({"update": {id: patch}}))
            .await?;
        if let Some(err) = result["notUpdated"].get(id) {
            bail!("Stalwart didn't change {kind} {id}: {err}");
        }
        Ok(())
    }

    /// Makes a new `kind` and returns its id.
    pub async fn create(&self, kind: &str, object: Value) -> Result<String> {
        let result = self
            .call(&format!("{kind}/set"), json!({"create": {"new": object}}))
            .await?;
        if let Some(err) = result["notCreated"].get("new") {
            bail!("Stalwart didn't make the {kind}: {err}");
        }
        result["created"]["new"]["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("Stalwart {kind}/set: no id in {result}"))
    }
}
