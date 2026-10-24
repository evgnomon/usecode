// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `brave_bookmarks`: sync the Brave bookmarks into the `urls` list of the
//! user config, so the picker (fzurls) sees everything Brave has, and the
//! machine configuration holds the same links across machines.
//!
//! The merge is by URL: entries the config already has keep their place
//! (they may hold links picked by hand), bookmarks Brave added are
//! appended in Brave's order, and a name Brave knows is written over the
//! config's. The roots to read are set in the user config:
//!
//!   brave_bookmarks:
//!     roots: [bookmark_bar, other, synced]

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::vars::Vars;
use anyhow::{Context, Result, bail};
use serde_yaml::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

/// The default roots, in the order they appear in the picker: the bar
/// first, then the rest.
const DEFAULT_ROOTS: &[&str] = &["bookmark_bar", "other", "synced"];

/// A bookmark as the role handles it: its path in the tree and its URL.
type Bookmark = (String, String);

/// The Brave profile directories under ~/.config/BraveSoftware, in name
/// order, so the "Default" profile comes before the others.
fn profiles(v: &Vars) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = v
        .home
        .join(".config/BraveSoftware/Brave-Browser")
        .read_dir()
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// Flattens a bookmark node: a folder prefixes the names of what it holds.
fn flatten(name: &str, node: &Value, out: &mut Vec<Bookmark>) {
    let Some(map) = node.as_mapping() else { return };
    let title = map
        .get(Value::from("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let full = match (name.is_empty(), title.is_empty()) {
        (_, true) => name.to_string(),
        (true, _) => title.to_string(),
        (false, false) => format!("{name} / {title}"),
    };
    match map.get(Value::from("url")).and_then(Value::as_str) {
        Some(url) => out.push((full, url.to_string())),
        None => {
            if let Some(children) = map
                .get(Value::from("children"))
                .and_then(Value::as_sequence)
            {
                for child in children {
                    flatten(&full, child, out);
                }
            }
        }
    }
}

/// The bookmarks of one profile's `Bookmarks` file, root by root.
fn bookmarks(path: &std::path::Path, roots: &[String]) -> Result<Vec<Bookmark>> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let json: Value =
        serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))?;
    let mut found = Vec::new();
    for root in roots {
        let node = json
            .get("roots")
            .and_then(|roots| roots.get(root))
            .cloned()
            .unwrap_or(Value::Null);
        // The root's own name ("Bookmarks bar", "Mobile bookmarks") does
        // not belong in the names; its children stand for themselves.
        if let Some(children) = node.get("children").and_then(Value::as_sequence) {
            for child in children {
                flatten("", child, &mut found);
            }
        } else {
            flatten("", &node, &mut found);
        }
    }
    Ok(found)
}

/// The `urls` entries of the config, in order.
fn urls(config: &Value) -> Vec<Bookmark> {
    config
        .get("urls")
        .and_then(Value::as_sequence)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let url = item.get("url").and_then(Value::as_str)?;
                    Some((
                        url.to_string(),
                        item.get("name")
                            .and_then(Value::as_str)
                            .unwrap_or(url)
                            .to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A YAML scalar quoted when it could be misread, plain otherwise.
fn yaml_str(s: &str) -> String {
    let risky = s.is_empty()
        || s.contains(": ")
        || s.contains('#')
        || s.contains('&')
        || s.contains('*')
        || s.contains('!')
        || s.contains('%')
        || s.contains('@')
        || s.contains('`')
        || s.contains('|')
        || s.contains('>')
        || s.starts_with(|c: char| !c.is_ascii_alphanumeric())
        || s.ends_with(':')
        || s.contains("  ");
    if risky {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

/// Renders the `urls` block: quoted where the value would be misread, as
/// in the file today.
fn render(entries: &[Bookmark]) -> String {
    let mut out = String::from("urls:\n");
    for (url, name) in entries {
        let plain = !url.is_empty()
            && url.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '/' || c == '"');
        if plain && !url.contains("# ") && !url.ends_with(':') && !url.contains(": ") {
            out.push_str(&format!("  - url: {}\n", url));
        } else {
            out.push_str(&format!(
                "  - url: \"{}\"\n",
                url.replace('\\', "\\\\").replace('"', "\\\"")
            ));
        }
        out.push_str(&format!("    name: {}\n", yaml_str(name)));
    }
    out
}

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let roots = v
        .config
        .brave_bookmarks
        .as_ref()
        .map(|section| section.roots.clone())
        .unwrap_or_else(|| DEFAULT_ROOTS.iter().map(|s| s.to_string()).collect());

    plan.add(
        Task::new(
            "brave_bookmarks/sync",
            "Merge the Brave bookmarks into the config urls",
        )
        .tags(&["brave_bookmarks"])
        .when(v.has_config, "no user config")
        .run(move |ctx| async move {
            let v = ctx.vars();
            let config_file = &v.config_file;

            // The config as it is on disk, so the merge keeps everything
            // else in the file as it is.
            let raw = std::fs::read_to_string(config_file)
                .with_context(|| format!("reading {}", config_file.display()))?;
            let config: Value = serde_yaml::from_str(&raw)
                .with_context(|| format!("parsing {}", config_file.display()))?;

            // The bookmarks, over every profile there is.
            let mut found: BTreeMap<String, String> = BTreeMap::new();
            let mut sources = 0;
            for dir in profiles(v) {
                let file = dir.join("Bookmarks");
                if !file.is_file() {
                    continue;
                }
                sources += 1;
                for (name, url) in bookmarks(&file, &roots)? {
                    found.entry(url).or_insert(name);
                }
            }
            if sources == 0 {
                return Ok(Outcome::Skipped("no Brave profile has bookmarks".into()));
            }

            // Merge by URL: the entries the config has keep their place
            // and take the name Brave has for them, bookmarks Brave added
            // come after, in Brave's order.
            let existing = urls(&config);
            let known: HashSet<String> = existing.iter().map(|(u, _)| u.clone()).collect();
            let mut merged = existing.clone();
            let mut added = 0;
            for (url, name) in found.iter() {
                if known.contains(url) {
                    continue;
                }
                merged.push((url.clone(), name.clone()));
                added += 1;
            }
            let mut renamed = 0;
            for (url, name) in merged.iter_mut() {
                // A short hand-written name ("X") stays; Brave's path name
                // only takes over when the config has no better one.
                let better = found
                    .get(url.as_str())
                    .filter(|n| !n.is_empty() && **n != *name)
                    .filter(|n| {
                        !(n.split('/').count() > 1 && name.split_whitespace().count() <= 2)
                    });
                if let Some(brave) = better {
                    *name = brave.clone();
                    renamed += 1;
                }
            }

            if added == 0 && renamed == 0 {
                return Ok(Outcome::Ok);
            }
            ctx.note(&format!("{added} new from Brave, {renamed} names updated"));
            if ctx.check() {
                return Ok(Outcome::Changed);
            }

            // Replace the `urls` block in the text, so the rest of the
            // file keeps its shape and comments. The block runs to the
            // next key at the left margin, or to the end of the file.
            let Some(start) = raw.find("\nurls:") else {
                bail!("{} has no urls block", config_file.display());
            };
            let mut end = raw.len();
            for (offset, line) in raw[start + 1..].lines().enumerate().skip(1) {
                let top = !line.trim().is_empty()
                    && !line.starts_with(' ')
                    && !line.starts_with('-')
                    && !line.starts_with('\t');
                if top {
                    end = start
                        + 1
                        + raw[start + 1..]
                            .lines()
                            .take(offset)
                            .map(|l| l.len() + 1)
                            .sum::<usize>();
                    break;
                }
            }
            let updated = format!("{}{}{}", &raw[..start + 1], render(&merged), &raw[end..]);
            std::fs::write(config_file, updated)
                .with_context(|| format!("writing {}", config_file.display()))?;
            Ok(Outcome::Changed)
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Value as Y;

    fn bookmarks_of(yaml: &str) -> Vec<Bookmark> {
        let json: Y = serde_yaml::from_str(yaml).unwrap();
        // Roots pass their children straight through, as bookmarks() does,
        // so the root's own name ("Bookmarks bar") stays out of the paths.
        let mut found = Vec::new();
        match json.get("children").and_then(Y::as_sequence) {
            Some(children) => {
                for child in children {
                    flatten("", child, &mut found);
                }
            }
            None => flatten("", &json, &mut found),
        }
        found
    }

    #[test]
    fn flattens_folders_into_paths() {
        let found = bookmarks_of(
            "name: Bar\nchildren:\n  - name: Folder\n    children:\n      - name: A\n        url: https://a\n      - name: B\n        url: https://b\n  - name: C\n    url: https://c\n",
        );
        assert_eq!(
            found,
            vec![
                ("Folder / A".to_string(), "https://a".to_string()),
                ("Folder / B".to_string(), "https://b".to_string()),
                ("C".to_string(), "https://c".to_string()),
            ]
        );
    }

    #[test]
    fn url_merges_keep_place_and_append() {
        let existing = vec![
            ("https://a".to_string(), "Old A".to_string()),
            ("https://b".to_string(), "B".to_string()),
        ];
        let found: BTreeMap<String, String> = [
            ("https://b".to_string(), "B now".to_string()),
            ("https://c".to_string(), "C".to_string()),
        ]
        .into();
        let known: HashSet<String> = existing.iter().map(|(u, _)| u.clone()).collect();
        let mut merged = existing.clone();
        for (url, name) in found.iter() {
            if !known.contains(url) {
                merged.push((url.clone(), name.clone()));
            }
        }
        for (url, name) in merged.iter_mut() {
            if let Some(brave) = found.get(url.as_str()) {
                *name = brave.clone();
            }
        }
        assert_eq!(
            merged,
            vec![
                ("https://a".to_string(), "Old A".to_string()),
                ("https://b".to_string(), "B now".to_string()),
                ("https://c".to_string(), "C".to_string()),
            ]
        );
    }

    #[test]
    fn urls_read_the_config_list() {
        let config: Y = serde_yaml::from_str(
            "urls:\n  - url: https://a\n    name: A\n  - url: https://b\n    name: \"B: two\"\n",
        )
        .unwrap();
        assert_eq!(
            urls(&config),
            vec![
                ("https://a".to_string(), "A".to_string()),
                ("https://b".to_string(), "B: two".to_string()),
            ]
        );
    }

    #[test]
    fn rendering_round_trips_through_yaml() {
        let entries = vec![
            (
                "outlook.office.com/mail/".to_string(),
                "Outlook".to_string(),
            ),
            ("https://x.com/home".to_string(), "X".to_string()),
            (
                "https://claude.ai/new?q=%s".to_string(),
                "Claude".to_string(),
            ),
            ("https://a".to_string(), "A / path name".to_string()),
            ("https://b".to_string(), "Weird: name".to_string()),
        ];
        let rendered = render(&entries);
        let config: Y = serde_yaml::from_str(&rendered).unwrap();
        assert_eq!(urls(&config), entries);
    }

    #[test]
    fn the_block_replacement_keeps_the_rest_of_the_file() {
        let raw = "before: kept\n\nurls:\n  - url: https://old\n    name: Old\n\nafter: kept\n";
        let config: Y = serde_yaml::from_str(raw).unwrap();
        let existing = urls(&config);
        let merged = vec![("https://new".to_string(), "New".to_string())];
        let _ = existing;
        let start = raw.find("\nurls:").unwrap();
        let mut end = raw.len();
        for (offset, line) in raw[start + 1..].lines().enumerate().skip(1) {
            let top = !line.trim().is_empty()
                && !line.starts_with(' ')
                && !line.starts_with('-')
                && !line.starts_with('\t');
            if top {
                end = start
                    + 1
                    + raw[start + 1..]
                        .lines()
                        .take(offset)
                        .map(|l| l.len() + 1)
                        .sum::<usize>();
                break;
            }
        }
        let updated = format!("{}{}{}", &raw[..start + 1], render(&merged), &raw[end..]);
        assert!(updated.starts_with("before: kept\n\nurls:\n"));
        assert!(updated.ends_with("\nafter: kept\n"));
        let out: Y = serde_yaml::from_str(&updated).unwrap();
        assert_eq!(out["before"], "kept");
        assert_eq!(out["after"], "kept");
        assert_eq!(urls(&out)[0].0, "https://new");
    }
}
