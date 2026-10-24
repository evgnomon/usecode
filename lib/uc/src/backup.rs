// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The SSH, GPG and kubeconfig backup behind `uc secret backup` and
//! `uc secret restore`.
//!
//! A backup is a `.tar.gz` of the current user's `~/.ssh`, `~/.gnupg` and
//! `~/.kube` (`SUDO_USER` under sudo, the home from `/etc/passwd`), with
//! paths relative to `/`, each entry carrying its mode, owner and group by
//! number and by name. Sockets, lock files and kubectl's caches are left
//! out, and symlinks are kept as symlinks, except for a `~/.ssh`, `~/.gnupg`
//! or `~/.kube` that is itself one, whose target's contents are taken.
//!
//! The first entry, `uc-backup.json`, lists the top-level paths with their
//! kind and, for one in a home, the home and its user. A restore ignores the
//! fields it doesn't know and restores kinds it doesn't know as they are, so
//! a backup made by a newer uc that takes more opens here too. A backup
//! without it (from before it was added) is read from its paths and owners.
//!
//! With a password, the archive is encrypted the way `uc secret encrypt`
//! does it, to `<name>.tar.gz.asc`, and opens with `uc secret decrypt` too.
//!
//! A restore puts the backup's home into the current user's, owned by them,
//! whatever their name and home are here. Of a backup with several homes
//! (from before it took only one user) that is the home of the user of the
//! same name, else nothing. Absolute symlinks into the old home are pointed
//! at the new one, and existing files are kept unless asked to replace them.

use crate::crypto;
use anyhow::{Context, Result, bail};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Component, Path, PathBuf};
use tar::{EntryType, Header, HeaderMode};
use zeroize::Zeroizing;

/// The name of the archive entry describing the rest.
const MANIFEST: &str = "uc-backup.json";

/// The directories taken from every home, by kind.
const HOME_DIRS: [(&str, &str); 3] = [("ssh", ".ssh"), ("gpg", ".gnupg"), ("kube", ".kube")];

/// The kinds of secrets to take or put back.
#[derive(Debug, Clone, Copy)]
pub struct Kinds {
    pub ssh: bool,
    pub gpg: bool,
    pub kube: bool,
}

impl Kinds {
    /// Whether secrets of `kind` are asked for. Kinds this uc doesn't know,
    /// from a newer one's backup, always are.
    fn wants(&self, kind: &str) -> bool {
        match kind {
            "ssh" => self.ssh,
            "gpg" => self.gpg,
            "kube" => self.kube,
            _ => true,
        }
    }
}

/// What a backup or restore did, for the caller to print.
#[derive(Debug, Default)]
pub struct Report {
    /// Files, directories and symlinks taken or put back, relative to `/`.
    pub done: Vec<PathBuf>,
    /// How many of `done` are of each kind.
    pub kinds: BTreeMap<String, usize>,
    /// Existing files left as they were.
    pub kept: Vec<PathBuf>,
    /// Anything skipped or moved, and why.
    pub warnings: Vec<String>,
}

impl Report {
    fn did(&mut self, path: PathBuf, kind: &str) {
        self.done.push(path);
        *self.kinds.entry(kind.to_string()).or_default() += 1;
    }
}

/// What `uc-backup.json` says. Every field has a default, so fields added
/// later can be left out, and unknown fields are ignored.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    sources: Vec<Source>,
}

/// A top-level path in a backup, relative to `/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Source {
    path: PathBuf,
    /// `ssh`, `gpg`, `kube`, or one added later.
    kind: String,
    /// The home it is in, if it is in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    home: Option<PathBuf>,
    /// The user whose home that is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user: Option<String>,
}

/// A user from `/etc/passwd`.
#[derive(Debug)]
struct User {
    name: String,
    uid: u32,
    gid: u32,
    home: String,
}

impl User {
    /// The home relative to `/`, unless it is `/` or not absolute.
    fn home(&self) -> Option<PathBuf> {
        let home = self.home.strip_prefix('/')?.trim_start_matches('/');
        (!home.is_empty()).then(|| PathBuf::from(home))
    }
}

/// User and group names and ids, read from a system's `/etc/passwd` and
/// `/etc/group`.
#[derive(Debug, Default)]
struct Accounts {
    users: Vec<User>,
    groups: Vec<(String, u32)>,
}

impl Accounts {
    fn read(root: &Path) -> Self {
        let lines = |file: &str| {
            fs::read_to_string(root.join(file))
                .unwrap_or_default()
                .lines()
                .map(|l| l.split(':').map(str::to_string).collect::<Vec<_>>())
                .filter(|f| f.len() >= 3)
                .collect::<Vec<_>>()
        };
        let mut accounts = Accounts::default();
        for f in lines("etc/passwd") {
            if let Ok(uid) = f[2].parse() {
                accounts.users.push(User {
                    name: f[0].clone(),
                    uid,
                    gid: f.get(3).and_then(|g| g.parse().ok()).unwrap_or(uid),
                    home: f.get(5).cloned().unwrap_or_default(),
                });
            }
        }
        for f in lines("etc/group") {
            if let Ok(gid) = f[2].parse() {
                accounts.groups.push((f[0].clone(), gid));
            }
        }
        accounts
    }

    fn named(&self, name: &str) -> Option<&User> {
        self.users.iter().find(|u| u.name == name)
    }

    fn user(&self, uid: u32) -> Option<&str> {
        self.users
            .iter()
            .find(|u| u.uid == uid)
            .map(|u| u.name.as_str())
    }

    fn group(&self, gid: u32) -> Option<&str> {
        self.groups
            .iter()
            .find(|g| g.1 == gid)
            .map(|g| g.0.as_str())
    }

    /// The user running uc, with a home: `SUDO_USER` under sudo, else the
    /// real user.
    fn current(&self, root: &Path) -> Result<&User> {
        // SAFETY: getuid can't fail.
        let uid = unsafe { libc::getuid() };
        let user = match std::env::var("SUDO_USER") {
            Ok(name) if !name.is_empty() => self.named(&name),
            _ => self.users.iter().find(|u| u.uid == uid),
        };
        let passwd = root.join("etc/passwd");
        match user {
            Some(user) if user.home().is_some() => Ok(user),
            Some(user) => bail!("{} has no home in {}", user.name, passwd.display()),
            None => bail!("no user {uid} in {}", passwd.display()),
        }
    }
}

/// `<hostname>-secrets-<YYYYMMDD>.tar.gz`, the default backup file name,
/// with `.asc` added when it is encrypted.
pub fn default_name(encrypted: bool) -> String {
    let host = fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|h| h.trim().to_string())
        .unwrap_or_else(|_| "host".to_string());
    let date = chrono::Local::now().format("%Y%m%d");
    let asc = if encrypted { ".asc" } else { "" };
    format!("{host}-secrets-{date}.tar.gz{asc}")
}

/// The top-level paths to take from `user`'s home on the system at `root`.
fn sources(root: &Path, user: &User, kinds: Kinds) -> Vec<Source> {
    let home = user.home().unwrap_or_default();
    HOME_DIRS
        .iter()
        .filter(|(kind, dir)| {
            kinds.wants(kind) && fs::metadata(root.join(&home).join(dir)).is_ok_and(|m| m.is_dir())
        })
        .map(|(kind, dir)| Source {
            path: home.join(dir),
            kind: kind.to_string(),
            home: Some(home.clone()),
            user: Some(user.name.clone()),
        })
        .collect()
}

/// Sockets, pipes and gpg's lock files: things that only mean something
/// while a program is running. And kubectl's caches in `~/.kube`, which it
/// fills again by itself.
fn transient(path: &Path, meta: &fs::Metadata) -> bool {
    let kind = meta.file_type();
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let in_kube = path
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|p| p == ".kube");
    (in_kube && (name == "cache" || name == "http-cache"))
        || kind.is_socket()
        || kind.is_fifo()
        || kind.is_block_device()
        || kind.is_char_device()
        || name.starts_with(".#lk")
        || name.ends_with(".lock")
}

/// Writes a backup of the system at `root` (`/` normally) to `out`, which
/// is created readable by its owner only, encrypted with `password` if given.
pub fn backup(root: &Path, kinds: Kinds, out: &Path, password: Option<&[u8]>) -> Result<Report> {
    let accounts = Accounts::read(root);
    let sources = sources(root, accounts.current(root)?, kinds);
    let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    let mut report = Report::default();

    let manifest = serde_json::to_vec_pretty(&Manifest {
        version: 1,
        sources: sources.clone(),
    })?;
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_mode(0o600);
    header.set_size(manifest.len() as u64);
    header.set_mtime(chrono::Utc::now().timestamp().max(0) as u64);
    tar.append_data(&mut header, MANIFEST, &manifest[..])?;

    for source in &sources {
        let path = root.join(&source.path);
        add(
            &mut tar,
            root,
            &path,
            true,
            &source.kind,
            &accounts,
            &mut report,
        )?;
    }
    let archive = Zeroizing::new(
        tar.into_inner()
            .context("writing the archive")?
            .finish()
            .context("compressing the archive")?,
    );
    let contents = match password {
        None => archive,
        Some(password) => {
            let armored = crypto::armor(&crypto::encrypt_raw(&archive, password)?);
            // Check the round trip before the keys depend on it.
            let (check, _) = crypto::decrypt_raw(&crypto::dearmor(armored.as_bytes())?, password)?;
            if check != *archive {
                bail!("the encrypted archive does not decrypt back; nothing written");
            }
            Zeroizing::new(armored.into_bytes())
        }
    };
    crate::cli::write_atomic(out, &contents, 0o600)?;
    Ok(report)
}

/// Adds `path` and, for a directory, everything under it. `top` follows a
/// symlink, so a linked `~/.ssh` is taken as the directory it points to.
fn add<W: io::Write>(
    tar: &mut tar::Builder<W>,
    root: &Path,
    path: &Path,
    top: bool,
    kind: &str,
    accounts: &Accounts,
    report: &mut Report,
) -> Result<()> {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let meta = match if top {
        fs::metadata(path)
    } else {
        fs::symlink_metadata(path)
    } {
        Ok(meta) => meta,
        Err(err) => {
            report.warnings.push(format!("/{}: {err}", rel.display()));
            return Ok(());
        }
    };
    if transient(path, &meta) {
        return Ok(());
    }

    let mut header = Header::new_gnu();
    header.set_metadata_in_mode(&meta, HeaderMode::Complete);
    if let Some(name) = accounts.user(meta.uid()) {
        let _ = header.set_username(name);
    }
    if let Some(name) = accounts.group(meta.gid()) {
        let _ = header.set_groupname(name);
    }

    if meta.is_dir() {
        let entries = match fs::read_dir(path) {
            Ok(dir) => dir,
            Err(err) => {
                report.warnings.push(format!("/{}: {err}", rel.display()));
                return Ok(());
            }
        };
        tar.append_data(&mut header, rel, io::empty())?;
        report.did(rel.to_path_buf(), kind);
        let mut children: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        children.sort();
        for child in children {
            add(tar, root, &child, false, kind, accounts, report)?;
        }
    } else if meta.file_type().is_symlink() {
        let target = fs::read_link(path)?;
        tar.append_link(&mut header, rel, target)?;
        report.did(rel.to_path_buf(), kind);
    } else {
        match File::open(path) {
            Ok(file) => {
                tar.append_data(&mut header, rel, file)?;
                report.did(rel.to_path_buf(), kind);
            }
            Err(err) => report.warnings.push(format!("/{}: {err}", rel.display())),
        }
    }
    Ok(())
}

/// The source `rel` is in: the deepest one, the first of equals.
fn source_of<'a>(sources: &'a [Source], rel: &Path) -> Option<&'a Source> {
    sources
        .iter()
        .rev()
        .filter(|s| rel.starts_with(&s.path))
        .max_by_key(|s| s.path.components().count())
}

/// The source of an entry of a backup without a manifest, from its path,
/// and the owner of its first entry, the top directory, for the user.
fn infer(rel: &Path, user: Option<String>) -> Option<Source> {
    let source = |path: PathBuf, kind: &str, home: Option<PathBuf>, user| Source {
        path,
        kind: kind.to_string(),
        home,
        user,
    };
    if rel.starts_with("etc/ssh") {
        return Some(source("etc/ssh".into(), "ssh", None, None));
    }
    let parts: Vec<_> = rel.iter().collect();
    let (i, kind) = parts.iter().enumerate().skip(1).find_map(|(i, part)| {
        HOME_DIRS
            .iter()
            .find(|(_, dir)| part == dir)
            .map(|(kind, _)| (i, *kind))
    })?;
    let home: PathBuf = parts[..i].iter().collect();
    Some(source(home.join(parts[i]), kind, Some(home), user))
}

/// The sources of the backup in `data`: those of its manifest, and for any
/// entry outside them, as read from its path.
fn read_sources(data: &[u8]) -> Result<Vec<Source>> {
    let mut sources = Vec::new();
    let mut entries = Vec::new();
    let mut tar = tar::Archive::new(GzDecoder::new(data));
    for entry in tar.entries().context("reading the archive")? {
        let mut entry = entry.context("reading the archive")?;
        let rel = entry.path()?.into_owned();
        if rel == Path::new(MANIFEST) {
            let mut json = Vec::new();
            entry.read_to_end(&mut json)?;
            if let Ok(manifest) = serde_json::from_slice::<Manifest>(&json) {
                sources.extend(manifest.sources);
            }
        } else {
            let user = entry
                .header()
                .username()
                .ok()
                .flatten()
                .filter(|u| !u.is_empty())
                .map(str::to_string);
            entries.push((rel, user));
        }
    }
    for (rel, user) in entries {
        if source_of(&sources, &rel).is_none()
            && let Some(source) = infer(&rel, user)
        {
            sources.push(source);
        }
    }
    Ok(sources)
}

/// The current user here, whose home the backup goes into.
#[derive(Debug)]
struct Target {
    /// Their home, relative to `/`.
    dir: PathBuf,
    /// Their user and group, given to everything restored.
    owner: (u32, u32),
}

/// The home of the backup to restore: `me`'s own, else its only one. The
/// rest, from a backup that took more, are skipped with a note.
fn pick<'a>(sources: &'a [Source], me: &User, report: &mut Report) -> Option<&'a Path> {
    let mut homes: Vec<(&Path, Option<&str>)> = Vec::new();
    for source in sources {
        match &source.home {
            Some(home) if !homes.iter().any(|(h, _)| h == home) => {
                homes.push((home, source.user.as_deref()))
            }
            Some(_) => {}
            None => report.warnings.push(format!(
                "/{}: not in a home, skipped",
                source.path.display()
            )),
        }
    }
    let picked = match homes
        .iter()
        .find(|(_, user)| *user == Some(me.name.as_str()))
    {
        Some((home, _)) => Some(*home),
        None if homes.len() == 1 => Some(homes[0].0),
        None => None,
    };
    for (home, user) in homes {
        let user = user.unwrap_or("someone else");
        if Some(home) != picked {
            report
                .warnings
                .push(format!("/{}: {user}'s, skipped", home.display()));
        } else if user != me.name {
            report.warnings.push(format!(
                "/{}: {user}'s, restored to {} for {}",
                home.display(),
                me.home,
                me.name
            ));
        }
    }
    picked
}

/// Puts the secrets in `archive` back into the current user's home under
/// `root` (`/` normally). Existing files are only replaced with `force`. An
/// encrypted archive is opened with the password `password` asks for.
pub fn restore(
    archive: &Path,
    root: &Path,
    kinds: Kinds,
    force: bool,
    password: impl FnOnce() -> Result<Zeroizing<String>>,
) -> Result<Report> {
    let accounts = Accounts::read(root);
    let me = accounts.current(root)?;
    let mut data = Zeroizing::new(
        fs::read(archive).with_context(|| format!("reading {}", archive.display()))?,
    );
    if !data.starts_with(&[0x1f, 0x8b]) {
        let raw = crypto::dearmor(&data)
            .map_err(|_| anyhow::anyhow!("{} is not a uc secret backup", archive.display()))?;
        data = Zeroizing::new(crypto::decrypt_raw(&raw, password()?.as_bytes())?.0);
    }
    let sources = read_sources(&data)?;
    let mut report = Report::default();
    let Some(home) = pick(&sources, me, &mut report) else {
        return Ok(report);
    };

    let mut tar = tar::Archive::new(GzDecoder::new(&data[..]));
    tar.set_preserve_permissions(true);
    tar.set_preserve_mtime(true);
    tar.set_overwrite(true);
    tar.set_unpack_xattrs(false);

    let restore = Restore {
        root,
        sources: &sources,
        home,
        target: Target {
            dir: me.home().unwrap_or_default(),
            owner: (me.uid, me.gid),
        },
        kinds,
        force,
    };
    // Keys are written before their mode is set; keep them private meanwhile.
    // SAFETY: umask only changes this process's file creation mask.
    let umask = unsafe { libc::umask(0o077) };
    let result = restore.unpack(&mut tar, &mut report);
    unsafe { libc::umask(umask) };
    result.map(|()| report)
}

/// What a restore needs to put each entry in its place.
struct Restore<'a> {
    root: &'a Path,
    sources: &'a [Source],
    /// The home in the backup that is restored.
    home: &'a Path,
    /// Where it goes.
    target: Target,
    kinds: Kinds,
    force: bool,
}

impl Restore<'_> {
    fn unpack<R: io::Read>(&self, tar: &mut tar::Archive<R>, report: &mut Report) -> Result<()> {
        let (root, home, target) = (self.root, self.home, &self.target);
        let real_root =
            fs::canonicalize(root).with_context(|| format!("reading {}", root.display()))?;
        // Directories not put back, whose contents are skipped without a word.
        let mut skipped = BTreeSet::new();
        let mut chown_failed = false;
        for entry in tar.entries().context("reading the archive")? {
            let mut entry = entry.context("reading the archive")?;
            let rel = entry.path()?.into_owned();
            if rel == Path::new(MANIFEST) {
                continue;
            }
            if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
                report
                    .warnings
                    .push(format!("{}: unsafe path, skipped", rel.display()));
                continue;
            }
            let Some(source) = source_of(self.sources, &rel)
                .filter(|s| s.home.as_deref() == Some(home) && self.kinds.wants(&s.kind))
            else {
                continue;
            };
            let to = target.dir.join(rel.strip_prefix(home).unwrap_or(&rel));

            let dest = root.join(&to);
            let is_dir = entry.header().entry_type() == EntryType::Directory;
            let parent = dest.parent().unwrap_or(root);
            if !parent.is_dir() {
                self.make_dirs(parent, &mut chown_failed, report)?;
            }
            if !parent.is_dir() {
                if !skipped.contains(parent) {
                    report.warnings.push(format!(
                        "/{}: no {}, skipped",
                        to.display(),
                        shown(root, parent)
                    ));
                }
                if is_dir {
                    skipped.insert(dest);
                }
                continue;
            }
            if !fs::canonicalize(parent).is_ok_and(|p| p.starts_with(&real_root)) {
                report.warnings.push(format!(
                    "/{}: {} leads outside {}, skipped",
                    to.display(),
                    shown(root, parent),
                    root.display()
                ));
                continue;
            }
            let exists = fs::symlink_metadata(&dest).is_ok();
            if exists && !self.force {
                if !is_dir {
                    report.kept.push(to);
                }
                continue;
            }

            // An absolute symlink into the old home points into the new one.
            let link = match entry.header().entry_type() {
                EntryType::Symlink if target.dir != home => entry.link_name()?.and_then(|link| {
                    link.strip_prefix(Path::new("/").join(home))
                        .ok()
                        .map(|rest| Path::new("/").join(&target.dir).join(rest))
                }),
                _ => None,
            };
            match link {
                Some(link) => {
                    if exists {
                        fs::remove_file(&dest)
                            .with_context(|| format!("restoring /{}", to.display()))?;
                    }
                    std::os::unix::fs::symlink(&link, &dest)
                        .with_context(|| format!("restoring /{}", to.display()))?;
                }
                None => {
                    entry
                        .unpack(&dest)
                        .with_context(|| format!("restoring /{}", to.display()))?;
                }
            }
            chown(&dest, target.owner, &mut chown_failed, report);
            report.did(to, &source.kind);
        }
        Ok(())
    }

    /// Makes the directories missing up to `dir` in the home, if that is
    /// there: the parents of a source a newer uc took, such as `~/.config`
    /// for `~/.config/gh`.
    fn make_dirs(&self, dir: &Path, chown_failed: &mut bool, report: &mut Report) -> Result<()> {
        let home = self.root.join(&self.target.dir);
        if !home.is_dir() || !dir.starts_with(&home) {
            return Ok(());
        }
        let mut missing: Vec<_> = dir.ancestors().take_while(|d| !d.exists()).collect();
        missing.reverse();
        for dir in missing {
            fs::create_dir(dir).with_context(|| format!("making {}", shown(self.root, dir)))?;
            chown(dir, self.target.owner, chown_failed, report);
        }
        Ok(())
    }
}

/// Gives `path` the user and group `owner`, warning once if it can't.
fn chown(path: &Path, owner: (u32, u32), failed: &mut bool, report: &mut Report) {
    if std::os::unix::fs::lchown(path, Some(owner.0), Some(owner.1)).is_err() && !*failed {
        *failed = true;
        report
            .warnings
            .push("could not set the owner of some files".to_string());
    }
}

/// `path` as it is on the restored system.
fn shown(root: &Path, path: &Path) -> String {
    format!("/{}", path.strip_prefix(root).unwrap_or(path).display())
}

/// How many entries of each kind a report covers, as
/// `ssh: N, gpg: M, kube: K`, followed by any other kinds.
pub fn counts(report: &Report) -> String {
    let mut shown: Vec<String> = ["ssh", "gpg", "kube"]
        .iter()
        .map(|kind| format!("{kind}: {}", report.kinds.get(*kind).unwrap_or(&0)))
        .collect();
    shown.extend(
        report
            .kinds
            .iter()
            .filter(|(kind, _)| !["ssh", "gpg", "kube"].contains(&kind.as_str()))
            .map(|(kind, n)| format!("{kind}: {n}")),
    );
    shown.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    const ALL: Kinds = Kinds {
        ssh: true,
        gpg: true,
        kube: true,
    };

    const SSH_ONLY: Kinds = Kinds {
        ssh: true,
        gpg: false,
        kube: false,
    };

    fn no_password() -> Result<Zeroizing<String>> {
        panic!("asked for a password")
    }

    fn me() -> (u32, u32) {
        // SAFETY: getuid and getgid can't fail.
        unsafe { (libc::getuid(), libc::getgid()) }
    }

    /// A system at `root` whose `users` all have a home, the first with the
    /// uid running the tests.
    fn system_of(root: &Path, users: &[&str]) {
        let (uid, gid) = me();
        fs::create_dir_all(root.join("etc/ssh")).unwrap();
        let mut passwd = String::new();
        for (i, user) in users.iter().enumerate() {
            let uid = uid + i as u32 * 1000;
            passwd += &format!("{user}:x:{uid}:{gid}::/home/{user}:/bin/sh\n");
            fs::create_dir_all(root.join(format!("home/{user}"))).unwrap();
        }
        passwd += "nobody:x:65534:65534::/nonexistent:/bin/false\n";
        fs::write(root.join("etc/passwd"), passwd).unwrap();
        fs::write(root.join("etc/group"), format!("{}:x:{gid}:\n", users[0])).unwrap();
    }

    fn system(root: &Path, user: &str) {
        system_of(root, &[user]);
    }

    fn write(path: &Path, body: &str, mode: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn mode(path: &Path) -> u32 {
        fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777
    }

    /// Rewrites the plain backup `out`, passing each entry through `edit`,
    /// which can drop it, then appends the files in `extra`.
    fn rewrite(out: &Path, edit: impl Fn(&Path) -> bool, extra: &[(&str, &[u8])]) {
        let data = fs::read(out).unwrap();
        let mut old = tar::Archive::new(GzDecoder::new(&data[..]));
        let mut new = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        for entry in old.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().into_owned();
            if edit(&path) {
                let mut header = entry.header().clone();
                new.append_data(&mut header, &path, &mut entry).unwrap();
            }
        }
        for (path, body) in extra {
            let mut header = Header::new_gnu();
            header.set_mode(0o600);
            header.set_size(body.len() as u64);
            new.append_data(&mut header, path, *body).unwrap();
        }
        fs::write(out, new.into_inner().unwrap().finish().unwrap()).unwrap();
    }

    #[test]
    fn keys_go_back_with_their_modes() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("src"), tmp.path().join("dst"));
        system(&src, "alice");
        write(&src.join("etc/ssh/ssh_host_ed25519_key"), "host", 0o600);
        write(&src.join("etc/ssh/sshd_config"), "config", 0o644);
        let ssh = src.join("home/alice/.ssh");
        write(&ssh.join("id_ed25519"), "private", 0o600);
        write(&ssh.join("id_ed25519.pub"), "public", 0o644);
        fs::set_permissions(&ssh, fs::Permissions::from_mode(0o700)).unwrap();
        symlink("id_ed25519.pub", ssh.join("link")).unwrap();
        let gnupg = src.join("home/alice/.gnupg");
        write(&gnupg.join("private-keys-v1.d/ABC.key"), "gpg", 0o600);
        write(&gnupg.join("public-keys.d/pubring.db.lock"), "", 0o600);
        let _socket = std::os::unix::net::UnixListener::bind(gnupg.join("S.gpg-agent")).unwrap();
        let kube = src.join("home/alice/.kube");
        write(&kube.join("config"), "kubeconfig", 0o600);
        write(&kube.join("cache/discovery/x.json"), "{}", 0o644);
        write(&kube.join("http-cache/y"), "", 0o644);

        let out = tmp.path().join("b.tar.gz");
        let report = backup(&src, ALL, &out, None).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(mode(&out), 0o600);
        let taken: Vec<_> = report.done.iter().map(|p| p.to_str().unwrap()).collect();
        assert!(!taken.iter().any(|p| p.starts_with("etc")));
        assert!(
            !taken
                .iter()
                .any(|p| p.ends_with("S.gpg-agent") || p.ends_with(".lock"))
        );
        assert!(taken.contains(&"home/alice/.kube/config"));
        assert!(!taken.iter().any(|p| p.contains("cache")));

        system(&dst, "alice");
        let report = restore(&out, &dst, ALL, false, no_password).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert!(!dst.join(MANIFEST).exists());
        let ssh = dst.join("home/alice/.ssh");
        assert_eq!(
            fs::read_to_string(ssh.join("id_ed25519")).unwrap(),
            "private"
        );
        assert_eq!(mode(&ssh.join("id_ed25519")), 0o600);
        assert_eq!(mode(&ssh.join("id_ed25519.pub")), 0o644);
        assert_eq!(mode(&ssh), 0o700);
        assert_eq!(
            fs::read_link(ssh.join("link")).unwrap(),
            Path::new("id_ed25519.pub")
        );
        assert_eq!(fs::metadata(ssh.join("id_ed25519")).unwrap().uid(), me().0);
        assert_eq!(
            fs::read_to_string(dst.join("home/alice/.gnupg/private-keys-v1.d/ABC.key")).unwrap(),
            "gpg"
        );
        assert!(!dst.join("etc/ssh/ssh_host_ed25519_key").exists());
        let config = dst.join("home/alice/.kube/config");
        assert_eq!(fs::read_to_string(&config).unwrap(), "kubeconfig");
        assert_eq!(mode(&config), 0o600);
    }

    #[test]
    fn existing_files_are_kept_unless_forced() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        system(&root, "bob");
        let key = root.join("home/bob/.ssh/id_rsa");
        write(&key, "old", 0o600);
        let out = tmp.path().join("b.tar.gz");
        backup(&root, ALL, &out, None).unwrap();

        write(&key, "new", 0o644);
        let report = restore(&out, &root, ALL, false, no_password).unwrap();
        assert_eq!(report.kept, [PathBuf::from("home/bob/.ssh/id_rsa")]);
        assert_eq!(fs::read_to_string(&key).unwrap(), "new");

        restore(&out, &root, ALL, true, no_password).unwrap();
        assert_eq!(fs::read_to_string(&key).unwrap(), "old");
        assert_eq!(mode(&key), 0o600);
    }

    #[test]
    fn it_goes_to_the_current_user_whatever_their_name() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("src"), tmp.path().join("dst"));
        system(&src, "carol");
        write(&src.join("home/carol/.ssh/id_ed25519"), "k", 0o600);
        symlink(
            "/home/carol/.ssh/id_ed25519",
            src.join("home/carol/.ssh/abs"),
        )
        .unwrap();
        write(&src.join("home/carol/.gnupg/trustdb.gpg"), "t", 0o600);
        let out = tmp.path().join("b.tar.gz");
        let report = backup(&src, SSH_ONLY, &out, None).unwrap();
        assert!(
            report
                .done
                .iter()
                .all(|p| !p.starts_with("home/carol/.gnupg"))
        );

        system(&dst, "dave");
        let report = restore(&out, &dst, ALL, false, no_password).unwrap();
        assert_eq!(
            report.warnings,
            ["/home/carol: carol's, restored to /home/dave for dave"]
        );
        let key = dst.join("home/dave/.ssh/id_ed25519");
        assert_eq!(fs::read_to_string(&key).unwrap(), "k");
        assert_eq!(fs::metadata(&key).unwrap().uid(), me().0);
        assert_eq!(
            fs::read_link(dst.join("home/dave/.ssh/abs")).unwrap(),
            Path::new("/home/dave/.ssh/id_ed25519")
        );
        assert!(!dst.join("home/carol").exists());
        assert_eq!(counts(&report), "ssh: 3, gpg: 0, kube: 0");
    }

    #[test]
    fn only_the_current_user_is_taken() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        system_of(&root, &["carol", "frank"]);
        write(&root.join("home/carol/.ssh/id_ed25519"), "c", 0o600);
        write(&root.join("home/frank/.ssh/id_ed25519"), "f", 0o600);
        let out = tmp.path().join("b.tar.gz");
        let report = backup(&root, ALL, &out, None).unwrap();
        assert_eq!(
            report.done,
            [
                PathBuf::from("home/carol/.ssh"),
                PathBuf::from("home/carol/.ssh/id_ed25519")
            ]
        );
    }

    #[test]
    fn a_backup_of_several_users_gives_back_the_current_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("src"), tmp.path().join("dst"));
        system(&src, "carol");
        write(&src.join("home/carol/.ssh/id_ed25519"), "c", 0o600);
        let out = tmp.path().join("b.tar.gz");
        backup(&src, ALL, &out, None).unwrap();
        // As an older uc wrote them: everyone's, host keys and no manifest.
        rewrite(
            &out,
            |p| p != Path::new(MANIFEST),
            &[
                ("home/frank/.ssh/id_ed25519", b"f"),
                ("etc/ssh/ssh_host_ed25519_key", b"h"),
            ],
        );

        system_of(&dst, &["carol", "frank"]);
        let report = restore(&out, &dst, ALL, false, no_password).unwrap();
        assert_eq!(
            fs::read_to_string(dst.join("home/carol/.ssh/id_ed25519")).unwrap(),
            "c"
        );
        assert!(!dst.join("home/frank/.ssh").exists());
        assert!(!dst.join("etc/ssh/ssh_host_ed25519_key").exists());
        assert_eq!(
            report.warnings,
            [
                "/etc/ssh: not in a home, skipped",
                "/home/frank: someone else's, skipped"
            ]
        );
    }

    #[test]
    fn a_backup_without_a_manifest_still_restores() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("src"), tmp.path().join("dst"));
        system(&src, "carol");
        write(&src.join("home/carol/.ssh/id_ed25519"), "k", 0o600);
        write(&src.join("home/carol/.kube/config"), "kc", 0o600);
        let out = tmp.path().join("b.tar.gz");
        backup(&src, ALL, &out, None).unwrap();
        rewrite(&out, |p| p != Path::new(MANIFEST), &[]);

        system(&dst, "dave");
        let report = restore(&out, &dst, SSH_ONLY, false, no_password).unwrap();
        assert_eq!(
            fs::read_to_string(dst.join("home/dave/.ssh/id_ed25519")).unwrap(),
            "k"
        );
        assert!(!dst.join("home/dave/.kube").exists());
        assert_eq!(counts(&report), "ssh: 2, gpg: 0, kube: 0");
    }

    #[test]
    fn kinds_from_a_newer_uc_are_restored() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("src"), tmp.path().join("dst"));
        system(&src, "carol");
        write(&src.join("home/carol/.ssh/id_ed25519"), "k", 0o600);
        let out = tmp.path().join("b.tar.gz");
        backup(&src, ALL, &out, None).unwrap();
        let manifest = br#"{
            "version": 7,
            "made_with": "a newer uc",
            "sources": [
                {"path": "home/carol/.ssh", "kind": "ssh", "home": "home/carol", "user": "carol"},
                {"path": "home/carol/.docker", "kind": "docker", "home": "home/carol",
                 "user": "carol", "encrypted": false}
            ]
        }"#;
        rewrite(
            &out,
            |p| p != Path::new(MANIFEST),
            &[
                (MANIFEST, manifest),
                ("home/carol/.docker/config.json", b"{}"),
            ],
        );

        system(&dst, "dave");
        let report = restore(&out, &dst, SSH_ONLY, false, no_password).unwrap();
        assert_eq!(
            fs::read_to_string(dst.join("home/dave/.docker/config.json")).unwrap(),
            "{}"
        );
        assert_eq!(counts(&report), "ssh: 2, gpg: 0, kube: 0, docker: 1");
        assert_eq!(
            fs::read_to_string(dst.join("home/dave/.ssh/id_ed25519")).unwrap(),
            "k"
        );
    }

    #[test]
    fn an_encrypted_backup_needs_its_password() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        system(&root, "erin");
        let key = root.join("home/erin/.ssh/id_ed25519");
        write(&key, "private", 0o600);
        let out = tmp.path().join("b.tar.gz.asc");
        backup(&root, ALL, &out, Some(b"hunter2")).unwrap();
        assert!(!fs::read_to_string(&out).unwrap().contains("private"));
        assert_eq!(mode(&out), 0o600);

        fs::remove_file(&key).unwrap();
        let wrong = || Ok(Zeroizing::new("nope".to_string()));
        assert!(restore(&out, &root, ALL, false, wrong).is_err());
        assert!(!key.exists());
        let right = || Ok(Zeroizing::new("hunter2".to_string()));
        restore(&out, &root, ALL, false, right).unwrap();
        assert_eq!(fs::read_to_string(&key).unwrap(), "private");
    }
}
