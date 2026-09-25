// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Build, push and delete images on the GitHub Container Registry for
//! `uc ghcr`.
//!
//! Building and pushing drive the container CLI; deleting goes through the
//! GitHub packages REST API, which is the only way to remove a version.

use anyhow::{Context, Result, bail};
use clap::Args;
use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub const REGISTRY: &str = "ghcr.io";

/// The image being worked on and the credentials to reach it.
#[derive(Args, Debug)]
pub struct Target {
    /// User or organization that owns the image.
    #[arg(short, long)]
    pub owner: String,

    /// Image name, without the registry and owner.
    #[arg(short, long)]
    pub image: String,

    /// GitHub user to authenticate as [default: the owner].
    #[arg(short, long)]
    pub user: Option<String>,

    /// GitHub token with the packages scopes the operation needs.
    #[arg(long, env = "GHCR_TOKEN", hide_env_values = true)]
    pub token: String,
}

impl Target {
    fn user(&self) -> &str {
        self.user.as_deref().unwrap_or(&self.owner)
    }

    /// `ghcr.io/<owner>/<image>:<tag>`, lowercased as the registry requires.
    pub fn reference(&self, tag: &str) -> String {
        format!("{REGISTRY}/{}/{}:{tag}", self.owner, self.image).to_lowercase()
    }
}

/// A tag as the registry accepts it: branch names like `feature/x` become
/// `feature-x`.
pub fn normalize_tag(tag: &str) -> String {
    tag.replace('/', "-")
}

/// How `uc ghcr build` builds and publishes.
#[derive(Debug)]
pub struct Build {
    pub tag: String,
    pub file: PathBuf,
    pub context: PathBuf,
    pub push: bool,
    pub cli: String,
}

/// Logs in to the registry, builds the image (pulling fresh bases) and pushes
/// it when asked.
pub fn build(target: &Target, build: &Build) -> Result<()> {
    let reference = target.reference(&normalize_tag(&build.tag));
    login(&build.cli, target)?;
    eprintln!("Building {reference}");
    run(
        Command::new(&build.cli)
            .args(["build", "--pull", "--rm", "-f"])
            .arg(&build.file)
            .args(["-t", &reference])
            .arg(&build.context),
        "build",
    )?;
    if build.push {
        eprintln!("Pushing {reference}");
        run(Command::new(&build.cli).args(["push", &reference]), "push")?;
    }
    Ok(())
}

fn login(cli: &str, target: &Target) -> Result<()> {
    let mut child = Command::new(cli)
        .args(["login", REGISTRY, "-u", target.user(), "--password-stdin"])
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("starting {cli}"))?;
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(format!("{}\n", target.token).as_bytes())
        .context("sending the token to login")?;
    let status = child.wait()?;
    if !status.success() {
        bail!("login to {REGISTRY} failed ({status}); check the token has the packages scopes");
    }
    Ok(())
}

fn run(cmd: &mut Command, what: &str) -> Result<()> {
    let status = cmd.status().with_context(|| format!("starting {what}"))?;
    if !status.success() {
        bail!("{what} failed ({status})");
    }
    Ok(())
}

/// A client for the GitHub REST API's container package endpoints.
///
/// Requests go through `curl`, which keeps this crate free of a TLS stack
/// (and so of C code) and honours the system's CA store and proxy settings.
pub struct Api {
    base: String,
    token: String,
}

impl Api {
    /// `base` is the API root, `https://api.github.com` outside GitHub
    /// Enterprise.
    pub fn new(base: &str, token: &str) -> Self {
        Api {
            base: base.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    /// Sends one request and returns the body of a 2xx response.
    fn request(&self, method: &str, path: &str) -> Result<String> {
        let url = format!("{}{path}", self.base);
        let what = format!("{method} {url}");
        let mut child = Command::new("curl")
            .args(["--silent", "--show-error", "--location", "--config", "-"])
            .args(["--request", method, "--write-out", "\n%{http_code}"])
            .arg(&url)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .context("starting curl")?;
        // Headers go in on stdin so the token never shows up in `ps`.
        child
            .stdin
            .take()
            .expect("stdin is piped")
            .write_all(curl_config(&self.token).as_bytes())
            .context("sending headers to curl")?;
        let output = child.wait_with_output()?;
        if !output.status.success() {
            bail!("{what}: curl failed ({})", output.status);
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let (body, status) =
            split_status(&stdout).with_context(|| format!("{what}: no status from curl"))?;
        if !(200..300).contains(&status) {
            bail!("{what}: HTTP {status}: {}", body.trim());
        }
        Ok(body.to_string())
    }

    fn get(&self, path: &str) -> Result<Value> {
        let body = self.request("GET", path)?;
        serde_json::from_str(&body).with_context(|| format!("GET {path}: bad JSON"))
    }

    fn delete(&self, path: &str) -> Result<()> {
        self.request("DELETE", path).map(drop)
    }

    /// The packages path prefix for `owner`, which differs for users and
    /// organizations.
    fn owner_path(&self, owner: &str) -> Result<String> {
        let account = self.get(&format!("/users/{owner}"))?;
        Ok(match account["type"].as_str() {
            Some("Organization") => format!("/orgs/{owner}"),
            _ => format!("/users/{owner}"),
        })
    }

    /// Deletes the version of `owner/image` carrying `tag` (or whose digest
    /// is `tag`), returning the id of the version removed.
    pub fn delete_tag(&self, owner: &str, image: &str, tag: &str) -> Result<u64> {
        let versions = format!(
            "{}/packages/container/{}/versions",
            self.owner_path(owner)?,
            image.replace('/', "%2F")
        );
        for page in 1.. {
            let batch = self.get(&format!("{versions}?per_page=100&page={page}"))?;
            let batch = batch.as_array().context("versions is not a list")?;
            if batch.is_empty() {
                break;
            }
            if let Some(id) = find_version(batch, tag) {
                self.delete(&format!("{versions}/{id}"))?;
                return Ok(id);
            }
        }
        bail!("no version of {owner}/{image} is tagged '{tag}'")
    }
}

/// A curl config carrying the API headers.
fn curl_config(token: &str) -> String {
    [
        format!("header = \"Authorization: Bearer {token}\""),
        "header = \"Accept: application/vnd.github+json\"".to_string(),
        "header = \"X-GitHub-Api-Version: 2022-11-28\"".to_string(),
        "user-agent = \"uc-ghcr\"".to_string(),
    ]
    .join("\n")
        + "\n"
}

/// Splits curl's output into the body and the status `--write-out` appended.
fn split_status(output: &str) -> Option<(&str, u16)> {
    let (body, status) = output.rsplit_once('\n')?;
    Some((body, status.trim().parse().ok()?))
}

/// The id of the version tagged `tag`, or whose name (its digest) is `tag`.
fn find_version(versions: &[Value], tag: &str) -> Option<u64> {
    versions.iter().find_map(|v| {
        let tagged = v["metadata"]["container"]["tags"]
            .as_array()
            .is_some_and(|tags| tags.iter().any(|t| t.as_str() == Some(tag)));
        (tagged || v["name"].as_str() == Some(tag))
            .then(|| v["id"].as_u64())
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_a_version_by_tag_or_digest() {
        let versions = json!([
            {"id": 1, "name": "sha256:aaa", "metadata": {"container": {"tags": ["main"]}}},
            {"id": 2, "name": "sha256:bbb", "metadata": {"container": {"tags": ["v1", "latest"]}}},
            {"id": 3, "name": "sha256:ccc", "metadata": {"container": {"tags": []}}},
        ]);
        let versions = versions.as_array().unwrap();
        assert_eq!(find_version(versions, "latest"), Some(2));
        assert_eq!(find_version(versions, "sha256:ccc"), Some(3));
        assert_eq!(find_version(versions, "gone"), None);
    }

    #[test]
    fn splits_the_status_off_the_body() {
        assert_eq!(split_status("{\"a\":1}\n200"), Some(("{\"a\":1}", 200)));
        assert_eq!(split_status("\n204"), Some(("", 204)));
        assert_eq!(split_status("no status"), None);
    }

    #[test]
    fn normalizes_branch_tags_and_lowercases_references() {
        assert_eq!(normalize_tag("feature/x"), "feature-x");
        let target = Target {
            owner: "Evgnomon".into(),
            image: "Ark".into(),
            user: None,
            token: String::new(),
        };
        assert_eq!(target.reference("main"), "ghcr.io/evgnomon/ark:main");
    }
}
