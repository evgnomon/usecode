// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Ansible's `apt` module. dpkg allows one writer at a time, so every call
//! holds the run-wide `apt` lock: package tasks queue up here while the rest
//! of the graph keeps running.

use crate::configure::ctx::Ctx;
use crate::configure::engine::Outcome;
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, SystemTime};

const LOCK: &str = "apt";

fn apt_get(ctx: &Ctx) -> crate::configure::modules::command::Cmd {
    ctx.cmd("apt-get")
        .sudo()
        .env("DEBIAN_FRONTEND", "noninteractive")
        .args([
            "-q",
            "-y",
            "-o",
            "DPkg::Lock::Timeout=600",
            "-o",
            "Dpkg::Options::=--force-confdef",
            "-o",
            "Dpkg::Options::=--force-confold",
        ])
}

/// `update_cache: true`, skipped while the cache is younger than `valid_for`.
pub async fn update(ctx: &Ctx, valid_for: Option<Duration>) -> Result<Outcome> {
    let _lock = ctx.lock(LOCK).await;
    update_locked(ctx, valid_for).await
}

async fn update_locked(ctx: &Ctx, valid_for: Option<Duration>) -> Result<Outcome> {
    if let Some(valid) = valid_for
        && cache_age().is_some_and(|age| age < valid)
    {
        return Ok(Outcome::Ok);
    }
    apt_get(ctx).arg("update").output().await?;
    Ok(Outcome::Ok)
}

/// When the package lists were last refreshed, the way Ansible measures it.
fn cache_age() -> Option<Duration> {
    let stamp = Path::new("/var/lib/apt/periodic/update-success-stamp");
    let path = if stamp.exists() {
        stamp
    } else {
        Path::new("/var/lib/apt/lists")
    };
    let modified = path.metadata().ok()?.modified().ok()?;
    SystemTime::now().duration_since(modified).ok()
}

/// Which of `names` dpkg reports as installed.
async fn installed(ctx: &Ctx, names: &[String]) -> Result<BTreeSet<String>> {
    let out = ctx
        .cmd("dpkg-query")
        .args(["-W", "-f", "${Package}\t${db:Status-Abbrev}\n"])
        .args(names.iter().cloned())
        .read_only()
        .any_code()
        .output()
        .await?;
    Ok(out
        .stdout
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .filter(|(_, status)| status.starts_with("ii"))
        .map(|(name, _)| name.to_string())
        .collect())
}

/// `state: present`. apt's summary line says whether it did anything.
pub async fn install(
    ctx: &Ctx,
    names: &[String],
    cache_valid_for: Option<Duration>,
) -> Result<Outcome> {
    let names = dedup(names);
    if names.is_empty() {
        return Ok(Outcome::Ok);
    }
    let _lock = ctx.lock(LOCK).await;
    if cache_valid_for.is_some() {
        update_locked(ctx, cache_valid_for).await?;
    }
    let have = installed(ctx, &names).await?;
    let missing: Vec<String> = names.into_iter().filter(|n| !have.contains(n)).collect();
    if missing.is_empty() {
        return Ok(Outcome::Ok);
    }
    // dpkg does not list virtual and transitional names; a simulated
    // install, which needs no root, tells whether they are already covered.
    let simulated = ctx
        .cmd("apt-get")
        .args(["-s", "-q", "install"])
        .args(missing.iter().cloned())
        .read_only()
        .output()
        .await?;
    if !did_something(&simulated.stdout) {
        return Ok(Outcome::Ok);
    }
    if ctx.check() {
        ctx.note(&format!("would install {}", missing.join(" ")));
        return Ok(Outcome::Changed);
    }
    let out = apt_get(ctx)
        .args(["install", "--no-remove"])
        .args(missing)
        .output()
        .await?;
    Ok(Outcome::changed(did_something(&out.stdout)))
}

/// `state: absent`, with `purge: true` when `purge` is set.
pub async fn remove(ctx: &Ctx, names: &[String], purge: bool) -> Result<Outcome> {
    let _lock = ctx.lock(LOCK).await;
    let present: Vec<String> = installed(ctx, &dedup(names)).await?.into_iter().collect();
    if present.is_empty() {
        return Ok(Outcome::Ok);
    }
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    let verb = if purge { "purge" } else { "remove" };
    apt_get(ctx).arg(verb).args(present).output().await?;
    Ok(Outcome::Changed)
}

/// `upgrade: full`.
pub async fn full_upgrade(ctx: &Ctx) -> Result<Outcome> {
    let _lock = ctx.lock(LOCK).await;
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    let out = apt_get(ctx).arg("full-upgrade").output().await?;
    Ok(Outcome::changed(did_something(&out.stdout)))
}

/// `autoremove: true`.
pub async fn autoremove(ctx: &Ctx) -> Result<Outcome> {
    let _lock = ctx.lock(LOCK).await;
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    let out = apt_get(ctx).arg("autoremove").output().await?;
    Ok(Outcome::changed(did_something(&out.stdout)))
}

/// Reads apt's `N upgraded, N newly installed, N to remove` summary.
fn did_something(stdout: &str) -> bool {
    let Some(line) = stdout.lines().find(|l| l.contains(" upgraded, ")) else {
        return false;
    };
    line.split(',')
        .filter_map(|part| part.split_whitespace().next()?.parse::<u64>().ok())
        .any(|n| n > 0)
}

fn dedup(names: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    names
        .iter()
        .filter(|n| !n.is_empty() && seen.insert(n.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_summary_line() {
        assert!(!did_something(
            "Reading package lists...\n0 upgraded, 0 newly installed, 0 to remove and 3 not upgraded.\n"
        ));
        assert!(did_something(
            "1 upgraded, 0 newly installed, 0 to remove and 0 not upgraded.\n"
        ));
        assert!(did_something(
            "0 upgraded, 2 newly installed, 0 to remove and 0 not upgraded.\n"
        ));
    }

    #[test]
    fn dedups_keeping_order() {
        let names: Vec<String> = ["b", "a", "b", ""].iter().map(|s| s.to_string()).collect();
        assert_eq!(dedup(&names), vec!["b".to_string(), "a".to_string()]);
    }
}
