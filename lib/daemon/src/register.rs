// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc daemon register`: have hosts call home to usecode.dev, so they show
//! up in the console with their version, uptime and how each module is
//! doing.
//!
//! For each host it asks the usecode API for a one-time enrollment token
//! with your API key and delivers it as the bundle's `callhome` section
//! (`usecoded join`). The daemon there registers with it and from then on
//! calls with a secret of its own (see [`crate::callhome`]); your API key
//! stays on the control node. Registering a host again is fine: it keeps
//! its place in the console and gets a new secret.

use crate::bundle::Bundle;
use crate::callhome::{self, Settings};
use crate::error::{Context, Result};
use crate::inventory::{self, Inventory};
use crate::remote::Target;

const USAGE: &str = "usage: uc daemon register [--usecode-api-key KEY] [--usecode-api URL] NODE...
       uc daemon register [--usecode-api-key KEY] [--usecode-api URL] --all

Have the usecode daemon on each NODE call home to usecode.dev: it
registers the host and says it's alive every few minutes, with the
daemon's version, the uptime and how each module is doing, so you see all
your hosts in one place. Remove a host in the console to stop it calling.

  NODE                    a host in the inventory, or any ssh destination
                          ([USER@]HOST or an alias from ~/.ssh/config)
  --all                   every host in the inventory
  --usecode-api-key KEY   your usecode.dev API key; also UC_USECODE_API_KEY
                          (or USECODE_MCP_API_KEY), or usecode_api_key under
                          uc: in config.yaml. It stays here: each host gets
                          a one-time token instead
  --usecode-api URL       the usecode API to call home to, for your own
                          (default https://api.usecode.dev); also
                          UC_USECODE_API, or usecode_api in config.yaml";

pub const SUMMARY: &str = "have hosts call home to usecode.dev";

pub fn run(args: &[String]) -> Result<()> {
    let mut api_key = None;
    let mut endpoint = None;
    let mut all = false;
    let mut names = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
            _ => (arg.as_str(), None),
        };
        let mut value = |name: &str| {
            inline
                .clone()
                .or_else(|| args.next().cloned())
                .ok_or_else(|| err!("{name} needs a value\n\n{USAGE}"))
        };
        match flag {
            "-h" | "--help" | "help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--summary" => {
                println!("{SUMMARY}");
                return Ok(());
            }
            "--all" => all = true,
            "--usecode-api-key" => api_key = Some(value(flag)?),
            "--usecode-api" => endpoint = Some(value(flag)?),
            f if f.starts_with('-') => bail!("unknown flag {f}\n\n{USAGE}"),
            name => names.push(name.to_string()),
        }
    }

    let api_key = api_key
        .or_else(|| env("UC_USECODE_API_KEY"))
        .or_else(|| env("USECODE_MCP_API_KEY"))
        .ok_or_else(|| {
            err!(
                "this needs your usecode.dev API key: sign in at https://usecode.dev, make one, \
                 and give it with --usecode-api-key, UC_USECODE_API_KEY or usecode_api_key \
                 under uc: in config.yaml"
            )
        })?;
    let endpoint = endpoint
        .or_else(|| env("UC_USECODE_API"))
        .unwrap_or_else(|| callhome::DEFAULT_ENDPOINT.to_string());
    if !endpoint.starts_with("https://") && !endpoint.starts_with("http://") {
        bail!("--usecode-api {endpoint:?} must be an http(s):// URL");
    }

    let inv = Inventory::load(&inventory::find()?).ok();
    if all {
        names.extend(
            inv.as_ref()
                .ok_or_else(|| err!("--all needs the inventory"))?
                .hosts
                .iter()
                .map(|h| h.name.clone()),
        );
    }
    if names.is_empty() {
        bail!("{USAGE}");
    }

    let mut failed = Vec::new();
    for name in &names {
        println!("== {name} ==");
        if let Err(e) = register(inv.as_ref(), name, &endpoint, &api_key) {
            eprintln!("{name}: {e}");
            failed.push(name.as_str());
        }
    }
    if !failed.is_empty() {
        bail!("not registered: {}", failed.join(", "));
    }
    println!("\nThe hosts register within a minute; see them at https://usecode.dev");
    Ok(())
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// Get a token for `name` and hand it to the daemon there.
fn register(inv: Option<&Inventory>, name: &str, endpoint: &str, api_key: &str) -> Result<()> {
    let target = match inv.and_then(|inv| inv.host(name)) {
        Some(host) => Target::for_host(host)?,
        None => Target::ssh(name)?,
    };
    let token = callhome::enroll(endpoint, api_key).ctx("get an enrollment token")?;
    // An ssh destination's host part is the best name it has.
    let short = name.rsplit_once('@').map_or(name, |(_, host)| host);
    let bundle = Bundle {
        host: short.to_string(),
        callhome: Some(Settings {
            endpoint: endpoint.to_string(),
            token,
            name: short.to_string(),
        }),
        ..Bundle::default()
    };
    bundle.deliver(&target)
}
