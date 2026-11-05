// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `windows`: the Windows side of a WSL machine follows the theme, its
//! light or dark app mode and the colors of its VS Code. Each part is
//! skipped when the Windows tool or file it needs isn't there.

use crate::configure::ctx::Ctx;
use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::command::which;
use crate::configure::modules::{file, jsonc};
use crate::configure::vars::{Theme, Vars};
use anyhow::Result;
use std::path::PathBuf;

const ONLY_WSL: &str = "only on WSL";
const PERSONALIZE: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let wsl = v.facts.wsl2();
    let theme = v.config.theme;

    plan.add(
        Task::new(
            "windows/color-scheme",
            "Set the Windows app and system colors to the theme",
        )
        .tags(&["windows", "theme"])
        .when(wsl, ONLY_WSL)
        .run(move |ctx| async move {
            if which(&ctx, "reg.exe").is_none() {
                return Ok(Outcome::Skipped("no reg.exe on the PATH".into()));
            }
            let light = if theme == Theme::Light { "1" } else { "0" };
            let mut outcome = Outcome::Ok;
            for value in ["AppsUseLightTheme", "SystemUsesLightTheme"] {
                let current = ctx
                    .cmd("reg.exe")
                    .args(["query", PERSONALIZE, "/v", value])
                    .read_only()
                    .any_code()
                    .output()
                    .await?;
                if reg_dword(&current.stdout, value) == Some(light == "1") {
                    continue;
                }
                let step = ctx
                    .cmd("reg.exe")
                    .args(["add", PERSONALIZE, "/v", value, "/t", "REG_DWORD"])
                    .args(["/d", light, "/f"])
                    .run_step()
                    .await?;
                outcome = outcome.and(step);
            }
            Ok(outcome)
        }),
    );

    plan.add(
        Task::new(
            "windows/vscode-theme",
            "Set the color theme of the Windows VS Code",
        )
        .tags(&["windows", "theme"])
        .when(wsl, ONLY_WSL)
        .run(move |ctx| async move {
            let Some(path) = windows_appdata(&ctx).await? else {
                return Ok(Outcome::Skipped("no Windows %APPDATA% found".into()));
            };
            let path = path.join("Code/User/settings.json");
            let Some(bytes) = file::read(&ctx, &path, false).await? else {
                return Ok(Outcome::Skipped(
                    "no settings of the Windows VS Code".into(),
                ));
            };
            let text = String::from_utf8_lossy(&bytes);
            let updated = set_vscode_theme(&text, theme);
            file::write(&ctx, &path, updated.as_bytes(), None, false).await
        }),
    );
}

/// Whether `reg query` printed `value` as a REG_DWORD of 1, `None` when it
/// isn't set.
fn reg_dword(stdout: &str, value: &str) -> Option<bool> {
    stdout.lines().find_map(|l| {
        let mut words = l.split_whitespace();
        if words.next() != Some(value) || words.next() != Some("REG_DWORD") {
            return None;
        }
        words.next().map(|n| n != "0x0")
    })
}

/// Where the Windows user's `%APPDATA%` is in WSL.
async fn windows_appdata(ctx: &Ctx) -> Result<Option<PathBuf>> {
    if which(ctx, "cmd.exe").is_none() || which(ctx, "wslpath").is_none() {
        return Ok(None);
    }
    // cmd.exe warns about a WSL working directory on stderr, not stdout.
    let echo = ctx
        .cmd("cmd.exe")
        .args(["/c", "echo", "%APPDATA%"])
        .read_only()
        .any_code()
        .output()
        .await?;
    let dir = echo.stdout.trim();
    if !echo.success() || dir.is_empty() || dir.contains('%') {
        return Ok(None);
    }
    let unix = ctx
        .cmd("wslpath")
        .args(["-u", dir])
        .read_only()
        .any_code()
        .output()
        .await?;
    Ok(unix
        .success()
        .then(|| PathBuf::from(unix.stdout.trim()))
        .filter(|p| p.is_dir()))
}

/// `text`, a VS Code `settings.json`, with `workbench.colorTheme` set to the
/// theme it prefers for `theme`, or Gruvbox. Comments and the other lines
/// stay as they are.
fn set_vscode_theme(text: &str, theme: Theme) -> String {
    let (key, fallback) = match theme {
        Theme::Dark => ("workbench.preferredDarkColorTheme", "Gruvbox Dark Medium"),
        Theme::Light => ("workbench.preferredLightColorTheme", "Gruvbox Light Hard"),
    };
    let name = jsonc::string(text, key).unwrap_or_else(|| fallback.to_string());
    jsonc::set_string(text, "workbench.colorTheme", &name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_reg_dword() {
        let out = "\r\nHKEY_CURRENT_USER\\...\\Personalize\r\n    AppsUseLightTheme    REG_DWORD    0x1\r\n";
        assert_eq!(reg_dword(out, "AppsUseLightTheme"), Some(true));
        assert_eq!(
            reg_dword(out.replace("0x1", "0x0").as_str(), "AppsUseLightTheme"),
            Some(false)
        );
        assert_eq!(reg_dword("", "AppsUseLightTheme"), None);
    }

    #[test]
    fn uses_the_preferred_theme() {
        let t = "// c\n{\n    \"workbench.colorTheme\": \"Default Dark+\",\n    \"workbench.preferredLightColorTheme\": \"Solarized Light\"\n}\n";
        assert_eq!(
            set_vscode_theme(t, Theme::Light),
            "// c\n{\n    \"workbench.colorTheme\": \"Solarized Light\",\n    \"workbench.preferredLightColorTheme\": \"Solarized Light\"\n}\n"
        );
    }

    #[test]
    fn adds_a_missing_theme() {
        assert_eq!(
            set_vscode_theme("{\n    \"a\": 1\n}\n", Theme::Dark),
            "{\n    \"workbench.colorTheme\": \"Gruvbox Dark Medium\",\n    \"a\": 1\n}\n"
        );
        assert_eq!(
            set_vscode_theme("{}\n", Theme::Light),
            "{\n    \"workbench.colorTheme\": \"Gruvbox Light Hard\"\n}\n"
        );
    }
}
