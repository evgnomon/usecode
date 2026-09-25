// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `pkg` and the package charts of the `charts` role: download an archive
//! into the blueprint cache, unpack it and link its executables. Each package
//! is its own task, so the downloads run in parallel.
//!
//! The cache layout is Ansible's, `<cache>/<name>/<os>_<arch>/<name>_<version>`,
//! so existing caches are reused and the desktop entries find their icons.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::command::path_exists;
use crate::configure::modules::file;
use crate::configure::roles::{NOT_IN_DEV_CONTAINER, local_bin};
use crate::configure::vars::{Profile, Vars};
use anyhow::Context;
use minijinja::{Value, context};
use std::path::PathBuf;

const IDEA_VERSION: &str = "2025.2.2";
const IDEA_BUILD: &str = "252.26199.169";
const RIDER_VERSION: &str = "2025.2.2";
const NERDFONTS_VERSION: &str = "3.4.0";
const DEBIAN_RELEASE: &str = "trixie";
const DEBIAN_VERSION: &str = "13";
const DEBIAN_BUILD: &str = "20250924-2245";
const DOCKER_VERSION: &str = "184744";

/// `pkg_versions`, which templates read too.
pub fn versions() -> Value {
    context! {
        idea => context! { version => IDEA_VERSION, version_full => IDEA_BUILD },
        rider => context! { version => RIDER_VERSION },
        nerdfonts => context! { version => NERDFONTS_VERSION },
        debian => context! {
            release => DEBIAN_RELEASE,
            version => DEBIAN_VERSION,
            build => DEBIAN_BUILD,
        },
        docker => context! { version => DOCKER_VERSION },
    }
}

#[derive(Debug, Clone, Copy)]
enum Unpack {
    Unzip,
    TarGz,
    /// The download is the artifact; it is moved into place.
    Move,
}

#[derive(Debug, Clone)]
struct Chart {
    name: &'static str,
    version: &'static str,
    os: String,
    arch: String,
    url: String,
    ext: &'static str,
    unpack: Unpack,
    /// Relative to the unpack directory; its presence means installed.
    creates: String,
    /// Executables linked into ~/.local/bin: (name, path in the package).
    bins: Vec<(&'static str, String)>,
}

impl Chart {
    fn dir(&self, v: &Vars) -> PathBuf {
        v.cache_dir
            .join(self.name)
            .join(format!("{}_{}", self.os, self.arch))
    }

    fn download(&self, v: &Vars) -> PathBuf {
        self.dir(v)
            .join(format!("{}_{}.{}", self.name, self.version, self.ext))
    }

    fn dest(&self, v: &Vars) -> PathBuf {
        self.dir(v).join(format!("{}_{}", self.name, self.version))
    }
}

fn font(name: &'static str, file: &str, creates: &str) -> Chart {
    Chart {
        name,
        version: NERDFONTS_VERSION,
        os: "all".into(),
        arch: "share".into(),
        url: format!(
            "https://github.com/ryanoasis/nerd-fonts/releases/download/v{NERDFONTS_VERSION}/{file}.zip"
        ),
        ext: "zip",
        unpack: Unpack::Unzip,
        creates: creates.into(),
        bins: Vec::new(),
    }
}

fn charts(v: &Vars) -> Vec<Chart> {
    let go_arch = v.facts.go_arch().to_string();
    let mut charts = vec![
        font(
            "jetbrains-mono",
            "JetBrainsMono",
            "JetBrainsMonoNerdFontMono-*.ttf",
        ),
        font("firamono", "FiraMono", "FiraMonoNerdFontMono-*.otf"),
        font(
            "cascadia-mono",
            "CascadiaMono",
            "CaskaydiaMonoNerdFontMono-*.ttf",
        ),
        font(
            "cascadia-code",
            "CascadiaCode",
            "CaskaydiaCoveNerdFontMono-*.ttf",
        ),
        Chart {
            name: "debian",
            version: DEBIAN_VERSION,
            os: "linux".into(),
            url: format!(
                "https://cloud.debian.org/images/cloud/{DEBIAN_RELEASE}/{DEBIAN_BUILD}/debian-{DEBIAN_VERSION}-generic-{go_arch}-{DEBIAN_BUILD}.qcow2"
            ),
            arch: go_arch,
            ext: "bin",
            unpack: Unpack::Move,
            creates: format!("debian_{DEBIAN_VERSION}.bin"),
            bins: Vec::new(),
        },
    ];
    if v.facts.is_debian_family() {
        let jetbrains_arch = match v.facts.architecture.as_str() {
            "aarch64" => "-aarch64",
            _ => "",
        };
        let idea = format!("idea-IC-{IDEA_BUILD}/bin/idea");
        let rider = format!("JetBrains Rider-{RIDER_VERSION}/bin/rider");
        charts.push(Chart {
            name: "idea",
            version: IDEA_VERSION,
            os: "tar.gz".into(),
            arch: jetbrains_arch.into(),
            url: format!(
                "https://download.jetbrains.com/idea/ideaIC-{IDEA_VERSION}{jetbrains_arch}.tar.gz"
            ),
            ext: "tar.gz",
            unpack: Unpack::TarGz,
            creates: idea.clone(),
            bins: vec![("idea", idea)],
        });
        charts.push(Chart {
            name: "rider",
            version: RIDER_VERSION,
            os: "tar.gz".into(),
            arch: jetbrains_arch.into(),
            url: format!(
                "https://download.jetbrains.com/rider/JetBrains.Rider-{RIDER_VERSION}.tar.gz"
            ),
            ext: "tar.gz",
            unpack: Unpack::TarGz,
            creates: rider.clone(),
            bins: vec![("rider", rider)],
        });
    }
    charts
}

pub fn tasks(plan: &mut Plan, v: &Vars) {
    for chart in charts(v) {
        plan.add(
            Task::new(
                format!("pkg/{}", chart.name),
                format!("Install the {} package", chart.name),
            )
            .tags(&["pkg"])
            .after([local_bin::DIR])
            .when(v.profile != Profile::DevContainer, NOT_IN_DEV_CONTAINER)
            .run(move |ctx| async move {
                let v = ctx.vars();
                let (src, dest) = (chart.download(v), chart.dest(v));
                let mut outcome = file::directory(&ctx, &dest, None, false).await?;

                if !path_exists(&dest.join(&chart.creates)) {
                    if !src.exists() {
                        let part = src.with_extension(format!("{}.part", chart.ext));
                        ctx.cmd("curl")
                            .args(["--proto", "=https", "--tlsv1.2", "-sSLf", "-o"])
                            .arg(part.display().to_string())
                            .arg(&chart.url)
                            .output()
                            .await?;
                        if !ctx.check() {
                            std::fs::rename(&part, &src)
                                .with_context(|| format!("saving {}", src.display()))?;
                        }
                    }
                    let (src_s, dest_s) = (src.display().to_string(), dest.display().to_string());
                    match chart.unpack {
                        Unpack::Unzip => ctx.cmd("unzip").args(["-o", "-q", "-d", &dest_s, &src_s]),
                        Unpack::TarGz => ctx.cmd("tar").args(["-C", &dest_s, "-xzf", &src_s]),
                        Unpack::Move => ctx.cmd("mv").args([&src_s, &dest_s]),
                    }
                    .output()
                    .await?;
                    outcome = Outcome::Changed;
                }

                for (name, rel) in &chart.bins {
                    let target = dest.join(rel);
                    let step = file::link(&ctx, &target, &v.local_bin.join(name), false).await?;
                    outcome = outcome.and(step);
                }
                Ok(outcome)
            }),
        );
    }
}
