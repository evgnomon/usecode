// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `dotfiles`: render the home templates, and `dot`: copy the tracked
//! dotfiles from `roles/dotfiles/files` into the home directories, one task
//! per group so the `sync` list picks which ones. The files are built into
//! uc, so a copy is all a machine gets: edit them here, not in the home. The
//! mise config is the exception: one already in the home is left alone.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::command::which;
use crate::configure::modules::inflate::inflate;
use crate::configure::modules::{copy, file, jsonc};
use crate::configure::vars::Vars;
use anyhow::Context;
use std::path::{Path, PathBuf};

/// The role of the dotfile groups: `dot/<group>` in the `sync` list copies
/// one group, `dot` all of them. None is copied unless listed, and they are
/// left untagged so `-t dot` runs only the listed ones.
pub const DOT: &str = "dot";

/// The task copying a group of [`DOTFILES`].
pub fn dot(group: &str) -> String {
    format!("{DOT}/{group}")
}

/// The dotfiles by group, `src[:dest]`, both relative to the role's files
/// and the home directory.
const DOTFILES: &[(&str, &[&str])] = &[
    (
        "bash",
        &[".bashrc", ".bash_aliases", ".inputrc", ".profile"],
    ),
    ("tmux", &[".tmux.conf"]),
    (
        "mise",
        &[
            ".config/mise/config.toml",
            ".default-python-packages",
            ".bashrc.d/mise-activate.sh",
        ],
    ),
    ("zoxide", &[".bashrc.d/zoxide-init.sh"]),
    ("gnupg", &[".gnupg/gpg.conf", ".gnupg/scdaemon.conf"]),
    ("ctags", &[".ctags"]),
    ("fd", &[".fdignore"]),
    ("eget", &[".eget.toml"]),
    ("containers", &[".config/containers/registries.conf"]),
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

/// Dotfiles only put in place when the home has none: a user's own copy is
/// never overwritten, relative to the home.
const SEEDS: &[&str] = &[".config/mise/config.toml"];

/// Dotfiles no longer synced, relative to the home. Earlier syncs linked
/// them into the usecode checkout; such a link is replaced by a copy of what
/// it points to, so the home keeps a real file without the checkout.
const RETIRED: &[&str] = &[".config/Code/extensions.txt"];

/// The `bash` group for root, copied by `dot/root`.
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

/// VS Code's settings, which name the themes it picks in dark and light mode.
const VSCODE_SETTINGS: &str = ".config/Code/User/settings.json";

/// VS Code's `settings.json` with the themes it follows the desktop's color
/// scheme with set to `dark` and `light`.
fn with_vscode_themes(text: &str, dark: &str, light: &str) -> String {
    let text = jsonc::set_string(text, "workbench.preferredDarkColorTheme", dark);
    jsonc::set_string(&text, "workbench.preferredLightColorTheme", light)
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

    let vscode = v.config.themes_of("vscode");
    for &(group, entries) in DOTFILES {
        let files = files.clone();
        let vscode = vscode.clone();
        let dirs = if group == "claude" {
            dotdirs(v)
        } else {
            Vec::new()
        };
        // VS Code's settings name its themes, so `-t theme` puts them in too.
        let tags: &[&str] = if group == "vscode" { &["theme"] } else { &[] };
        plan.add(
            Task::new(dot(group), format!("Copy the {group} dotfiles"))
                .tags(tags)
                .run(move |ctx| async move {
                    let home = &ctx.vars().home;
                    let mut outcome = Outcome::Ok;
                    for entry in entries {
                        let (src, dest) = entry.split_once(':').unwrap_or((entry, entry));
                        let (src, dest) = (files.join(src), home.join(dest));
                        let step = if SEEDS.iter().any(|s| dest.ends_with(s))
                            && file::stat(&ctx, &dest, false).await?.is_some()
                        {
                            Outcome::Ok
                        } else if dest.ends_with(VSCODE_SETTINGS) {
                            let text = ctx.vars().assets.read_string(&src)?;
                            let text = with_vscode_themes(&text, &vscode.0, &vscode.1);
                            copy::content(&ctx, &text, &dest, None, false).await?
                        } else {
                            copy::copy(&ctx, &src, &dest, None, false).await?
                        };
                        outcome = outcome.and(step);
                    }
                    for (dest, src) in &dirs {
                        let step =
                            file::link(&ctx, &under(home, src), &under(home, dest), false).await?;
                        outcome = outcome.and(step);
                    }
                    Ok(outcome)
                }),
        );
    }

    let jcode_theme = v.config.theme_of("jcode");
    plan.add(
        Task::new("dotfiles/jcode-theme", "Set the jcode theme in its config")
            .tags(&["dotfiles", "theme"])
            .run(move |ctx| async move {
                let path = ctx.vars().home.join(".jcode/config.toml");
                let Some(bytes) = file::read(&ctx, &path, false).await? else {
                    return Ok(Outcome::Ok);
                };
                let text = String::from_utf8_lossy(&bytes);
                let updated = set_toml_string(&text, "display", "theme", &jcode_theme);
                file::write(&ctx, &path, updated.as_bytes(), None, false).await
            }),
    );

    // herdr writes its config on the first start; without it there is no
    // herdr to theme.
    let herdr_theme = v.config.theme_of("herdr");
    plan.add(
        Task::new("dotfiles/herdr-theme", "Set the herdr theme in its config")
            .tags(&["dotfiles", "theme"])
            .run(move |ctx| async move {
                let path = ctx.vars().home.join(".config/herdr/config.toml");
                let Some(bytes) = file::read(&ctx, &path, false).await? else {
                    return Ok(Outcome::Skipped("herdr isn't set up".into()));
                };
                let text = String::from_utf8_lossy(&bytes);
                let updated = set_toml_string(&text, "theme", "name", &herdr_theme);
                let outcome = file::write(&ctx, &path, updated.as_bytes(), None, false).await?;
                // A running server picks the change up; none running is fine.
                if outcome == Outcome::Changed && which(&ctx, "herdr").is_some() {
                    ctx.cmd("herdr")
                        .args(["server", "reload-config"])
                        .any_code()
                        .output()
                        .await?;
                }
                Ok(outcome)
            }),
    );

    // opencode keeps its TUI settings, the theme among them, in tui.json.
    let opencode_theme = v.config.theme_of("opencode");
    plan.add(
        Task::new(
            "dotfiles/opencode-theme",
            "Set the opencode theme in its tui.json",
        )
        .tags(&["dotfiles", "theme"])
        .run(move |ctx| async move {
            let dir = ctx.vars().home.join(".config/opencode");
            if !dir.is_dir() && which(&ctx, "opencode").is_none() {
                return Ok(Outcome::Skipped("opencode isn't installed".into()));
            }
            let path = dir.join("tui.json");
            let updated = match file::read(&ctx, &path, false).await? {
                Some(bytes) => {
                    jsonc::set_string(&String::from_utf8_lossy(&bytes), "theme", &opencode_theme)
                }
                None => format!(
                    "{{\n  \"$schema\": \"https://opencode.ai/tui.json\",\n  \"theme\": \"{opencode_theme}\"\n}}\n"
                ),
            };
            file::write(&ctx, &path, updated.as_bytes(), None, false).await
        }),
    );

    plan.add(
        Task::new("dotfiles/retired", "Copy retired dotfiles over their links")
            .tags(&["dotfiles"])
            .run(move |ctx| async move {
                let home = &ctx.vars().home;
                let mut outcome = Outcome::Ok;
                for entry in RETIRED {
                    let dest = home.join(entry);
                    let Some(file::Stat {
                        kind: file::Kind::Link(_),
                        ..
                    }) = file::stat(&ctx, &dest, false).await?
                    else {
                        continue;
                    };
                    // A link to nothing is left alone: there is nothing to keep.
                    let Some(content) = file::read(&ctx, &dest, false).await? else {
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
        Task::new(dot("root"), "Copy the bash dotfiles for root")
            .sudo()
            .after(["dotfiles/root-templates"])
            .run(move |ctx| async move {
                let mut outcome = Outcome::Ok;
                for entry in ROOT_DOTFILES {
                    let (src, dest) = (files.join(entry), Path::new("/root").join(entry));
                    outcome = outcome.and(copy::copy(&ctx, &src, &dest, None, true).await?);
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
            v.config_dir.is_dir() || v.home.join(".config/usecode").is_dir(),
            format!(
                "there is no ~/.config/usecode and {} is not cloned",
                v.config_dir.display()
            ),
        )
        .run(move |ctx| async move {
            let dest = ctx.vars().home.join(".config/usecode");
            // A config made right in ~/.config/usecode, with no repository
            // to link it to, is already where it belongs.
            if !config_dir.is_dir() {
                return Ok(Outcome::Ok);
            }
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

/// `text`, a TOML file, with `key` in its `[table]` set to the string
/// `value`. Every other line stays as it is.
fn set_toml_string(text: &str, table: &str, key: &str, value: &str) -> String {
    let (line, header) = (format!("{key} = \"{value}\""), format!("[{table}]"));
    let mut out: Vec<String> = Vec::new();
    let (mut in_table, mut done) = (false, false);
    for l in text.lines() {
        let t = l.trim();
        if t.starts_with('[') {
            if in_table && !done {
                out.push(line.clone());
                done = true;
            }
            in_table = t == header;
        } else if in_table && !done && t.split('=').next().map(str::trim) == Some(key) {
            out.push(line.clone());
            done = true;
            continue;
        }
        out.push(l.to_string());
    }
    if !done {
        if in_table {
            out.push(line);
        } else {
            out.extend(["".into(), header, line]);
        }
    }
    out.join("\n") + "\n"
}

#[cfg(test)]
mod toml_string_tests {
    use super::set_toml_string;

    fn set_jcode_theme(t: &str, theme: &str) -> String {
        set_toml_string(t, "display", "theme", theme)
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure::assets::Assets;

    #[test]
    fn every_dotfile_is_built_in() {
        let files = Path::new("dotfiles/files");
        let entries = DOTFILES.iter().flat_map(|(_, entries)| entries.iter());
        for entry in entries.chain(ROOT_DOTFILES) {
            let src = entry.split_once(':').map_or(*entry, |(src, _)| src);
            assert!(Assets::Embedded.is_file(&files.join(src)), "{src}");
        }
        assert!(Assets::Embedded.is_dir(Path::new("dotfiles/templates/home")));
    }
}
