// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Ansible's `git` module: clone a repository and optionally keep it at a
//! version or at the tip of the remote's default branch.

use crate::configure::ctx::Ctx;
use crate::configure::engine::Outcome;
use crate::configure::modules::file;
use anyhow::{Result, bail};
use std::path::Path;

pub struct Checkout<'a> {
    pub repo: &'a str,
    pub dest: &'a Path,
    /// A tag, branch or commit; the remote's default branch when unset.
    pub version: Option<&'a str>,
    pub depth: Option<u32>,
    /// Fetch and move an existing clone.
    pub update: bool,
    /// Discard local modifications instead of failing on them.
    pub force: bool,
}

impl Default for Checkout<'_> {
    fn default() -> Self {
        Checkout {
            repo: "",
            dest: Path::new(""),
            version: None,
            depth: None,
            update: false,
            force: false,
        }
    }
}

impl Checkout<'_> {
    pub async fn run(&self, ctx: &Ctx) -> Result<Outcome> {
        let dest = self.dest.display().to_string();
        let git = || ctx.cmd("git").arg("-C").arg(dest.clone());

        if !self.dest.join(".git").exists() {
            if let Some(parent) = self.dest.parent() {
                file::directory(ctx, parent, None, false).await?;
            }
            let mut clone = ctx.cmd("git").arg("clone");
            if let Some(depth) = self.depth {
                clone = clone.arg(format!("--depth={depth}"));
            }
            clone = clone.arg(self.repo).arg(&dest);
            clone.output().await?;
            if let Some(version) = self.version {
                self.checkout(ctx, version).await?;
            }
            return Ok(Outcome::Changed);
        }
        if !self.update {
            return Ok(Outcome::Ok);
        }

        let before = head(ctx, &dest).await?;
        let mut fetch = git().args(["fetch", "--tags", "origin"]);
        if let Some(depth) = self.depth {
            fetch = fetch.arg(format!("--depth={depth}"));
        }
        match self.version {
            Some(version) => {
                fetch.output().await?;
                self.checkout(ctx, version).await?;
            }
            None => {
                fetch.arg("HEAD").output().await?;
                if !self.force && dirty(ctx, &dest).await? {
                    bail!("{dest} has local modifications");
                }
                git()
                    .args(["reset", "--hard", "FETCH_HEAD"])
                    .output()
                    .await?;
            }
        }
        Ok(Outcome::changed(head(ctx, &dest).await? != before))
    }

    async fn checkout(&self, ctx: &Ctx, version: &str) -> Result<()> {
        let dest = self.dest.display().to_string();
        let mut checkout =
            ctx.cmd("git")
                .args(["-C", &dest, "-c", "advice.detachedHead=false", "checkout"]);
        if self.force {
            checkout = checkout.arg("--force");
        }
        checkout.arg(version).output().await?;
        Ok(())
    }
}

async fn head(ctx: &Ctx, dest: &str) -> Result<String> {
    let out = ctx
        .cmd("git")
        .args(["-C", dest, "rev-parse", "HEAD"])
        .read_only()
        .output()
        .await?;
    Ok(out.stdout.trim().to_string())
}

async fn dirty(ctx: &Ctx, dest: &str) -> Result<bool> {
    let out = ctx
        .cmd("git")
        .args(["-C", dest, "status", "--porcelain", "--untracked-files=no"])
        .read_only()
        .output()
        .await?;
    Ok(!out.stdout.trim().is_empty())
}

/// Sets `key` in a repository's config unless it already has `value`.
pub async fn config(ctx: &Ctx, repo: &Path, key: &str, value: &str) -> Result<Outcome> {
    let dir = repo.display().to_string();
    let current = ctx
        .cmd("git")
        .args(["-C", &dir, "config", "--local", "--get", key])
        .read_only()
        .any_code()
        .output()
        .await?;
    if current.success() && current.stdout.trim_end_matches('\n') == value {
        return Ok(Outcome::Ok);
    }
    ctx.cmd("git")
        .args(["-C", &dir, "config", "--local", key, value])
        .output()
        .await?;
    Ok(Outcome::Changed)
}

/// Removes `key` from a repository's config if it is set.
pub async fn unset(ctx: &Ctx, repo: &Path, key: &str) -> Result<Outcome> {
    let dir = repo.display().to_string();
    let current = ctx
        .cmd("git")
        .args(["-C", &dir, "config", "--local", "--get", key])
        .read_only()
        .any_code()
        .output()
        .await?;
    if !current.success() {
        return Ok(Outcome::Ok);
    }
    ctx.cmd("git")
        .args(["-C", &dir, "config", "--local", "--unset", key])
        .output()
        .await?;
    Ok(Outcome::Changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure::ctx::tests::ctx;
    use crate::configure::modules::file::scratch;

    #[tokio::test]
    async fn clones_then_follows_the_remote() {
        let dir = scratch("git");
        let origin = dir.join("origin");
        let c = ctx(false);
        let sh = |script: String| c.shell(script).output();
        sh(format!(
            "git init -q -b main {o} && git -C {o} -c user.email=t@t -c user.name=t commit -q --allow-empty -m one",
            o = origin.display()
        ))
        .await
        .unwrap();

        let dest = dir.join("clone/repo");
        let repo = origin.display().to_string();
        let spec = Checkout {
            repo: &repo,
            dest: &dest,
            update: true,
            ..Checkout::default()
        };
        assert_eq!(spec.run(&c).await.unwrap(), Outcome::Changed);
        assert_eq!(spec.run(&c).await.unwrap(), Outcome::Ok);

        sh(format!(
            "git -C {o} -c user.email=t@t -c user.name=t commit -q --allow-empty -m two",
            o = origin.display()
        ))
        .await
        .unwrap();
        assert_eq!(spec.run(&c).await.unwrap(), Outcome::Changed);

        assert_eq!(
            config(&c, &dest, "user.name", "x").await.unwrap(),
            Outcome::Changed
        );
        assert_eq!(
            config(&c, &dest, "user.name", "x").await.unwrap(),
            Outcome::Ok
        );
        assert_eq!(
            unset(&c, &dest, "user.name").await.unwrap(),
            Outcome::Changed
        );
        assert_eq!(unset(&c, &dest, "user.name").await.unwrap(), Outcome::Ok);
    }
}
