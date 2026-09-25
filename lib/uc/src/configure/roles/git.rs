// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `git`: the global git config, and the user's repositories cloned under
//! `~/src/<host>/<owner>/<name>` with their commit identity. Every repository
//! is its own task, so they clone in parallel.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::{file, git};
use crate::configure::vars::{GitRepo, Vars};
use anyhow::{Result, anyhow};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Cloned on every machine, besides the user's own `git_repos`.
const COMMON_REPOS: &[&str] = &["ssh://git@github.com/evgnomon/usecode.git"];

/// Where a repository goes and which identity it commits with.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// `<owner>/<name>`, used in task ids.
    pub key: String,
    pub url: String,
    pub dest: PathBuf,
    pub profile: String,
}

/// Splits a git URL into host and path; both `scheme://[user@]host[:port]/path`
/// and scp-like `user@host:path` are understood.
fn split_url(url: &str) -> Option<(String, String)> {
    let (authority, path) = match url.split_once("://") {
        Some((_, rest)) => {
            let (authority, path) = rest.split_once('/')?;
            (authority, path)
        }
        None => url.split_once(':')?,
    };
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?;
    Some((host.to_string(), path.trim_matches('/').to_string()))
}

pub fn target(home: &Path, repo: &GitRepo) -> Result<Target> {
    let bad = || anyhow!("cannot parse git url '{}'", repo.url);
    let (host, path) = split_url(&repo.url).ok_or_else(bad)?;
    let parts: Vec<&str> = path.split('/').collect();
    let name = parts.last().ok_or_else(bad)?.trim_end_matches(".git");
    let owner = parts.first().ok_or_else(bad)?;
    let key = format!("{owner}/{name}");
    let prefix = repo
        .prefix
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("src"));
    let dest = match &repo.dest {
        Some(dest) => PathBuf::from(dest),
        None => prefix
            .join(&host)
            .join(repo.rewrite.as_deref().unwrap_or(&key)),
    };
    Ok(Target {
        key,
        url: repo.url.clone(),
        dest,
        profile: repo.profile.clone().unwrap_or_else(|| "main".into()),
    })
}

/// The user's repositories and the common ones, each destination once.
fn targets(v: &Vars) -> Vec<Result<Target>> {
    let common = COMMON_REPOS.iter().map(|url| GitRepo {
        url: url.to_string(),
        ..GitRepo::default()
    });
    let mut seen = BTreeSet::new();
    v.config
        .git_repos
        .iter()
        .cloned()
        .chain(common)
        .map(|repo| target(&v.home, &repo))
        .filter(|t| match t {
            Ok(t) => seen.insert(t.dest.clone()),
            Err(_) => true,
        })
        .collect()
}

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let files = v.role("git").join("files");
    plan.add(
        Task::new("git/configs", "Link the git configs")
            .tags(&["dotfiles", "git"])
            .run(move |ctx| async move {
                let home = &ctx.vars().home;
                let mut outcome = Outcome::Ok;
                for name in [".gitconfig", ".gitignore_global"] {
                    outcome = outcome
                        .and(file::link(&ctx, &files.join(name), &home.join(name), false).await?);
                }
                Ok(outcome)
            }),
    );

    for (i, target) in targets(v).into_iter().enumerate() {
        let target = match target {
            Ok(t) => t,
            Err(err) => {
                let msg = err.to_string();
                plan.add(
                    Task::new(format!("git/clone:#{i}"), "Clone a git repository")
                        .tags(&["git"])
                        .ignore_errors()
                        .run(move |_| async move { Err(anyhow!(msg)) }),
                );
                continue;
            }
        };
        let clone = format!("git/clone:{}", target.key);

        let t = target.clone();
        plan.add(
            Task::new(&clone, format!("Clone {}", t.url))
                .tags(&["git"])
                .ignore_errors()
                .run(move |ctx| async move {
                    let spec = git::Checkout {
                        repo: &t.url,
                        dest: &t.dest,
                        ..git::Checkout::default()
                    };
                    spec.run(&ctx).await
                }),
        );

        let t = target.clone();
        plan.add(
            Task::new(format!("git/fetch:{}", t.key), format!("Fetch {}", t.key))
                .tags(&["git_fetch", "never"])
                .after([clone.clone()])
                .run(move |ctx| async move {
                    ctx.cmd("git")
                        .arg("-C")
                        .arg(t.dest.display().to_string())
                        .args(["fetch", "--all"])
                        .run_step()
                        .await
                }),
        );

        let t = target;
        plan.add(
            Task::new(
                format!("git/profile:{}", t.key),
                format!("Set the commit identity of {}", t.key),
            )
            .tags(&["git"])
            .ignore_errors()
            .after([clone])
            .when(v.has_config, "no user config")
            .run(move |ctx| async move { profile(&ctx, &t).await }),
        );
    }
}

async fn profile(ctx: &crate::configure::ctx::Ctx, t: &Target) -> Result<Outcome> {
    let v = ctx.vars();
    if !t.dest.join(".git").exists() {
        return Ok(Outcome::Skipped("not cloned".into()));
    }
    let Some(p) = v.config.profiles.get(&t.profile) else {
        return Ok(Outcome::Skipped(format!("no git profile '{}'", t.profile)));
    };
    let signers = p
        .allowed_signers_file
        .clone()
        .unwrap_or_else(|| v.home.join(".ssh/allowed_signers").display().to_string());
    let settings = [
        ("user.email", p.useremail.clone()),
        (
            "user.name",
            p.fullname.clone().or_else(|| p.username.clone()),
        ),
        ("user.signingKey", p.gpg_key.clone()),
        ("commit.gpgsign", Some("true".into())),
        ("tag.gpgSign", Some("true".into())),
        ("gpg.ssh.allowedSignersFile", Some(signers)),
    ];
    let mut outcome = Outcome::Ok;
    for (key, value) in settings {
        if let Some(value) = value {
            outcome = outcome.and(git::config(ctx, &t.dest, key, &value).await?);
        }
    }
    let format = match p.git_sign_format.as_deref().filter(|f| !f.is_empty()) {
        Some(format) => git::config(ctx, &t.dest, "gpg.format", format).await?,
        None => git::unset(ctx, &t.dest, "gpg.format").await?,
    };
    Ok(outcome.and(format))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(url: &str) -> GitRepo {
        GitRepo {
            url: url.into(),
            ..GitRepo::default()
        }
    }

    #[test]
    fn places_repositories_like_ansible() {
        let home = Path::new("/home/u");
        let t = target(home, &repo("ssh://git@github.com/evgnomon/usecode.git")).unwrap();
        assert_eq!(
            t.dest,
            PathBuf::from("/home/u/src/github.com/evgnomon/usecode")
        );
        assert_eq!(t.key, "evgnomon/usecode");

        let t = target(home, &repo("git@gitlab.com:group/proj.git")).unwrap();
        assert_eq!(t.dest, PathBuf::from("/home/u/src/gitlab.com/group/proj"));

        let t = target(home, &repo("https://host:8443/a/b")).unwrap();
        assert_eq!(t.dest, PathBuf::from("/home/u/src/host/a/b"));

        let t = target(
            home,
            &GitRepo {
                rewrite: Some("x/y".into()),
                prefix: Some("/srv".into()),
                ..repo("ssh://git@github.com/a/b.git")
            },
        )
        .unwrap();
        assert_eq!(t.dest, PathBuf::from("/srv/github.com/x/y"));
    }
}
