// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `alacritty`: the terminal configuration from `roles/alacritty/files`. On
//! a workstation it is copied into `~/.config/alacritty`; under WSL it goes
//! to the Windows Alacritty in `%APPDATA%\alacritty`, with a shell that opens
//! tmux in this distribution and a maximized window instead of a full screen.
//! The colors come from `roles/alacritty/files/themes`, the one `theme` names.

use crate::configure::ctx::Ctx;
use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::copy;
use crate::configure::roles::WORKSTATION_ONLY;
use crate::configure::vars::{Profile, Theme, Vars};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let shared = v.role("alacritty").join("files/alacritty.toml");
    let theme = theme_file(&v.role("alacritty"), v.config.theme);

    let config = shared.clone();
    let colors = theme.clone();
    plan.add(
        Task::new("alacritty/link", "Copy the Alacritty configuration")
            .tags(&["alacritty", "theme"])
            .when(v.profile == Profile::Workstation, WORKSTATION_ONLY)
            .run(move |ctx| async move {
                let dir = ctx.vars().home.join(".config/alacritty");
                let (main_dest, theme_dest) = (dir.join("alacritty.toml"), dir.join("theme.toml"));
                let main = copy::copy(&ctx, &config, &main_dest, None, false).await?;
                // Alacritty watches its imports, so the open windows switch
                // colors as soon as the theme is written.
                let theme = copy::copy(&ctx, &colors, &theme_dest, None, false).await?;
                Ok(main.and(theme))
            }),
    );

    plan.add(
        Task::new(
            "alacritty/windows",
            "Configure the Windows Alacritty to open WSL",
        )
        .tags(&["alacritty", "theme"])
        .when(v.profile == Profile::Wsl, "only on the wsl profile")
        .run(move |ctx| async move {
            let appdata = windows_appdata(&ctx).await?;
            let dir = wsl_path(&ctx, &appdata).await?.join("alacritty");
            let main_path = dir.join("alacritty.toml");
            let local_path = dir.join("local.toml");
            let local = keep_as_local(&ctx, &main_path, &local_path)?;
            // Alacritty appends the arrays of an imported file to the ones
            // set here, so a shell in the shared copy or in local.toml puts
            // its arguments before ours and the window closes as soon as it
            // opens. Both are imported without one.
            let local_shell = match std::fs::read_to_string(&local_path) {
                Ok(text) if without_shell(&text) != text => {
                    copy::content(&ctx, &without_shell(&text), &local_path, None, false).await?
                }
                _ => Outcome::Ok,
            };
            let text = ctx.vars().assets.read_string(&shared)?;
            let shared_copy = copy::content(
                &ctx,
                &maximized(&without_import(&without_shell(&text))),
                &dir.join("usecode.toml"),
                None,
                false,
            )
            .await?;
            let colors = ctx.vars().assets.read_string(&theme)?;
            let theme_copy =
                copy::content(&ctx, &colors, &dir.join("theme.toml"), None, false).await?;
            let main = copy::content(
                &ctx,
                &windows_config(&appdata, std::env::var("WSL_DISTRO_NAME").ok().as_deref()),
                &main_path,
                None,
                false,
            )
            .await?;
            Ok(local
                .and(local_shell)
                .and(shared_copy)
                .and(theme_copy)
                .and(main))
        }),
    );
}

/// Moves a Windows `alacritty.toml` this role did not write to `local.toml`,
/// which is imported after the shared configuration, so its settings (a font
/// installed on Windows, say) still apply.
fn keep_as_local(ctx: &Ctx, main: &Path, local: &Path) -> Result<Outcome> {
    let ours = std::fs::read_to_string(main).map_or(true, |text| text.starts_with(MARKER));
    if ours || local.exists() {
        return Ok(Outcome::Ok);
    }
    if !ctx.check() {
        std::fs::rename(main, local)
            .with_context(|| format!("moving {} to {}", main.display(), local.display()))?;
    }
    Ok(Outcome::Changed)
}

/// `%APPDATA%` of the Windows user, as a Windows path.
async fn windows_appdata(ctx: &Ctx) -> Result<String> {
    // Started from a Linux directory, cmd.exe warns that UNC paths are not
    // supported, so it runs from the Windows drive.
    let out = ctx
        .cmd("cmd.exe")
        .args(["/d", "/c", "echo %APPDATA%"])
        .cwd("/mnt/c")
        .read_only()
        .output()
        .await?;
    let appdata = out.stdout.trim().to_string();
    if appdata.is_empty() || appdata.contains('%') {
        bail!(
            "cmd.exe did not report %APPDATA%: {}",
            out.combined().trim()
        );
    }
    Ok(appdata)
}

/// The WSL path of a Windows path.
async fn wsl_path(ctx: &Ctx, windows: &str) -> Result<PathBuf> {
    let out = ctx
        .cmd("wslpath")
        .args(["-u", windows])
        .read_only()
        .output()
        .await
        .with_context(|| format!("converting {windows}"))?;
    Ok(PathBuf::from(out.stdout.trim()))
}

/// The colors of `theme`, in the role's directory `role`.
fn theme_file(role: &Path, theme: Theme) -> PathBuf {
    let name = match theme {
        Theme::Dark => "gruvbox-dark",
        Theme::Light => "gruvbox-light-hard",
    };
    role.join(format!("files/themes/{name}.toml"))
}

/// A configuration without its `import = …` line: its paths are the Linux
/// ones, and the Windows `alacritty.toml` imports the copies itself.
fn without_import(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("import ="))
        .map(|line| format!("{line}\n"))
        .collect()
}

/// A configuration without its shell: a `shell = …` line or a
/// `[terminal.shell]` table.
fn without_shell(text: &str) -> String {
    let mut in_shell = false;
    text.lines()
        .filter(|line| {
            let line = line.trim_start();
            if line.starts_with('[') {
                in_shell = line.starts_with("[terminal.shell]");
            }
            !in_shell && !line.starts_with("shell =")
        })
        .map(|line| format!("{line}\n"))
        .collect()
}

/// A configuration that starts maximized: on Windows a full screen window
/// hides the taskbar and the other windows.
fn maximized(text: &str) -> String {
    text.lines()
        .map(|line| {
            if line.trim_start().starts_with("startup_mode =") {
                "startup_mode = \"Maximized\"\n".to_string()
            } else {
                format!("{line}\n")
            }
        })
        .collect()
}

/// How the Windows `alacritty.toml` this role writes starts.
const MARKER: &str = "# Written by `uc sync -t alacritty`";

/// The Windows `alacritty.toml`: the shared configuration and its theme with
/// a shell that starts tmux in `distro`, or in the default distribution when
/// unknown.
fn windows_config(appdata: &str, distro: Option<&str>) -> String {
    let mut args = Vec::new();
    if let Some(distro) = distro.filter(|d| !d.is_empty()) {
        args.extend(["-d", distro]);
    }
    args.extend([
        "--cd",
        "~",
        "--",
        "bash",
        "--login",
        "-c",
        // Alacritty joins the arguments without quoting them and wsl.exe
        // hands the line to the distribution's shell, so the quotes keep
        // this one argument.
        "'tmux new-session -A -s main'",
    ]);
    let args = args
        .iter()
        .map(|a| format!("{a:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{MARKER}; edit local.toml next to this file.\n\
         \n\
         [general]\n\
         import = ['{appdata}\\alacritty\\usecode.toml', '{appdata}\\alacritty\\theme.toml', '{appdata}\\alacritty\\local.toml']\n\
         \n\
         [terminal]\n\
         shell = {{ program = \"wsl.exe\", args = [{args}] }}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_config_opens_the_distro() {
        let text = windows_config(r"C:\Users\me\AppData\Roaming", Some("Debian"));
        assert!(text.contains(
            r"import = ['C:\Users\me\AppData\Roaming\alacritty\usecode.toml', 'C:\Users\me\AppData\Roaming\alacritty\theme.toml', 'C:\Users\me\AppData\Roaming\alacritty\local.toml']"
        ));
        assert!(text.contains(
            r#"shell = { program = "wsl.exe", args = ["-d", "Debian", "--cd", "~", "--", "bash", "--login", "-c", "'tmux new-session -A -s main'"] }"#
        ));
    }

    #[test]
    fn without_shell_drops_the_linux_shell() {
        let text = without_shell("[terminal]\nshell = { program = \"/bin/bash\" }\n\n[window]\n");
        assert_eq!(text, "[terminal]\n\n[window]\n");
    }

    #[test]
    fn without_shell_drops_a_shell_table() {
        let text = without_shell(
            "[font]\nsize = 12.0\n\n[terminal.shell]\nprogram = \"wsl.exe\"\nargs = [\"tmux\"]\n\n[colors.primary]\nbackground = \"#282828\"\n",
        );
        assert_eq!(
            text,
            "[font]\nsize = 12.0\n\n[colors.primary]\nbackground = \"#282828\"\n"
        );
    }

    #[test]
    fn without_import_drops_the_linux_imports() {
        let text = without_import(
            "[general]\nimport = [\"~/.config/alacritty/theme.toml\"]\n\n[env]\nTERM = \"xterm-256color\"\n",
        );
        assert_eq!(text, "[general]\n\n[env]\nTERM = \"xterm-256color\"\n");
    }

    #[test]
    fn theme_file_exists_for_every_theme() {
        let assets = crate::configure::assets::Assets::Embedded;
        for theme in [Theme::Dark, Theme::Light] {
            let file = theme_file(Path::new("alacritty"), theme);
            assert!(assets.is_file(&file), "{theme:?}");
        }
    }

    #[test]
    fn maximized_replaces_fullscreen() {
        let text =
            maximized("[window]\nstartup_mode = \"Fullscreen\"\npadding = { x = 10, y = 0 }\n");
        assert_eq!(
            text,
            "[window]\nstartup_mode = \"Maximized\"\npadding = { x = 10, y = 0 }\n"
        );
    }

    #[test]
    fn windows_config_without_distro_uses_the_default() {
        let text = windows_config(r"C:\x", None);
        assert!(text.contains(r#"args = ["--cd", "~""#));
    }
}
