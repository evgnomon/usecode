// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use std::env;
use std::path::PathBuf;

/// Well-known locations derived from HOME, USER and PLAT_HOME.
pub struct Paths {
    /// `~/src/github.com/$USER/usecode`
    pub root: PathBuf,
    /// `$PLAT_HOME` or `~/.plat`
    pub plat_home: PathBuf,
    /// `$ROOT/lib/templates/lib`
    pub templates: PathBuf,
}

impl Paths {
    pub fn from_env() -> Self {
        let home = PathBuf::from(env::var_os("HOME").unwrap_or_default());
        let user = env::var("USER").unwrap_or_default();
        let root = home
            .join("src")
            .join("github.com")
            .join(user)
            .join("usecode");
        let plat_home = env::var_os("PLAT_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".plat"));
        let templates = root.join("lib").join("templates").join("lib");
        Self {
            root,
            plat_home,
            templates,
        }
    }
}
