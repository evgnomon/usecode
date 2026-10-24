// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The roles' files and templates. They are built into uc from `lib/uc/roles`,
//! so a machine with just the binary or the .deb can sync; `--roles DIR`
//! reads them from a directory instead, to try changes to them without a
//! rebuild. Paths are relative to the roles directory, e.g.
//! `dotfiles/files/.bashrc`.

use anyhow::{Result, bail};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/roles.rs"));

pub enum Assets {
    /// The files built into uc.
    Embedded,
    /// A roles directory on disk, given with `--roles`.
    Dir(PathBuf),
}

impl Assets {
    /// The content of a file.
    pub fn read(&self, rel: &Path) -> Result<Cow<'static, [u8]>> {
        match self {
            Assets::Embedded => match embedded(rel) {
                Some(bytes) => Ok(Cow::Borrowed(bytes)),
                None => bail!("{} is not built into uc", rel.display()),
            },
            Assets::Dir(dir) => {
                let path = dir.join(rel);
                match std::fs::read(&path) {
                    Ok(bytes) => Ok(Cow::Owned(bytes)),
                    Err(err) => bail!("reading {}: {err}", path.display()),
                }
            }
        }
    }

    pub fn read_string(&self, rel: &Path) -> Result<String> {
        match String::from_utf8(self.read(rel)?.into_owned()) {
            Ok(text) => Ok(text),
            Err(_) => bail!("{} is not UTF-8", rel.display()),
        }
    }

    /// Whether `rel` is a file.
    pub fn is_file(&self, rel: &Path) -> bool {
        match self {
            Assets::Embedded => embedded(rel).is_some(),
            Assets::Dir(dir) => dir.join(rel).is_file(),
        }
    }

    /// Whether `rel` has any files under it.
    pub fn is_dir(&self, rel: &Path) -> bool {
        match self {
            Assets::Embedded => ROLES.iter().any(|(path, _)| {
                Path::new(path)
                    .strip_prefix(rel)
                    .is_ok_and(|r| r != Path::new(""))
            }),
            Assets::Dir(dir) => dir.join(rel).is_dir(),
        }
    }

    /// Every file under `rel`, as paths relative to it, sorted.
    pub fn walk(&self, rel: &Path) -> Result<Vec<PathBuf>> {
        match self {
            Assets::Embedded => {
                let mut files: Vec<PathBuf> = ROLES
                    .iter()
                    .filter_map(|(path, _)| Path::new(path).strip_prefix(rel).ok())
                    .filter(|r| *r != Path::new(""))
                    .map(Path::to_path_buf)
                    .collect();
                if files.is_empty() {
                    bail!("{} is not built into uc", rel.display());
                }
                files.sort();
                Ok(files)
            }
            Assets::Dir(dir) => crate::configure::modules::walk_files(&dir.join(rel)),
        }
    }
}

fn embedded(rel: &Path) -> Option<&'static [u8]> {
    ROLES
        .iter()
        .find(|(path, _)| Path::new(path) == rel)
        .map(|(_, bytes)| *bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roles_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("roles")
    }

    #[test]
    fn the_built_in_roles_match_the_directory() {
        let dir = Assets::Dir(roles_dir());
        let built_in = Assets::Embedded;
        let root = Path::new("dotfiles");
        let files = dir.walk(root).unwrap();
        assert_eq!(built_in.walk(root).unwrap(), files);
        for file in &files {
            let rel = root.join(file);
            assert_eq!(built_in.read(&rel).unwrap(), dir.read(&rel).unwrap());
        }
        assert!(built_in.is_file(Path::new("dotfiles/files/.bashrc")));
        assert!(!built_in.is_file(Path::new("dotfiles/files")));
        assert!(built_in.is_dir(Path::new("dotfiles/files")));
        assert!(!built_in.is_dir(Path::new("dotfiles/files/.bashrc")));
        assert!(!built_in.is_dir(Path::new("nope")));
        assert!(built_in.read(Path::new("nope")).is_err());
    }
}
