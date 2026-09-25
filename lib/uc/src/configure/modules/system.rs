// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The smaller Ansible modules: `make`, `systemd` (user scope), `group` and
//! `uri` (reachability probes).

use crate::configure::ctx::Ctx;
use crate::configure::engine::Outcome;
use anyhow::Result;
use std::path::Path;

/// `community.general.make`: runs `target` unless `make -q` says it is up
/// to date.
pub async fn make(ctx: &Ctx, dir: &Path, target: Option<&str>, sudo: bool) -> Result<Outcome> {
    let with_target = |cmd: crate::configure::modules::command::Cmd| match target {
        Some(t) => cmd.arg(t),
        None => cmd,
    };
    let query = with_target(ctx.cmd("make").arg("-q").cwd(dir))
        .read_only()
        .any_code()
        .output()
        .await?;
    if query.success() {
        return Ok(Outcome::Ok);
    }
    with_target(ctx.cmd("make").cwd(dir).sudo_if(sudo))
        .run_step()
        .await
}

/// Whether a systemd user instance answers, which it does not in most
/// containers and under WSL without systemd.
pub async fn user_manager_running(ctx: &Ctx) -> Result<bool> {
    let out = ctx
        .cmd("systemctl")
        .args(["--user", "is-system-running"])
        .read_only()
        .any_code()
        .output()
        .await?;
    // Any state but "offline" means a manager replied; a degraded session
    // still manages units.
    let state = out.stdout.trim();
    Ok(!state.is_empty() && state != "offline" && state != "unknown")
}

pub async fn user_daemon_reload(ctx: &Ctx) -> Result<Outcome> {
    ctx.cmd("systemctl")
        .args(["--user", "daemon-reload"])
        .run_step()
        .await
}

/// `systemd: enabled: true, state: started, scope: user`.
pub async fn user_enable_now(ctx: &Ctx, unit: &str) -> Result<Outcome> {
    let query = |verb: &'static str| {
        ctx.cmd("systemctl")
            .args(["--user", verb, unit])
            .read_only()
            .any_code()
            .output()
    };
    let enabled = query("is-enabled").await?.success();
    let active = query("is-active").await?.success();
    if enabled && active {
        return Ok(Outcome::Ok);
    }
    ctx.cmd("systemctl")
        .args(["--user", "enable", "--now", unit])
        .run_step()
        .await
}

/// `group: state: present`.
pub async fn group(ctx: &Ctx, name: &str) -> Result<Outcome> {
    let exists = ctx
        .cmd("getent")
        .args(["group", name])
        .read_only()
        .any_code()
        .output()
        .await?
        .success();
    if exists {
        return Ok(Outcome::Ok);
    }
    ctx.cmd("groupadd").arg(name).sudo().run_step().await
}

/// `uri: method: HEAD`: the HTTP status of `url`, 0 when it cannot be
/// reached at all.
pub async fn http_status(ctx: &Ctx, url: &str, timeout_secs: u32) -> Result<u16> {
    let out = ctx
        .cmd("curl")
        .args(["-sS", "-I", "-o", "/dev/null", "-w", "%{http_code}"])
        .args(["--max-time", &timeout_secs.to_string()])
        .arg(url)
        .read_only()
        .any_code()
        .output()
        .await?;
    Ok(out.stdout.trim().parse().unwrap_or(0))
}
