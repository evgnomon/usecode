// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Ansible's `file` module: directories, symlinks, modes and removal. Paths
//! owned by root are changed through sudo; everything else directly.

use crate::configure::ctx::Ctx;
use crate::configure::engine::Outcome;
use anyhow::{Context, Result, bail};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Whether an operation must go through sudo.
pub(crate) fn elevated(ctx: &Ctx, sudo: bool) -> bool {
    sudo && !ctx.is_root()
}

fn mode_of(path: &Path) -> Option<u32> {
    path.metadata()
        .ok()
        .map(|m| m.permissions().mode() & 0o7777)
}

/// What is at a path, as far as the file operations care.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
    Link(PathBuf),
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stat {
    pub kind: Kind,
    /// The mode of the path, or of its target for a link.
    pub mode: Option<u32>,
}

/// `stat:` without following a final symlink. Paths the user may not look
/// into, such as `/root`, are examined through sudo when the task may use it,
/// so tasks on them stay idempotent.
pub async fn stat(ctx: &Ctx, path: &Path, sudo: bool) -> Result<Option<Stat>> {
    match path.symlink_metadata() {
        Ok(meta) => {
            let kind = if meta.file_type().is_symlink() {
                Kind::Link(std::fs::read_link(path)?)
            } else if meta.is_dir() {
                Kind::Dir
            } else if meta.is_file() {
                Kind::File
            } else {
                Kind::Other
            };
            Ok(Some(Stat {
                kind,
                mode: mode_of(path),
            }))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied && elevated(ctx, sudo) => {
            sudo_stat(ctx, path).await
        }
        Err(err) => Err(err).with_context(|| format!("examining {}", path.display())),
    }
}

async fn sudo_stat(ctx: &Ctx, path: &Path) -> Result<Option<Stat>> {
    let shown = path.display().to_string();
    let query = |args: &[&str]| {
        ctx.cmd("stat")
            .args(args.iter().copied())
            .arg(shown.clone())
            .sudo()
            .read_only()
            .any_code()
            .output()
    };
    let out = query(&["-c", "%F"]).await?;
    if !out.success() {
        return Ok(None);
    }
    let mode = u32::from_str_radix(query(&["-L", "-c", "%a"]).await?.stdout.trim(), 8).ok();
    let kind = match out.stdout.trim() {
        "directory" => Kind::Dir,
        "symbolic link" => {
            let target = ctx
                .cmd("readlink")
                .arg(shown.clone())
                .sudo()
                .read_only()
                .output()
                .await?;
            Kind::Link(PathBuf::from(target.stdout.trim_end_matches('\n')))
        }
        kind if kind.starts_with("regular") => Kind::File,
        _ => Kind::Other,
    };
    Ok(Some(Stat { kind, mode }))
}

/// `state: directory`, creating parents. `mode` is applied when given.
pub async fn directory(ctx: &Ctx, path: &Path, mode: Option<u32>, sudo: bool) -> Result<Outcome> {
    match stat(ctx, path, sudo).await? {
        Some(Stat {
            kind: Kind::Dir,
            mode: current,
        }) => {
            return match mode {
                Some(m) if current != Some(m) => chmod(ctx, path, m, sudo).await,
                _ => Ok(Outcome::Ok),
            };
        }
        // A link to a directory is as good as the directory.
        Some(Stat {
            kind: Kind::Link(_),
            ..
        }) if path.is_dir() => return Ok(Outcome::Ok),
        Some(_) => bail!("{} exists and is not a directory", path.display()),
        None => {}
    }
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    let mode = mode.unwrap_or(0o755);
    if elevated(ctx, sudo) {
        ctx.cmd("install")
            .args(["-d", "-m", &format!("{mode:o}")])
            .arg(path.display().to_string())
            .sudo()
            .output()
            .await?;
    } else {
        std::fs::create_dir_all(path).with_context(|| format!("creating {}", path.display()))?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    }
    Ok(Outcome::Changed)
}

/// `mode:` on an existing path.
pub async fn chmod(ctx: &Ctx, path: &Path, mode: u32, sudo: bool) -> Result<Outcome> {
    let current = stat(ctx, path, sudo).await?.and_then(|s| s.mode);
    if current == Some(mode) {
        return Ok(Outcome::Ok);
    }
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    if elevated(ctx, sudo) {
        ctx.cmd("chmod")
            .arg(format!("{mode:o}"))
            .arg(path.display().to_string())
            .sudo()
            .output()
            .await?;
    } else {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .with_context(|| format!("chmod {}", path.display()))?;
    }
    Ok(Outcome::Changed)
}

/// `state: absent`: removes a file, link or whole directory.
pub async fn absent(ctx: &Ctx, path: &Path, sudo: bool) -> Result<Outcome> {
    let Some(found) = stat(ctx, path, sudo).await? else {
        return Ok(Outcome::Ok);
    };
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    if elevated(ctx, sudo) {
        ctx.cmd("rm")
            .args(["-rf", "--"])
            .arg(path.display().to_string())
            .sudo()
            .output()
            .await?;
    } else {
        let removed = if found.kind == Kind::Dir {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        match removed {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
                return Err(err).with_context(|| format!("removing {}", path.display()));
            }
            _ => {}
        }
    }
    Ok(Outcome::Changed)
}

/// `state: link, force: true`: points `dest` at `src`, replacing a file, a
/// link or an empty directory in the way. Missing parents are created.
pub async fn link(ctx: &Ctx, src: &Path, dest: &Path, sudo: bool) -> Result<Outcome> {
    match stat(ctx, dest, sudo).await? {
        Some(Stat {
            kind: Kind::Link(target),
            ..
        }) if target == src => return Ok(Outcome::Ok),
        Some(Stat {
            kind: Kind::Dir, ..
        }) if std::fs::read_dir(dest).is_ok_and(|mut d| d.next().is_some()) => {
            bail!(
                "cannot link {}: it is a non-empty directory",
                dest.display()
            );
        }
        _ => {}
    }
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    if let Some(parent) = dest.parent() {
        directory(ctx, parent, None, sudo).await?;
    }
    if elevated(ctx, sudo) {
        ctx.cmd("ln")
            .arg("-sfnT")
            .arg(src.display().to_string())
            .arg(dest.display().to_string())
            .sudo()
            .output()
            .await?;
        return Ok(Outcome::Changed);
    }
    // Link under a temporary name and rename over the destination, so there
    // is no moment without a working link.
    let tmp = sibling_tmp(dest);
    std::os::unix::fs::symlink(src, &tmp)
        .with_context(|| format!("linking {} -> {}", dest.display(), src.display()))?;
    if dest.is_dir() && !dest.is_symlink() {
        std::fs::remove_dir(dest)?;
    }
    std::fs::rename(&tmp, dest).map_err(|err| {
        let _ = std::fs::remove_file(&tmp);
        anyhow::Error::new(err).context(format!("replacing {}", dest.display()))
    })?;
    Ok(Outcome::Changed)
}

/// Writes `content` to `dest` unless it already holds exactly that with
/// the right mode. New files get `mode` or 0644; an existing file keeps
/// its mode unless `mode` is given.
pub async fn write(
    ctx: &Ctx,
    dest: &Path,
    content: &[u8],
    mode: Option<u32>,
    sudo: bool,
) -> Result<Outcome> {
    let current = read(ctx, dest, sudo).await?;
    let current_mode = stat(ctx, dest, sudo).await?.and_then(|s| s.mode);
    let want_mode = mode.or(current_mode).unwrap_or(0o644);
    if current.as_deref() == Some(content) {
        return match mode {
            Some(m) if current_mode != Some(m) => chmod(ctx, dest, m, sudo).await,
            _ => Ok(Outcome::Ok),
        };
    }
    if ctx.check() {
        return Ok(Outcome::Changed);
    }
    if let Some(parent) = dest.parent() {
        directory(ctx, parent, None, sudo).await?;
    }
    if elevated(ctx, sudo) {
        let tmp = std::env::temp_dir().join(format!(
            "uc-configure.{}.{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        crate::cli::write_atomic(&tmp, content, 0o600)?;
        let installed = ctx
            .cmd("install")
            .args(["-m", &format!("{want_mode:o}")])
            .arg(tmp.display().to_string())
            .arg(dest.display().to_string())
            .sudo()
            .output()
            .await;
        let _ = std::fs::remove_file(&tmp);
        installed?;
    } else {
        crate::cli::write_atomic(dest, content, want_mode)?;
        // The open mode is filtered by the umask; set it exactly.
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(want_mode))?;
    }
    Ok(Outcome::Changed)
}

/// The current content of `path`, `None` when it does not exist. Files the
/// user cannot read are read through sudo when the task may use it.
pub async fn read(ctx: &Ctx, path: &Path, sudo: bool) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied && elevated(ctx, sudo) => {
            let out = ctx
                .cmd("cat")
                .arg(path.display().to_string())
                .sudo()
                .read_only()
                .output()
                .await?;
            Ok(Some(out.stdout.into_bytes()))
        }
        Err(err) => Err(err).with_context(|| format!("reading {}", path.display())),
    }
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn sibling_tmp(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(format!(
        ".uc.{}.{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    dest.with_file_name(name)
}

/// A fresh scratch directory for a test.
#[cfg(test)]
pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "uc-configure-test-{}-{name}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure::ctx::tests::ctx;

    #[tokio::test]
    async fn write_is_idempotent_and_sets_mode() {
        let dir = scratch("write");
        let f = dir.join("a/b/file");
        let c = ctx(false);
        assert_eq!(
            write(&c, &f, b"x", Some(0o600), false).await.unwrap(),
            Outcome::Changed
        );
        assert_eq!(
            write(&c, &f, b"x", Some(0o600), false).await.unwrap(),
            Outcome::Ok
        );
        assert_eq!(mode_of(&f), Some(0o600));
        assert_eq!(
            write(&c, &f, b"x", Some(0o644), false).await.unwrap(),
            Outcome::Changed
        );
        assert_eq!(mode_of(&f), Some(0o644));
        assert_eq!(
            write(&c, &f, b"y", None, false).await.unwrap(),
            Outcome::Changed
        );
        assert_eq!(std::fs::read(&f).unwrap(), b"y");
    }

    #[tokio::test]
    async fn link_replaces_files_and_is_idempotent() {
        let dir = scratch("link");
        let src = dir.join("src");
        let dest = dir.join("sub/dest");
        std::fs::write(&src, "s").unwrap();
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, "old").unwrap();
        let c = ctx(false);
        assert_eq!(
            link(&c, &src, &dest, false).await.unwrap(),
            Outcome::Changed
        );
        assert_eq!(std::fs::read_link(&dest).unwrap(), src);
        assert_eq!(link(&c, &src, &dest, false).await.unwrap(), Outcome::Ok);

        let full = dir.join("full");
        std::fs::create_dir_all(full.join("x")).unwrap();
        assert!(link(&c, &src, &full, false).await.is_err());
    }

    #[tokio::test]
    async fn absent_and_directory() {
        let dir = scratch("absent");
        let c = ctx(false);
        let d = dir.join("x/y");
        assert_eq!(
            directory(&c, &d, Some(0o700), false).await.unwrap(),
            Outcome::Changed
        );
        assert_eq!(
            directory(&c, &d, Some(0o700), false).await.unwrap(),
            Outcome::Ok
        );
        assert_eq!(
            absent(&c, &dir.join("x"), false).await.unwrap(),
            Outcome::Changed
        );
        assert_eq!(
            absent(&c, &dir.join("x"), false).await.unwrap(),
            Outcome::Ok
        );
    }

    #[tokio::test]
    async fn check_mode_changes_nothing() {
        let dir = scratch("check");
        let c = ctx(true);
        let f = dir.join("f");
        assert_eq!(
            write(&c, &f, b"x", None, false).await.unwrap(),
            Outcome::Changed
        );
        assert!(!f.exists());
    }
}
