// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `dotfiles`: render the home templates, and `dot`: link the tracked
//! dotfiles from `roles/dotfiles/files` into the home directories, one task
//! per group so the `sync` list picks which ones.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::file;
use crate::configure::modules::inflate::inflate;
use crate::configure::vars::Vars;
use anyhow::Context;
use std::path::{Path, PathBuf};

/// The role of the dotfile groups: `dot/<group>` in the `sync` list links
/// one group, `dot` all of them. None is linked unless listed, and they are
/// left untagged so `-t dot` runs only the listed ones.
pub const DOT: &str = "dot";

/// The task linking a group of [`DOTFILES`].
pub fn dot(group: &str) -> String {
    format!("{DOT}/{group}")
}

/// The dotfiles by group, `src[:dest]`, both relative to the role's files
/// and the home directory.
const DOTFILES: &[(&str, &[&str])] = &[
    (
        "bash",
        &[
            ".bashrc",
            ".bashrc.d/mise-activate.sh",
            ".bash_aliases",
            ".inputrc",
            ".profile",
        ],
    ),
    ("tmux", &[".tmux.conf"]),
    (
        "mise",
        &[".config/mise/config.toml", ".default-python-packages"],
    ),
    ("gnupg", &[".gnupg/gpg.conf", ".gnupg/scdaemon.conf"]),
    ("ctags", &[".ctags", ".ctags.d/javascript.ctags"]),
    ("fd", &[".fdignore"]),
    ("eget", &[".eget.toml"]),
    ("containers", &[".config/containers/registries.conf"]),
    ("hcloud", &[".config/hcloud/cli.sh"]),
    ("yt-dlp", &[".config/yt-dlp/config"]),
    ("opencode", &[".config/opencode/opencode.jsonc"]),
    ("claude", &["claude/settings.json:.claude/settings.json"]),
    (
        "copilot",
        &[
            "copilot/settings.json:.copilot/settings.json",
            "copilot/mcp-config.json:.copilot/mcp-config.json",
        ],
    ),
    (
        "vscode",
        &[
            ".config/Code/User/keybindings.json",
            ".config/Code/User/settings.json",
            ".config/Code/User/tasks.json",
        ],
    ),
];

/// Dotfiles no longer linked, `src[:dest]` like those of `DOTFILES`. A link still
/// pointing at the role's file is replaced by a copy of it, so the home keeps
/// a real file once the role's file is deleted.
const RETIRED: &[&str] = &[".config/Code/extensions.txt"];

/// The `bash` group for root, linked by `dot/root`.
const ROOT_DOTFILES: &[&str] = &[
    ".bashrc",
    ".bashrc.d/mise-activate.sh",
    ".inputrc",
    ".profile",
    ".bash_aliases",
];

/// Directories of the `claude` group linked into the home, (dest, src),
/// relative to it.
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
        .tags(&["dotfiles", "theme"])
        .run(move |ctx| async move {
            inflate(&ctx, &home_templates, &ctx.vars().home, |_| true).await
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

    for &(group, entries) in DOTFILES {
        let files = files.clone();
        let dirs = if group == "claude" {
            dotdirs(v)
        } else {
            Vec::new()
        };
        plan.add(
            Task::new(dot(group), format!("Link the {group} dotfiles")).run(
                move |ctx| async move {
                    let home = &ctx.vars().home;
                    let mut outcome = Outcome::Ok;
                    for entry in entries {
                        let (src, dest) = entry.split_once(':').unwrap_or((entry, entry));
                        let step =
                            file::link(&ctx, &files.join(src), &home.join(dest), false).await?;
                        outcome = outcome.and(step);
                    }
                    for (dest, src) in &dirs {
                        let step =
                            file::link(&ctx, &under(home, src), &under(home, dest), false).await?;
                        outcome = outcome.and(step);
                    }
                    Ok(outcome)
                },
            ),
        );
    }

    let jcode_theme = v.config.theme.name();
    plan.add(
        Task::new("dotfiles/jcode-theme", "Set the jcode theme in its config")
            .tags(&["dotfiles", "theme"])
            .run(move |ctx| async move {
                let path = ctx.vars().home.join(".jcode/config.toml");
                let Some(bytes) = file::read(&ctx, &path, false).await? else {
                    return Ok(Outcome::Ok);
                };
                let text = String::from_utf8_lossy(&bytes);
                let updated = set_jcode_theme(&text, jcode_theme);
                file::write(&ctx, &path, updated.as_bytes(), None, false).await
            }),
    );

    let retired_files = files.clone();
    plan.add(
        Task::new("dotfiles/retired", "Copy retired dotfiles over their links")
            .tags(&["dotfiles"])
            .run(move |ctx| async move {
                let home = &ctx.vars().home;
                let mut outcome = Outcome::Ok;
                for entry in RETIRED {
                    let (src, dest) = entry.split_once(':').unwrap_or((entry, entry));
                    let (src, dest) = (retired_files.join(src), home.join(dest));
                    let linked = matches!(
                        file::stat(&ctx, &dest, false).await?,
                        Some(file::Stat { kind: file::Kind::Link(target), .. }) if target == src
                    );
                    let Some(content) = file::read(&ctx, &src, false).await?.filter(|_| linked)
                    else {
                        continue;
                    };
                    if ctx.check() {
                        outcome = outcome.and(Outcome::Changed);
                        continue;
                    }
                    file::absent(&ctx, &dest, false).await?;
                    outcome = outcome.and(file::write(&ctx, &dest, &content, None, false).await?);
                }
                Ok(outcome)
            }),
    );

    plan.add(
        Task::new(dot("root"), "Link the bash dotfiles for root")
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

    let config_dir = v.config_dir.clone();
    plan.add(
        Task::new(
            "dotfiles/usecode-config",
            "Link ~/.config/usecode to the config repository",
        )
        .tags(&["dotfiles"])
        .when(
            v.config_dir.is_dir(),
            format!("{} is not cloned", v.config_dir.display()),
        )
        .run(move |ctx| async move {
            let dest = ctx.vars().home.join(".config/usecode");
            file::link(&ctx, &config_dir, &dest, false)
                .await
                .with_context(|| {
                    format!(
                        "move what is in {} into {} first",
                        dest.display(),
                        config_dir.display()
                    )
                })
        }),
    );

    plan.add(
        Task::new("dotfiles/gnupg", "Restrict gnupg directory permissions")
            .tags(&["dotfiles"])
            .after([dot("gnupg"), "dotfiles/home-templates".to_string()])
            .run(|ctx| async move {
                file::chmod(&ctx, &ctx.vars().home.join(".gnupg"), 0o700, false).await
            }),
    );
}

/// `text`, a jcode `config.toml`, with `theme` in its `[display]` table set
/// to `theme`. Every other line stays as it is.
fn set_jcode_theme(text: &str, theme: &str) -> String {
    let line = format!("theme = \"{theme}\"");
    let mut out: Vec<String> = Vec::new();
    let (mut in_display, mut done) = (false, false);
    for l in text.lines() {
        let t = l.trim();
        if t.starts_with('[') {
            if in_display && !done {
                out.push(line.clone());
                done = true;
            }
            in_display = t == "[display]";
        } else if in_display && !done && t.split('=').next().map(str::trim) == Some("theme") {
            out.push(line.clone());
            done = true;
            continue;
        }
        out.push(l.to_string());
    }
    if !done {
        if in_display {
            out.push(line);
        } else {
            out.extend(["".into(), "[display]".into(), line]);
        }
    }
    out.join("\n") + "\n"
}

#[cfg(test)]
mod jcode_theme_tests {
    use super::set_jcode_theme;

    #[test]
    fn replaces_existing() {
        let t = "[display]\ntheme = \"light\"\nx = 1\n\n[features]\ntheme = \"no\"\n";
        assert_eq!(
            set_jcode_theme(t, "dark"),
            "[display]\ntheme = \"dark\"\nx = 1\n\n[features]\ntheme = \"no\"\n"
        );
    }

    #[test]
    fn adds_missing() {
        assert_eq!(
            set_jcode_theme("[display]\nx = 1\n\n[f]\n", "dark"),
            "[display]\nx = 1\n\ntheme = \"dark\"\n[f]\n"
        );
        assert!(set_jcode_theme("[a]\n", "dark").ends_with("[display]\ntheme = \"dark\"\n"));
    }
}
