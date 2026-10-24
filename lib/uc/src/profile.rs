// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc profile`: which secret stores this machine uses.
//!
//! The stores in `config/secrets` are encrypted to the GPG key in `gpg.key`.
//! A machine without that key - your YubiKey is at home - gets a profile:
//! stores of its own in `secrets/<name>/`, encrypted to a key of its own.
//!
//! A profile is a config file, `profiles/<name>.yaml` next to
//! `config.yaml`, or a directory, `profiles/<name>/`. It may include others
//! and set any key:
//!
//! ```yaml
//! profile: laptop-wsl
//! gpg:
//!   key: 0123456789ABCDEF0123456789ABCDEF01234567
//! ```
//!
//! The active one is linked from `profile.yaml` and laid over `config.yaml`.
//! A directory profile has the same shape as the config root: its own
//! `config.yaml`, a `profile.yaml` over it and `profiles/`, so
//! `main/contract/client-1` is
//! `profiles/main/profiles/contract/profiles/client-1.yaml`, each level
//! linked from the one above. With no `profile` key, the shared stores are
//! used. What gets checked in is up to the user's `.gitignore`.

use crate::configure::vars::{active_profile, read_user_config};
use crate::secret::{status, user_config};
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The active profile, as merged into the user config.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    /// Gives the stores a directory of their own, `secrets/<name>/`; may be
    /// a path, `main/contract`.
    pub name: Option<String>,
    /// The key the profile's vault passwords are encrypted to.
    pub gpg_key: Option<String>,
}

impl Profile {
    /// Read from the user config with everything it includes.
    pub fn load() -> Result<Profile> {
        let file = config_file();
        if !file.is_file() {
            return Ok(Profile::default());
        }
        Profile::from_config(&read_user_config(&file)?)
    }

    fn from_config(config: &serde_yaml::Value) -> Result<Profile> {
        let text = |v: Option<&serde_yaml::Value>| {
            v.and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        let name = text(config.get("profile"));
        if let Some(name) = &name {
            split(name)?;
        }
        Ok(Profile {
            name,
            gpg_key: text(config.get("gpg").and_then(|g| g.get("key"))).map(|k| k.replace(' ', "")),
        })
    }
}

/// A profile path, `main/contract/client-1`: its names, each one path
/// component of the secrets directory.
fn split(path: &str) -> Result<Vec<&str>> {
    let names: Vec<&str> = path.split('/').collect();
    for name in &names {
        if name.is_empty()
            || name.starts_with('.')
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            bail!(
                "profile '{path}': each name must be letters, digits, '-', '_' and '.', \
                 separated by '/'"
            );
        }
    }
    Ok(names)
}

fn config_file() -> PathBuf {
    user_config("config.yaml")
}

/// Where profile `name` is in `profiles`: `<name>.yaml`, or else `<name>/`.
fn find(profiles: &Path, name: &str) -> Option<PathBuf> {
    let file = profiles.join(format!("{name}.yaml"));
    let dir = profiles.join(name);
    if file.is_file() {
        Some(file)
    } else if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

/// The name of a profile entry: `x.yaml` and `x/` are both `x`.
fn entry_name(entry: &Path) -> String {
    let name = entry.file_name().unwrap_or_default().to_string_lossy();
    match entry.is_dir() {
        true => name.into_owned(),
        false => name.trim_end_matches(".yaml").to_string(),
    }
}

/// Every profile, nested ones as `a/b`, in name order.
pub fn list() -> Result<Vec<String>> {
    fn walk(profiles: &Path, prefix: &str, out: &mut Vec<String>) -> Result<()> {
        if !profiles.is_dir() {
            return Ok(());
        }
        let mut entries = fs::read_dir(profiles)
            .with_context(|| format!("reading {}", profiles.display()))?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        entries.retain(|e| e.is_dir() || e.extension().is_some_and(|x| x == "yaml"));
        entries.sort();
        for entry in entries {
            let path = format!("{prefix}{}", entry_name(&entry));
            if !out.contains(&path) {
                out.push(path.clone());
            }
            if entry.is_dir() {
                walk(&entry.join("profiles"), &format!("{path}/"), out)?;
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(&profiles_dir(), "", &mut out)?;
    Ok(out)
}

fn profiles_dir() -> PathBuf {
    user_config("profiles")
}

/// The active profile path, following the `profile.yaml` links down.
pub fn active() -> Result<Option<String>> {
    let mut link = active_profile(&config_file());
    let mut names = Vec::new();
    while link
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        let entry = link
            .parent()
            .expect("links have a directory")
            .join(fs::read_link(&link)?);
        names.push(entry_name(&entry));
        if !entry.is_dir() {
            break;
        }
        link = entry.join("profile.yaml");
    }
    Ok((!names.is_empty()).then(|| names.join("/")))
}

/// Makes `path` the active profile, creating whatever of it is missing and
/// turning a file profile that gets a nested one into a directory. If the
/// config then names no GPG key this machine has, it finds or, once
/// confirmed, generates one for the default git profile's user, giving the
/// outermost new profile stores of its own. Returns whether anything was
/// created.
pub fn switch(path: &str, yes: bool) -> Result<bool> {
    let names = split(path)?;
    let mut profiles = profiles_dir();
    let mut link_at = active_profile(&config_file());
    // The outermost new profile's own file, and its path.
    let mut first_new: Option<(PathBuf, String)> = None;
    let mut entry = PathBuf::new();
    for (i, name) in names.iter().enumerate() {
        let leaf = i + 1 == names.len();
        let so_far = names[..=i].join("/");
        entry = match find(&profiles, name) {
            Some(file) if !leaf && file.is_file() => into_dir(&file)?,
            Some(found) => found,
            None => {
                let (entry, own) = match leaf {
                    true => {
                        let file = profiles.join(format!("{name}.yaml"));
                        (file.clone(), file)
                    }
                    false => (profiles.join(name), profiles.join(name).join("config.yaml")),
                };
                let dir = own.parent().expect("profiles have a directory");
                fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
                fs::write(&own, format!("# The usecode profile {so_far}.\n"))
                    .with_context(|| format!("writing {}", own.display()))?;
                first_new.get_or_insert((own, so_far));
                entry
            }
        };
        relink(&link_at, Some(&entry))?;
        link_at = entry.join("profile.yaml");
        profiles = entry.join("profiles");
    }
    // Exactly this profile: nothing nested in it active.
    if entry.is_dir() {
        relink(&link_at, None)?;
    }
    let created = first_new.is_some();
    // Without a new profile, the outermost one can still take the key, if
    // it hasn't one of its own: an earlier switch may have been declined.
    let target = match first_new {
        Some(new) => Some(new),
        None => {
            let top = find(&profiles_dir(), names[0]).expect("just linked");
            let own = if top.is_dir() {
                top.join("config.yaml")
            } else {
                top
            };
            keyless(&own)?.then(|| (own, names[0].to_string()))
        }
    };
    ensure_key(target, yes)?;
    Ok(created)
}

/// Whether a profile's own file sets neither `gpg` nor `profile`.
fn keyless(own: &Path) -> Result<bool> {
    if !own.exists() {
        return Ok(true);
    }
    let raw = fs::read_to_string(own).with_context(|| format!("reading {}", own.display()))?;
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&raw).with_context(|| format!("parsing {}", own.display()))?;
    Ok(yaml.get("gpg").is_none() && yaml.get("profile").is_none())
}

/// Gives `target`, a profile's own file and its path, a key and stores of
/// its own when the config's key isn't on this machine.
fn ensure_key(target: Option<(PathBuf, String)>, yes: bool) -> Result<()> {
    let current = Profile::load()?;
    if current.gpg_key.as_deref().is_some_and(has_secret_key) {
        return Ok(());
    }
    let Some((own, path)) = target else {
        eprintln!(
            "this machine doesn't have the GPG key {}: the secrets won't open",
            current.gpg_key.as_deref().unwrap_or("(none set)")
        );
        return Ok(());
    };
    let config = read_user_config(&config_file())?;
    let key = gpg_key(&user_id(&config, &path)?, yes)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&own)
        .with_context(|| format!("opening {}", own.display()))?;
    std::io::Write::write_all(&mut file, own_stores_yaml(&path, &key).as_bytes())
        .with_context(|| format!("writing {}", own.display()))
}

/// Turns the file profile `x.yaml` into `x/config.yaml`, so profiles can
/// nest in `x/profiles/`.
fn into_dir(file: &Path) -> Result<PathBuf> {
    let dir = file.with_extension("");
    let own = dir.join("config.yaml");
    if own.exists() {
        bail!(
            "can't move {} to {}: it exists",
            file.display(),
            own.display()
        );
    }
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    fs::rename(file, &own).with_context(|| format!("moving {}", file.display()))?;
    eprintln!("moved {} to {}", file.display(), own.display());
    Ok(dir)
}

/// Goes back to the main config. Returns whether a profile was active.
pub fn off() -> Result<bool> {
    let link = active_profile(&config_file());
    let had = link.symlink_metadata().is_ok();
    relink(&link, None)?;
    Ok(had)
}

/// Points `link` at the profile `entry` next to it, or removes it.
fn relink(link: &Path, entry: Option<&Path>) -> Result<()> {
    if let Ok(meta) = link.symlink_metadata() {
        if !meta.file_type().is_symlink() {
            bail!(
                "{} is not a link to a profile, leaving it alone",
                link.display()
            );
        }
        fs::remove_file(link).with_context(|| format!("removing {}", link.display()))?;
    }
    if let Some(entry) = entry {
        let target = Path::new("profiles").join(entry.file_name().expect("profiles have a name"));
        std::os::unix::fs::symlink(target, link)
            .with_context(|| format!("linking {}", link.display()))?;
    }
    Ok(())
}

/// Whether this machine has the secret part of `key`.
fn has_secret_key(key: &str) -> bool {
    Command::new("gpg")
        .args(["--batch", "--list-secret-keys", key])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The key of `uid`, generated once confirmed if there is none.
fn gpg_key(uid: &str, yes: bool) -> Result<String> {
    if let Some(key) = secret_key(uid)? {
        eprintln!("using the existing key {key} of {uid}");
        return Ok(key);
    }
    if !yes {
        crate::mail::configure::confirm(&format!(
            "Generate a GPG key for {uid}, for secret stores of this profile's own?"
        ))?;
    }
    let mut generate = Command::new("gpg");
    generate.args(["--quick-generate-key", uid, "default", "default", "never"]);
    status(&mut generate, "gpg --quick-generate-key")?;
    secret_key(uid)?.context("gpg made no key")
}

fn own_stores_yaml(path: &str, key: &str) -> String {
    format!(
        "# Its secrets are in secrets/{path}/, encrypted to its own key.\n\
         profile: {path}\n\
         gpg:\n  key: {key}\n"
    )
}

/// `Full Name (profile) <email>` from the default git profile.
fn user_id(config: &serde_yaml::Value, name: &str) -> Result<String> {
    let profile_name = config
        .get("default_user_profile")
        .and_then(|p| p.as_str())
        .unwrap_or("main");
    let profile = config
        .get("profiles")
        .and_then(|p| p.get(profile_name))
        .with_context(|| format!("no profile '{profile_name}' in the config"))?;
    let field = |key: &str| {
        profile
            .get(key)
            .and_then(|v| v.as_str())
            .filter(|v| !v.trim().is_empty())
            .with_context(|| format!("profile '{profile_name}' has no {key}"))
    };
    Ok(format!(
        "{} ({name}) <{}>",
        field("fullname")?,
        field("useremail")?
    ))
}

/// The fingerprint of the newest secret key with exactly this user id.
fn secret_key(uid: &str) -> Result<Option<String>> {
    let mut cmd = Command::new("gpg");
    cmd.args(["--batch", "--with-colons", "--list-secret-keys"])
        .stderr(Stdio::null());
    let out = cmd.output().context("running gpg --list-secret-keys")?;
    Ok(newest_key(&String::from_utf8_lossy(&out.stdout), uid))
}

/// From `gpg --with-colons` output, the newest primary key with `uid`.
fn newest_key(listing: &str, uid: &str) -> Option<String> {
    let mut found = None;
    let (mut fpr, mut created, mut matches) = (None, 0u64, false);
    let mut finish = |fpr: &mut Option<String>, created: u64, matches: bool| {
        if let (Some(f), true) = (fpr.take(), matches)
            && found.as_ref().is_none_or(|(_, c)| created >= *c)
        {
            found = Some((f, created));
        }
    };
    let mut in_primary = false;
    for line in listing.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        match fields.first().copied() {
            Some("sec") => {
                finish(&mut fpr, created, matches);
                created = fields.get(5).and_then(|c| c.parse().ok()).unwrap_or(0);
                matches = false;
                in_primary = true;
            }
            Some("ssb") => in_primary = false,
            Some("fpr") if in_primary && fpr.is_none() => {
                fpr = fields.get(9).map(|f| f.to_string());
            }
            Some("uid") if fields.get(9).is_some_and(|u| *u == uid) => matches = true,
            _ => {}
        }
    }
    finish(&mut fpr, created, matches);
    found.map(|(f, _)| f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_profile_from_the_config() {
        let config = |y: &str| Profile::from_config(&serde_yaml::from_str(y).unwrap());
        let shared = config("gpg:\n  key: C412\n").unwrap();
        assert_eq!(shared.name, None);
        let own = config("profile: wsl\ngpg:\n  key: \"AB CD\"\n").unwrap();
        assert_eq!(own.name.as_deref(), Some("wsl"));
        assert_eq!(own.gpg_key.as_deref(), Some("ABCD"));
        assert!(config("profile: ../x\n").is_err());
        assert!(config("profile: a//b\n").is_err());
        let nested = config("profile: main/contract\n").unwrap();
        assert_eq!(nested.name.as_deref(), Some("main/contract"));
    }

    #[test]
    fn finds_the_newest_key_of_a_user_id() {
        let listing = "\
sec:u:255:22:AAAA:100:::u:::scESC:::+:::ed25519:::0:
fpr:::::::::OLDFPR:
uid:u::::100::H::Me (wsl) <me@x>::::::::::0:
ssb:u:255:18:BBBB:100::::::e:::+:::cv25519::
fpr:::::::::OLDSUB:
sec:u:255:22:CCCC:200:::u:::scESC:::+:::ed25519:::0:
fpr:::::::::NEWFPR:
uid:u::::200::H::Me (wsl) <me@x>::::::::::0:
sec:u:255:22:DDDD:300:::u:::scESC:::+:::ed25519:::0:
fpr:::::::::OTHER:
uid:u::::300::H::Me (box) <me@x>::::::::::0:
";
        assert_eq!(
            newest_key(listing, "Me (wsl) <me@x>").as_deref(),
            Some("NEWFPR")
        );
        assert_eq!(
            newest_key(listing, "Me (box) <me@x>").as_deref(),
            Some("OTHER")
        );
        assert_eq!(newest_key(listing, "nobody"), None);
    }
}
