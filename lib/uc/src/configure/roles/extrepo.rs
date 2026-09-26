// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `extrepo`: enable the third-party Apt repositories through extrepo and
//! remove the hand-written files it replaces. Each repository is enabled by
//! its own task, so they are fetched in parallel.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::{apt, copy, file};
use crate::configure::roles::apt as apt_role;
use crate::configure::vars::Vars;
use anyhow::{Result, bail};
use std::path::Path;

const SOURCES_DIR: &str = "/etc/apt/sources.list.d";

#[derive(Debug, Clone)]
struct Repo {
    id: &'static str,
    /// Identifies the repository's `extrepo_*.sources` file.
    uri: String,
    /// Hand-written files this repository replaces.
    legacy: &'static [&'static str],
    /// A package installed once the repository is enabled.
    package: Option<&'static str>,
}

fn repo(id: &'static str, uri: &str, legacy: &'static [&'static str]) -> Repo {
    Repo {
        id,
        uri: uri.to_string(),
        legacy,
        package: None,
    }
}

const BRAVE_LEGACY: &[&str] = &[
    "/etc/apt/sources.list.d/brave-browser-release.sources",
    "/etc/apt/trusted.gpg.d/brave-browser-release.gpg",
];
const BRAVE_URI: &str = "https://brave-browser-apt-release.s3.brave.com";
const VSCODE_URI: &str = "https://packages.microsoft.com/repos/code";

fn mise() -> Repo {
    Repo {
        package: Some("mise"),
        ..repo("mise", "https://mise.jdx.dev/deb", &[])
    }
}

fn repositories(v: &Vars) -> Vec<Repo> {
    if v.facts.distribution == "Ubuntu" {
        // extrepo-data has no Ubuntu tree, so Ubuntu reads the Debian data.
        // Only the repositories whose suite is distro-agnostic ("stable") are
        // enabled here; docker-ce and dotnet publish per-release suites and
        // stay on Ubuntu's own sources.
        return vec![
            repo("brave_release", BRAVE_URI, BRAVE_LEGACY),
            repo(
                "vscode",
                VSCODE_URI,
                &[
                    "/etc/apt/sources.list.d/vscode.list",
                    "/etc/apt/trusted.gpg.d/vscode.gpg",
                ],
            ),
            mise(),
        ];
    }
    let dotnet = match v.facts.distribution_release.as_str() {
        "trixie" => "https://packages.microsoft.com/debian/13/prod",
        _ => "https://packages.microsoft.com/debian/12/prod",
    };
    vec![
        repo("brave_release", BRAVE_URI, BRAVE_LEGACY),
        repo(
            "docker-ce",
            "https://download.docker.com/linux/debian",
            &[
                "/etc/apt/sources.list.d/docker.list",
                "/etc/apt/keyrings/docker.asc",
            ],
        ),
        repo(
            "dotnet",
            dotnet,
            &[
                "/etc/apt/sources.list.d/ms.list",
                "/etc/apt/keyrings/microsoft-prod.gpg",
                "/etc/apt/trusted.gpg.d/microsoft.gpg",
            ],
        ),
        repo(
            "vscode",
            VSCODE_URI,
            &[
                "/etc/apt/sources.list.d/vscode.sources",
                "/etc/apt/keyrings/microsoft-prod.gpg",
            ],
        ),
        repo(
            "mullvad",
            "https://repository.mullvad.net/deb/stable",
            &[
                "/etc/apt/sources.list.d/mullvad.list",
                "/etc/apt/trusted.gpg.d/mullvad-keyring.gpg",
            ],
        ),
        mise(),
    ]
}

/// HashiCorp tools come from mise, so no HashiCorp Apt repository is used.
const DISABLED: &[&str] = &["hashicorp"];

const RETIRED: &[&str] = &[
    "/etc/apt/sources.list.d/hashicorp.list",
    "/etc/apt/keyrings/hashicorp-archive-keyring.gpg",
    "/etc/apt/trusted.gpg.d/hashicorp-archive-keyring.gpg",
];

/// Whether an `extrepo_*.sources` file points at `uri`.
fn enabled(uri: &str) -> Result<bool> {
    let Ok(entries) = std::fs::read_dir(SOURCES_DIR) else {
        return Ok(false);
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("extrepo_")
            && name.ends_with(".sources")
            && std::fs::read_to_string(entry.path()).is_ok_and(|text| text.contains(uri))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn drop_legacy(ctx: &crate::configure::ctx::Ctx, repo: &Repo) -> Result<Outcome> {
    let mut outcome = Outcome::Ok;
    for path in repo.legacy {
        outcome = outcome.and(file::absent(ctx, Path::new(path), true).await?);
    }
    Ok(outcome)
}

pub fn tasks(plan: &mut Plan, v: &Vars) {
    // Runs after the apt files are inflated and before anything installs
    // from them, so extrepo hands over from the legacy sources in one pass.
    plan.add(
        Task::new("extrepo/install", "Install extrepo")
            .tags(&["extrepo"])
            .sudo()
            .after([apt_role::PROXY, apt_role::SOURCES, apt_role::PROBE])
            .run(|ctx| async move {
                // New or changed sources make the cached lists stale.
                if ctx.deps_changed() {
                    apt::update(&ctx, None).await?;
                }
                let hour = Some(std::time::Duration::from_secs(3600));
                apt::install(&ctx, &["extrepo".to_string()], hour).await
            }),
    );

    plan.add(
        Task::new("extrepo/retire", "Remove retired apt repositories")
            .tags(&["extrepo"])
            .sudo()
            .after(["extrepo/install"])
            .run(|ctx| async move {
                let mut outcome = Outcome::Ok;
                for path in RETIRED {
                    outcome = outcome.and(file::absent(&ctx, Path::new(path), true).await?);
                }
                for id in DISABLED {
                    let step = ctx
                        .cmd("extrepo")
                        .args(["disable", id])
                        .sudo()
                        .removes(format!("{SOURCES_DIR}/extrepo_{id}.sources"))
                        .run_step()
                        .await?;
                    outcome = outcome.and(step);
                }
                Ok(outcome)
            }),
    );

    let template = v
        .role("extrepo")
        .join("templates/etc/extrepo/config.yaml.j2");
    plan.add(
        Task::new("extrepo/config", "Install the extrepo config")
            .tags(&["extrepo"])
            .sudo()
            .after(["extrepo/install"])
            .run(move |ctx| async move {
                let dest = Path::new("/etc/extrepo/config.yaml");
                copy::template(&ctx, &template, dest, Some(0o644), true).await
            }),
    );

    let repos = repositories(v);
    for repo in repos.clone() {
        plan.add(
            Task::new(
                format!("extrepo/enable:{}", repo.id),
                format!("Enable the {} repository", repo.id),
            )
            .tags(&["extrepo"])
            .sudo()
            .after(["extrepo/config", "extrepo/retire"])
            .run(move |ctx| async move {
                let existed = enabled(&repo.uri)?;
                if ctx.check() {
                    return Ok(Outcome::changed(!existed));
                }
                let mut outcome = Outcome::Ok;
                if existed {
                    outcome = drop_legacy(&ctx, &repo).await?;
                }
                let out = ctx
                    .cmd("extrepo")
                    .args(["enable", repo.id])
                    .sudo()
                    .any_code()
                    .output()
                    .await?;
                if out.code != 0 || !enabled(&repo.uri)? {
                    bail!(
                        "extrepo enable {} did not produce a source for {}\n{}",
                        repo.id,
                        repo.uri,
                        out.combined().trim_end()
                    );
                }
                let fresh = !existed || !out.combined().contains("already existed");
                outcome = outcome.and(Outcome::changed(fresh));
                Ok(outcome.and(drop_legacy(&ctx, &repo).await?))
            }),
        );
    }

    plan.add(
        Task::new("extrepo/packages", "Install extrepo-managed packages")
            .tags(&["extrepo"])
            .sudo()
            .after(["extrepo/enable:*"])
            .run(move |ctx| async move {
                if ctx.deps_changed() {
                    apt::update(&ctx, None).await?;
                }
                let mut names = Vec::new();
                for repo in &repos {
                    if let Some(package) = repo.package
                        && (ctx.check() || enabled(&repo.uri)?)
                    {
                        names.push(package.to_string());
                    }
                }
                apt::install(&ctx, &names, None).await
            }),
    );
}
