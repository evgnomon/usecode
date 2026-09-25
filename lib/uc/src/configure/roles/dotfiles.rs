// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `dotfiles`: render the home templates and link the tracked dotfiles from
//! `roles/dotfiles/files` into the home directories.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::file;
use crate::configure::modules::inflate::inflate;
use crate::configure::roles::NOT_IN_DEV_CONTAINER;
use crate::configure::vars::{Profile, Vars};
use std::path::{Path, PathBuf};

pub const LINKS: &str = "dotfiles/links";

/// `src[:dest]`, both relative to the role's files and the home directory.
const DOTFILES: &[&str] = &[
    ".config/containers/registries.conf",
    ".config/hcloud/cli.sh",
    ".config/opencode/opencode.jsonc",
    ".ctags",
    ".gnupg/gpg.conf",
    ".gnupg/scdaemon.conf",
    ".eget.toml",
    ".fdignore",
    ".ctags.d/javascript.ctags",
    ".bashrc",
    ".bashrc.d/mise-activate.sh",
    ".inputrc",
    ".local/bin/foot-shell",
    ".local/bin/bazel",
    ".config/mise/config.toml",
    ".default-python-packages",
    ".profile",
    ".bash_aliases",
    ".tmux.conf",
    "claude/settings.json:.claude/settings.json",
    "copilot/settings.json:.copilot/settings.json",
    "copilot/mcp-config.json:.copilot/mcp-config.json",
    ".config/Code/extensions.txt",
    ".config/Code/User/keybindings.json",
    ".config/Code/User/settings.json",
    ".config/Code/User/tasks.json",
];

const ROOT_DOTFILES: &[&str] = &[
    ".bashrc",
    ".bashrc.d/mise-activate.sh",
    ".inputrc",
    ".profile",
    ".bash_aliases",
];

/// Directories linked into the home, (dest, src), relative to it.
fn dotdirs(v: &Vars) -> Vec<(String, String)> {
    let agents = format!("src/github.com/{}/agents/claude", v.user);
    vec![
        (".claude/skills".into(), format!("{agents}/skills")),
        (".claude/agents".into(), format!("{agents}/agents")),
    ]
}

fn under(home: &Path, path: &str) -> PathBuf {
    if path.starts_with('/') {
        PathBuf::from(path)
    } else {
        home.join(path)
    }
}

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let templates = v.role("dotfiles").join("templates");
    let files = v.role("dotfiles").join("files");

    let home_templates = templates.join("home");
    plan.add(
        Task::new(
            "dotfiles/home-templates",
            "Inflate template configs in the home",
        )
        .tags(&["dotfiles"])
        .run(move |ctx| async move {
            inflate(&ctx, &home_templates, &ctx.vars().home, |_| true).await
        }),
    );

    let desktop_templates = templates.join("desktop");
    plan.add(
        Task::new(
            "dotfiles/desktop-templates",
            "Inflate desktop template configs in the home",
        )
        .tags(&["dotfiles"])
        .when(v.profile != Profile::DevContainer, NOT_IN_DEV_CONTAINER)
        .run(move |ctx| async move {
            inflate(&ctx, &desktop_templates, &ctx.vars().home, |_| true).await
        }),
    );

    let root_templates = templates.join("home");
    plan.add(
        Task::new(
            "dotfiles/root-templates",
            "Inflate bashrc.d templates in /root",
        )
        .tags(&["dotfiles"])
        .sudo()
        .run(move |ctx| async move {
            let root = Path::new("/root");
            let dir = file::directory(&ctx, &root.join(".bashrc.d"), Some(0o755), true).await?;
            let inflated = inflate(&ctx, &root_templates, root, |rel| {
                rel.starts_with(".bashrc.d")
            })
            .await?;
            Ok(dir.and(inflated))
        }),
    );

    let user_files = files.clone();
    plan.add(
        Task::new(LINKS, "Link dotfiles")
            .tags(&["dotfiles"])
            .run(move |ctx| async move {
                let home = &ctx.vars().home;
                let mut outcome =
                    file::directory(&ctx, &home.join(".bashrc.d"), Some(0o755), false).await?;
                for entry in DOTFILES {
                    let (src, dest) = entry.split_once(':').unwrap_or((entry, entry));
                    let step =
                        file::link(&ctx, &user_files.join(src), &home.join(dest), false).await?;
                    outcome = outcome.and(step);
                }
                Ok(outcome)
            }),
    );

    plan.add(
        Task::new("dotfiles/root-links", "Link dotfiles for root")
            .tags(&["dotfiles"])
            .sudo()
            .after(["dotfiles/root-templates"])
            .run(move |ctx| async move {
                let mut outcome = Outcome::Ok;
                for entry in ROOT_DOTFILES {
                    let dest = Path::new("/root").join(entry);
                    outcome = outcome.and(file::link(&ctx, &files.join(entry), &dest, true).await?);
                }
                Ok(outcome)
            }),
    );

    let dirs = dotdirs(v);
    plan.add(
        Task::new("dotfiles/dotdirs", "Link dotdirs")
            .tags(&["dotfiles"])
            .run(move |ctx| async move {
                let home = &ctx.vars().home;
                let mut outcome = Outcome::Ok;
                for (dest, src) in &dirs {
                    let step =
                        file::link(&ctx, &under(home, src), &under(home, dest), false).await?;
                    outcome = outcome.and(step);
                }
                Ok(outcome)
            }),
    );

    plan.add(
        Task::new("dotfiles/gnupg", "Restrict gnupg directory permissions")
            .tags(&["dotfiles"])
            .after([LINKS, "dotfiles/home-templates"])
            .run(|ctx| async move {
                file::chmod(&ctx, &ctx.vars().home.join(".gnupg"), 0o700, false).await
            }),
    );
}
