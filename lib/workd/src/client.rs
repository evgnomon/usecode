// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! HTTPS client for the API with mutual TLS.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use ureq::Agent;
use ureq::tls::{Certificate, ClientCert, PemItem, PrivateKey, RootCerts, TlsConfig};

pub struct ApiClient {
    base_url: String,
    agent: Agent,
}

/// Error from a request; `Status` carries the HTTP status code.
#[derive(Debug)]
pub enum ClientError {
    Status(u16, String),
    Other(anyhow::Error),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Status(code, url) => write!(f, "HTTP {code} for url: {url}"),
            ClientError::Other(e) => write!(f, "{e:#}"),
        }
    }
}

impl std::error::Error for ClientError {}

fn read_certs(path: &Path) -> Result<Vec<Certificate<'static>>> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut certs = Vec::new();
    for item in ureq::tls::parse_pem(&bytes) {
        if let PemItem::Certificate(c) =
            item.with_context(|| format!("parsing {}", path.display()))?
        {
            certs.push(c);
        }
    }
    Ok(certs)
}

impl ApiClient {
    pub fn new(
        base_url: &str,
        client_cert: &Path,
        client_key: &Path,
        ca_cert: &Path,
    ) -> Result<Self> {
        let key_bytes = std::fs::read(client_key)
            .with_context(|| format!("reading {}", client_key.display()))?;
        let key = PrivateKey::from_pem(&key_bytes)
            .with_context(|| format!("parsing {}", client_key.display()))?;
        let chain = read_certs(client_cert)?;
        let roots = read_certs(ca_cert)?;
        let tls = TlsConfig::builder()
            .client_cert(Some(ClientCert::new_with_certs(&chain, key)))
            .root_certs(RootCerts::Specific(Arc::new(roots)))
            .build();
        let agent: Agent = Agent::config_builder().tls_config(tls).build().into();
        Ok(ApiClient {
            base_url: base_url.trim_end_matches('/').to_string(),
            agent,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn map_err(&self, url: &str, e: ureq::Error) -> ClientError {
        match e {
            ureq::Error::StatusCode(code) => ClientError::Status(code, url.to_string()),
            other => ClientError::Other(anyhow!(other).context(format!("request to {url}"))),
        }
    }

    fn read_body(
        &self,
        url: &str,
        mut resp: ureq::http::Response<ureq::Body>,
    ) -> Result<String, ClientError> {
        resp.body_mut()
            .with_config()
            .limit(u64::MAX)
            .read_to_string()
            .map_err(|e| self.map_err(url, e))
    }

    fn parse_json(&self, url: &str, text: &str) -> Result<Value, ClientError> {
        serde_json::from_str(text).map_err(|e| {
            ClientError::Other(anyhow!(e).context(format!("decoding response from {url}")))
        })
    }

    pub fn post(&self, path: &str, data: &Value) -> Result<Value, ClientError> {
        let url = self.url(path);
        let resp = self
            .agent
            .post(&url)
            .send_json(data)
            .map_err(|e| self.map_err(&url, e))?;
        let text = self.read_body(&url, resp)?;
        self.parse_json(&url, &text)
    }

    pub fn get_text(&self, path: &str) -> Result<String, ClientError> {
        let url = self.url(path);
        let resp = self
            .agent
            .get(&url)
            .call()
            .map_err(|e| self.map_err(&url, e))?;
        self.read_body(&url, resp)
    }

    pub fn get_json(&self, path: &str) -> Result<Value, ClientError> {
        let url = self.url(path);
        let text = self.get_text(path)?;
        self.parse_json(&url, &text)
    }

    pub fn delete(&self, path: &str) -> Result<Value, ClientError> {
        let url = self.url(path);
        let resp = self
            .agent
            .delete(&url)
            .call()
            .map_err(|e| self.map_err(&url, e))?;
        let text = self.read_body(&url, resp)?;
        self.parse_json(&url, &text)
    }
}
