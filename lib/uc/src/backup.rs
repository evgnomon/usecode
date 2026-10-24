// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The SSH, GPG and kubeconfig backup behind `uc secret backup` and
//! `uc secret restore`.
//!
//! A backup is a `.tar.gz` of every user's `~/.ssh`, `~/.gnupg` and `~/.kube`
//! (users and homes from `/etc/passwd`) and the SSH host keys in `/etc/ssh`,
//! with paths relative to `/`, each entry carrying its mode, owner and group
//! by number and by name. Sockets, lock files and kubectl's caches are left
//! out, and symlinks are kept as symlinks, except for a `~/.ssh`, `~/.gnupg`
//! or `~/.kube` that is itself one, whose target's contents are taken.
//!
//! With a password, the archive is encrypted the way `uc secret encrypt`
//! does it, to `<name>.tar.gz.asc`, and opens with `uc secret decrypt` too.
//!
//! A restore puts the entries back under `/` with the same mode, and with the
//! owner and group of the same name there, falling back to the number. An
//! entry whose parent directory is missing (a user without a home here) is
//! skipped, and existing files are kept unless asked to replace them.

use crate::crypto;
use anyhow::{Context, Result, bail};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Component, Path, PathBuf};
use tar::{EntryType, Header, HeaderMode};
use zeroize::Zeroizing;

/// The kinds of secrets to take or put back.
#[derive(Debug, Clone, Copy)]
pub struct Kinds {
    pub ssh: bool,
    pub gpg: bool,
    pub kube: bool,
}

impl Kinds {
    /// Whether the archive path `rel` (relative to `/`) is of a kind asked for.
    fn wants(&self, rel: &Path) -> bool {
        let parts: Vec<_> = rel.components().map(|c| c.as_os_str()).collect();
        let ssh = parts.starts_with(&["etc".as_ref(), "ssh".as_ref()])
            || parts.iter().any(|p| *p == ".ssh");
        let gpg = parts.iter().any(|p| *p == ".gnupg");
        let kube = parts.iter().any(|p| *p == ".kube");
        (ssh && self.ssh) || (gpg && self.gpg) || (kube && self.kube)
    }
}

/// What a backup or restore did, for the caller to print.
#[derive(Debug, Default)]
pub struct Report {
    /// Files, directories and symlinks taken or put back, relative to `/`.
    pub done: Vec<PathBuf>,
    /// Existing files left as they were.
    pub kept: Vec<PathBuf>,
    /// Anything skipped, and why.
    pub warnings: Vec<String>,
}

/// User and group names and ids, read from a system's `/etc/passwd` and
/// `/etc/group`.
#[derive(Debug, Default)]
struct Accounts {
    users: Vec<(String, u32)>,
    groups: Vec<(String, u32)>,
    homes: Vec<String>,
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
                accounts.users.push((f[0].clone(), uid));
            }
            if let Some(home) = f.get(5).filter(|h| h.starts_with('/'))
                && !accounts.homes.contains(home)
            {
                accounts.homes.push(home.clone());
            }
        }
        for f in lines("etc/group") {
            if let Ok(gid) = f[2].parse() {
                accounts.groups.push((f[0].clone(), gid));
            }
        }
        accounts
    }

    fn user(&self, uid: u32) -> Option<&str> {
        self.users.iter().find(|u| u.1 == uid).map(|u| u.0.as_str())
    }

    fn group(&self, gid: u32) -> Option<&str> {
        self.groups
            .iter()
            .find(|g| g.1 == gid)
            .map(|g| g.0.as_str())
    }

    fn uid(&self, name: &str) -> Option<u32> {
        self.users.iter().find(|u| u.0 == name).map(|u| u.1)
    }

    fn gid(&self, name: &str) -> Option<u32> {
        self.groups.iter().find(|g| g.0 == name).map(|g| g.1)
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

/// The top-level paths to take from the system at `root`.
fn sources(root: &Path, accounts: &Accounts, kinds: Kinds) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if kinds.ssh
        && let Ok(dir) = fs::read_dir(root.join("etc/ssh"))
    {
        let mut keys: Vec<_> = dir
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("ssh_host_"))
            .map(|e| e.path())
            .collect();
        keys.sort();
        paths.extend(keys);
    }
    for home in &accounts.homes {
        let home = root.join(home.trim_start_matches('/'));
        for (want, dir) in [
            (kinds.ssh, ".ssh"),
            (kinds.gpg, ".gnupg"),
            (kinds.kube, ".kube"),
        ] {
            let path = home.join(dir);
            if want && !paths.contains(&path) && fs::metadata(&path).is_ok_and(|m| m.is_dir()) {
                paths.push(path);
            }
        }
    }
    paths
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
    let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    let mut report = Report::default();
    for top in sources(root, &accounts, kinds) {
        add(&mut tar, root, &top, true, &accounts, &mut report)?;
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
        report.done.push(rel.to_path_buf());
        let mut children: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        children.sort();
        for child in children {
            add(tar, root, &child, false, accounts, report)?;
        }
    } else if meta.file_type().is_symlink() {
        let target = fs::read_link(path)?;
        tar.append_link(&mut header, rel, target)?;
        report.done.push(rel.to_path_buf());
    } else {
        match File::open(path) {
            Ok(file) => {
                tar.append_data(&mut header, rel, file)?;
                report.done.push(rel.to_path_buf());
            }
            Err(err) => report.warnings.push(format!("/{}: {err}", rel.display())),
        }
    }
    Ok(())
}

/// Puts the secrets in `archive` back under `root` (`/` normally). Existing
/// files are only replaced with `force`. An encrypted archive is opened
/// with the password `password` asks for.
pub fn restore(
    archive: &Path,
    root: &Path,
    kinds: Kinds,
    force: bool,
    password: impl FnOnce() -> Result<Zeroizing<String>>,
) -> Result<Report> {
    let accounts = Accounts::read(root);
    let mut data = Zeroizing::new(
        fs::read(archive).with_context(|| format!("reading {}", archive.display()))?,
    );
    if !data.starts_with(&[0x1f, 0x8b]) {
        let raw = crypto::dearmor(&data)
            .map_err(|_| anyhow::anyhow!("{} is not a uc secret backup", archive.display()))?;
        data = Zeroizing::new(crypto::decrypt_raw(&raw, password()?.as_bytes())?.0);
    }
    let mut tar = tar::Archive::new(GzDecoder::new(&data[..]));
    tar.set_preserve_permissions(true);
    tar.set_preserve_mtime(true);
    tar.set_overwrite(true);
    tar.set_unpack_xattrs(false);

    // Keys are written before their mode is set; keep them private meanwhile.
    // SAFETY: umask only changes this process's file creation mask.
    let umask = unsafe { libc::umask(0o077) };
    let result = unpack(&mut tar, root, &accounts, kinds, force);
    unsafe { libc::umask(umask) };
    result
}

fn unpack<R: io::Read>(
    tar: &mut tar::Archive<R>,
    root: &Path,
    accounts: &Accounts,
    kinds: Kinds,
    force: bool,
) -> Result<Report> {
    let mut report = Report::default();
    // Directories not put back, whose contents are skipped without a word.
    let mut skipped = BTreeSet::new();
    let mut chown_failed = false;
    for entry in tar.entries().context("reading the archive")? {
        let mut entry = entry.context("reading the archive")?;
        let rel = entry.path()?.into_owned();
        if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
            report
                .warnings
                .push(format!("{}: unsafe path, skipped", rel.display()));
            continue;
        }
        if !kinds.wants(&rel) {
            continue;
        }
        let dest = root.join(&rel);
        let is_dir = entry.header().entry_type() == EntryType::Directory;
        let parent = dest.parent().unwrap_or(root);
        if !parent.is_dir() {
            if !skipped.contains(parent) {
                report.warnings.push(format!(
                    "/{}: no {}, skipped",
                    rel.display(),
                    shown(root, parent)
                ));
            }
            if is_dir {
                skipped.insert(dest);
            }
            continue;
        }
        let exists = fs::symlink_metadata(&dest).is_ok();
        if exists && !force {
            if !is_dir {
                report.kept.push(rel);
            }
            continue;
        }

        let header = entry.header();
        let uid = owner(header.username(), header.uid(), |n| accounts.uid(n));
        let gid = owner(header.groupname(), header.gid(), |n| accounts.gid(n));
        if !entry
            .unpack_in(root)
            .with_context(|| format!("restoring /{}", rel.display()))?
        {
            report
                .warnings
                .push(format!("{}: unsafe path, skipped", rel.display()));
            continue;
        }
        if let (Some(uid), Some(gid)) = (uid, gid)
            && std::os::unix::fs::lchown(&dest, Some(uid), Some(gid)).is_err()
            && !chown_failed
        {
            chown_failed = true;
            report.warnings.push(
                "could not set the owner of some files; run as root to restore other users'"
                    .to_string(),
            );
        }
        report.done.push(rel);
    }
    Ok(report)
}

/// The id of the account named in the archive if there is one here, or the
/// archived id.
fn owner(
    name: Result<Option<&str>, std::str::Utf8Error>,
    id: io::Result<u64>,
    lookup: impl Fn(&str) -> Option<u32>,
) -> Option<u32> {
    name.ok()
        .flatten()
        .and_then(lookup)
        .or_else(|| id.ok().and_then(|id| u32::try_from(id).ok()))
}

/// `path` as it is on the restored system.
fn shown(root: &Path, path: &Path) -> String {
    format!("/{}", path.strip_prefix(root).unwrap_or(path).display())
}

/// How many entries of each kind a report covers, as
/// `ssh: N, gpg: M, kube: K`.
pub fn counts(paths: &[PathBuf]) -> String {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for path in paths {
        let has = |dir: &str| path.components().any(|c| c.as_os_str() == dir);
        let kind = if has(".gnupg") {
            "gpg"
        } else if has(".kube") {
            "kube"
        } else {
            "ssh"
        };
        *counts.entry(kind).or_default() += 1;
    }
    let count = |kind| counts.get(kind).unwrap_or(&0);
    format!(
        "ssh: {}, gpg: {}, kube: {}",
        count("ssh"),
        count("gpg"),
        count("kube")
    )
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

    fn no_password() -> Result<Zeroizing<String>> {
        panic!("asked for a password")
    }

    fn me() -> (u32, u32) {
        // SAFETY: getuid and getgid can't fail.
        unsafe { (libc::getuid(), libc::getgid()) }
    }

    fn system(root: &Path, user: &str) {
        let (uid, gid) = me();
        fs::create_dir_all(root.join("etc/ssh")).unwrap();
        fs::write(
            root.join("etc/passwd"),
            format!("{user}:x:{uid}:{gid}::/home/{user}:/bin/sh\nnobody:x:65534:65534::/nonexistent:/bin/false\n"),
        )
        .unwrap();
        fs::write(root.join("etc/group"), format!("{user}:x:{gid}:\n")).unwrap();
        fs::create_dir_all(root.join(format!("home/{user}"))).unwrap();
    }

    fn write(path: &Path, body: &str, mode: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn mode(path: &Path) -> u32 {
        fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777
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
        assert!(taken.contains(&"etc/ssh/ssh_host_ed25519_key"));
        assert!(!taken.contains(&"etc/ssh/sshd_config"));
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
        assert_eq!(mode(&dst.join("etc/ssh/ssh_host_ed25519_key")), 0o600);
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
    fn kinds_and_missing_homes_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("src"), tmp.path().join("dst"));
        system(&src, "carol");
        write(&src.join("home/carol/.ssh/id_ed25519"), "k", 0o600);
        write(&src.join("home/carol/.gnupg/trustdb.gpg"), "t", 0o600);
        let out = tmp.path().join("b.tar.gz");
        let ssh_only = Kinds {
            ssh: true,
            gpg: false,
            kube: false,
        };
        let report = backup(&src, ssh_only, &out, None).unwrap();
        assert!(
            report
                .done
                .iter()
                .all(|p| !p.starts_with("home/carol/.gnupg"))
        );

        system(&dst, "dave");
        let report = restore(&out, &dst, ALL, false, no_password).unwrap();
        assert!(report.done.is_empty());
        assert_eq!(
            report.warnings,
            ["/home/carol/.ssh: no /home/carol, skipped"]
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
