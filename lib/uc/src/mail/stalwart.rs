// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Stalwart's management API: JMAP calls on `urn:stalwart:jmap` objects
//! (`x:Domain`, `x:DkimSignature`, ...), which replaced the REST API in
//! 0.16. They go through a `kubectl port-forward` to its HTTP listener, so
//! they work before the mail name resolves or has a certificate.

use crate::kube::cloudflare::{client, send};
use crate::kube::remote::{Forward, kubectl_json, port_forward};
use crate::kube::roles::mail::{ADMIN_SECRET, HTTP_PORT, NAME, NAMESPACE};
use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use serde_json::{Value, json};

const USING: [&str; 2] = ["urn:ietf:params:jmap:core", "urn:stalwart:jmap"];

pub struct Stalwart {
    url: String,
    user: String,
    password: String,
    account: String,
    /// The port-forward; dropping it ends it.
    _forward: Forward,
}

impl Stalwart {
    /// The Stalwart the mail role runs on `context`, as the recovery admin
    /// in mail/mail-admin, unless `UC_MAIL_USER` and `UC_MAIL_PASSWORD`
    /// name another.
    pub async fn admin(context: &str) -> Result<Stalwart> {
        let (user, password) = credentials(context).await?;
        Stalwart::connect(context, NAMESPACE, NAME, HTTP_PORT, &user, &password).await
    }

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
        let forward = port_forward(context, namespace, &format!("svc/{service}"), port).await?;
        let url = forward.url.clone();
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

    /// Deletes object `id` of `kind`.
    pub async fn destroy(&self, kind: &str, id: &str) -> Result<()> {
        let result = self
            .call(&format!("{kind}/set"), json!({"destroy": [id]}))
            .await?;
        if let Some(err) = result["notDestroyed"].get(id) {
            bail!("Stalwart didn't delete {kind} {id}: {err}");
        }
        Ok(())
    }
}

async fn credentials(context: &str) -> Result<(String, String)> {
    if let (Ok(user), Ok(password)) = (
        std::env::var("UC_MAIL_USER"),
        std::env::var("UC_MAIL_PASSWORD"),
    ) {
        return Ok((user, password));
    }
    let secret = kubectl_json(context, &["-n", NAMESPACE, "get", "secret", ADMIN_SECRET])
        .await
        .context(
            "reading the recovery admin; set UC_MAIL_USER and UC_MAIL_PASSWORD for another admin",
        )?;
    let field = |key: &str| -> Result<String> {
        let raw = secret["data"][key].as_str().unwrap_or_default();
        let bytes = base64::engine::general_purpose::STANDARD.decode(raw)?;
        Ok(String::from_utf8(bytes)?)
    };
    Ok((field("username")?, field("password")?))
}
