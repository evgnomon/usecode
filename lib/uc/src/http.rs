// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! HTTPS requests for the REST API clients (`uc ghcr`, `uc vm`).
//!
//! Requests go through `curl`, which keeps this crate free of a TLS stack
//! (and so of C code) and honours the system's CA store and proxy settings.
//! Headers and the body reach curl as a config on stdin, so tokens never show
//! up in `ps`.

use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

/// The status and body of a response, whatever the status.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

impl Response {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Sends `method url` with `headers` (`Name: value`) and an optional body.
pub fn request(
    method: &str,
    url: &str,
    headers: &[String],
    body: Option<&str>,
) -> Result<Response> {
    let what = format!("{method} {url}");
    let mut child = Command::new("curl")
        .args(["--silent", "--show-error", "--location", "--config", "-"])
        .args(["--request", method, "--write-out", "\n%{http_code}"])
        .arg(url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("starting curl")?;
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(config(headers, body).as_bytes())
        .context("sending the request to curl")?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        anyhow::bail!("{what}: curl failed ({})", output.status);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (body, status) =
        split_status(&stdout).with_context(|| format!("{what}: no status from curl"))?;
    Ok(Response {
        status,
        body: body.to_string(),
    })
}

/// A curl config carrying the headers and the body.
fn config(headers: &[String], body: Option<&str>) -> String {
    let mut config: String = headers
        .iter()
        .map(|header| format!("header = \"{}\"\n", quote(header)))
        .collect();
    if let Some(body) = body {
        config += &format!("data-binary = \"{}\"\n", quote(body));
    }
    config
}

/// Escapes a value for a double-quoted curl config string.
fn quote(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Splits curl's output into the body and the status `--write-out` appended.
fn split_status(output: &str) -> Option<(&str, u16)> {
    let (body, status) = output.rsplit_once('\n')?;
    Some((body, status.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_the_status_off_the_body() {
        assert_eq!(split_status("{\"a\":1}\n200"), Some(("{\"a\":1}", 200)));
        assert_eq!(split_status("\n204"), Some(("", 204)));
        assert_eq!(split_status("no status"), None);
    }

    #[test]
    fn config_quotes_headers_and_body() {
        let config = config(
            &["Authorization: Bearer t".to_string()],
            Some("{\"user_data\":\"a\\nb\"}"),
        );
        assert_eq!(
            config,
            "header = \"Authorization: Bearer t\"\n\
             data-binary = \"{\\\"user_data\\\":\\\"a\\\\nb\\\"}\"\n"
        );
    }
}
