// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc kube configure`: `uc configure` for a Kubernetes cluster instead of
//! this machine.
//!
//! The same [`engine`](crate::configure::engine) runs it, with the same tags,
//! `--check`, `--list` and parallelism. What differs is the target: the
//! facts are the [`Cluster`] kubectl's context points at, read once before
//! any task starts, and the [`roles`] change that cluster with kubectl and
//! helm, both pinned to that context.

pub mod cluster;
pub mod roles;

use crate::configure::ctx::Ctx;
use crate::configure::engine::{Outcome, Plan};
use crate::configure::modules::command::Cmd;
use crate::configure::vars::Vars;
use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::path::PathBuf;
use std::sync::Arc;

pub use cluster::Cluster;

/// Every task for the cluster.
pub fn plan(cluster: &Arc<Cluster>, vars: &Vars) -> Result<Plan> {
    let mut plan = Plan::default();
    roles::hcloud_csi::tasks(&mut plan, cluster, vars)?;
    roles::forgejo::tasks(&mut plan, cluster, vars)?;
    roles::registry::tasks(&mut plan, cluster, vars)?;
    Ok(plan)
}

/// kubectl against the run's context.
pub fn kubectl(ctx: &Ctx, cluster: &Cluster) -> Cmd {
    ctx.cmd("kubectl").args(["--context", &cluster.context])
}

/// helm against the run's context.
pub fn helm(ctx: &Ctx, cluster: &Cluster) -> Cmd {
    ctx.cmd("helm").args(["--kube-context", &cluster.context])
}

/// `kubectl apply` of `manifest`, a change only when kubectl changed
/// something. Under `--check`, `kubectl diff` tells whether it would.
pub async fn apply(ctx: &Ctx, cluster: &Cluster, manifest: &str) -> Result<Outcome> {
    if ctx.check() {
        let diff = kubectl(ctx, cluster)
            .args(["diff", "-f", "-"])
            .stdin(manifest)
            .ok_codes(&[0, 1])
            .read_only()
            .output()
            .await?;
        return Ok(Outcome::changed(diff.code == 1));
    }
    let out = kubectl(ctx, cluster)
        .args(["apply", "-f", "-"])
        .stdin(manifest)
        .output()
        .await?;
    Ok(Outcome::changed(
        out.stdout.lines().any(|l| !l.ends_with(" unchanged")),
    ))
}

/// Read-only kubectl that returns its stdout as JSON.
pub async fn get_json(ctx: &Ctx, cluster: &Cluster, args: &[&str]) -> Result<serde_json::Value> {
    let out = kubectl(ctx, cluster)
        .args(args.iter().copied())
        .args(["-o", "json"])
        .read_only()
        .output()
        .await?;
    serde_json::from_str(&out.stdout)
        .with_context(|| format!("reading `kubectl {}`", args.join(" ")))
}

/// A variable given with `-e` or in the user config, read as `T`.
pub fn var<T: DeserializeOwned>(vars: &Vars, key: &str) -> Result<Option<T>> {
    let Ok(value) = vars.template.get_attr(key) else {
        return Ok(None);
    };
    if value.is_undefined() || value.is_none() {
        return Ok(None);
    }
    let json = serde_json::to_value(&value)?;
    serde_json::from_value(json)
        .map(Some)
        .with_context(|| format!("variable {key}"))
}

/// Where the files written for a cluster are kept, next to the mesh
/// inventory: `~/.config/usecode/kube/<context>`.
pub fn files_dir(vars: &Vars, cluster: &Cluster) -> PathBuf {
    vars.home
        .join(".config/usecode/kube")
        .join(cluster.context.replace('/', "_"))
}
