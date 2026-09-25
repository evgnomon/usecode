// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `llama-cpp`: build llama.cpp with Vulkan and link its tools.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::{apt, file, git};
use crate::configure::roles::{NOT_IN_DEV_CONTAINER, apt as apt_role, local_bin};
use crate::configure::vars::{Profile, Vars};

const REPO: &str = "https://github.com/ggerganov/llama.cpp";
const MIN_RAM_MB: u64 = 8192;

const BUILD_DEPS: &[&str] = &[
    "git",
    "cmake",
    "make",
    "gcc",
    "g++",
    "libopenblas-dev",
    "libvulkan-dev",
    "vulkan-tools",
    "glslc",
    "spirv-tools",
    "spirv-headers",
    "mesa-vulkan-drivers",
];

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let wanted = v.profile != Profile::DevContainer;
    let dir = v.home.join("src/github.com/ggml-org/llama.cpp");
    let build = dir.join("build");
    let mem = v.facts.memtotal_mb;
    let enough_ram = mem >= MIN_RAM_MB;
    let low_ram = format!("insufficient RAM: {mem}MB < {MIN_RAM_MB}MB");

    plan.add(
        Task::new("llama-cpp/deps", "Install the llama.cpp build dependencies")
            .tags(&["llama-cpp"])
            .sudo()
            .after([apt_role::UPDATE])
            .when(wanted, NOT_IN_DEV_CONTAINER)
            .run(|ctx| async move {
                let names: Vec<String> = BUILD_DEPS.iter().map(|s| s.to_string()).collect();
                apt::install(&ctx, &names, None).await
            }),
    );

    let src = dir.clone();
    plan.add(
        Task::new("llama-cpp/source", "Clone llama.cpp")
            .tags(&["llama-cpp"])
            .when(wanted, NOT_IN_DEV_CONTAINER)
            .run(move |ctx| async move {
                let spec = git::Checkout {
                    repo: REPO,
                    dest: &src,
                    depth: Some(1),
                    update: true,
                    ..git::Checkout::default()
                };
                spec.run(&ctx).await
            }),
    );

    let build_dir = build.clone();
    plan.add(
        Task::new("llama-cpp/build", "Build llama.cpp")
            .tags(&["llama-cpp"])
            .after(["llama-cpp/deps", "llama-cpp/source"])
            .when(wanted, NOT_IN_DEV_CONTAINER)
            .when(enough_ram, low_ram)
            .run(move |ctx| async move {
                // Rebuild when the source or its build dependencies moved.
                let built = build_dir.join("bin/llama-cli").exists();
                if built && !ctx.deps_changed() {
                    return Ok(Outcome::Ok);
                }
                file::directory(&ctx, &build_dir, Some(0o755), false).await?;
                ctx.cmd("cmake")
                    .args(["..", "-DCMAKE_BUILD_TYPE=Release", "-DGGML_VULKAN=ON"])
                    .cwd(&build_dir)
                    .output()
                    .await?;
                ctx.shell("cmake --build . --config Release -j$(nproc --ignore=2)")
                    .cwd(&build_dir)
                    .run_step()
                    .await
            }),
    );

    plan.add(
        Task::new("llama-cpp/link", "Link the llama.cpp tools to ~/.local/bin")
            .tags(&["llama-cpp"])
            .after(["llama-cpp/build", local_bin::DIR])
            .when(wanted, NOT_IN_DEV_CONTAINER)
            .when(enough_ram, "llama.cpp was not built")
            .run(move |ctx| async move {
                let bin = build.join("bin");
                let local = &ctx.vars().local_bin;
                let mut outcome = Outcome::Ok;
                let entries = std::fs::read_dir(&bin)
                    .map(|d| d.flatten().collect::<Vec<_>>())
                    .unwrap_or_default();
                for entry in entries {
                    let name = entry.file_name();
                    if name.to_string_lossy().starts_with("llama-") && entry.path().is_file() {
                        outcome = outcome
                            .and(file::link(&ctx, &entry.path(), &local.join(&name), false).await?);
                    }
                }
                let version = ctx
                    .cmd(local.join("llama-cli").display().to_string())
                    .arg("--version")
                    .read_only()
                    .any_code()
                    .output()
                    .await;
                if let Ok(out) = version {
                    ctx.log(&format!("llama.cpp installed: {}", out.combined().trim()));
                }
                Ok(outcome)
            }),
    );
}
