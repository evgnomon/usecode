// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Ansible's `copy` and `template` modules: put a file, literal content or
//! a rendered Jinja template at a destination, changing it only when its
//! content or mode differs.

use crate::configure::ctx::Ctx;
use crate::configure::engine::Outcome;
use crate::configure::modules::file;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Where the content comes from. `File` and `Template` are paths among the
/// roles' files ([`Vars::assets`](crate::configure::vars::Vars::assets)).
pub enum Source<'a> {
    File(&'a Path),
    Content(&'a str),
    Template(&'a Path),
}

pub struct Copy<'a> {
    pub source: Source<'a>,
    pub dest: &'a Path,
    pub mode: Option<u32>,
    pub sudo: bool,
    /// Keep the previous version as `<dest>.<unix time>~`, like `backup: true`.
    pub backup: bool,
}

impl<'a> Copy<'a> {
    pub fn new(source: Source<'a>, dest: &'a Path) -> Copy<'a> {
        Copy {
            source,
            dest,
            mode: None,
            sudo: false,
            backup: false,
        }
    }

    pub fn mode(mut self, mode: u32) -> Copy<'a> {
        self.mode = Some(mode);
        self
    }

    pub fn mode_opt(mut self, mode: Option<u32>) -> Copy<'a> {
        self.mode = mode;
        self
    }

    pub fn sudo(mut self) -> Copy<'a> {
        self.sudo = true;
        self
    }

    pub fn sudo_if(mut self, sudo: bool) -> Copy<'a> {
        self.sudo = sudo;
        self
    }

    pub fn backup(mut self) -> Copy<'a> {
        self.backup = true;
        self
    }

    pub async fn run(self, ctx: &Ctx) -> Result<Outcome> {
        let role_file = matches!(self.source, Source::File(_) | Source::Template(_));
        let content = match self.source {
            Source::Content(text) => text.as_bytes().to_vec(),
            Source::File(src) => ctx.vars().assets.read(src)?.into_owned(),
            Source::Template(src) => {
                let text = ctx.vars().assets.read_string(src)?;
                ctx.render_named(&src.display().to_string(), &text)?
                    .into_bytes()
            }
        };
        if self.backup
            && !ctx.check()
            && let Some(old) = file::read(ctx, self.dest, self.sudo).await?
            && old != content
        {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let mut name = self.dest.as_os_str().to_os_string();
            name.push(format!(".{stamp}~"));
            file::write(ctx, &PathBuf::from(name), &old, None, self.sudo).await?;
        }
        // Earlier syncs linked role files into the usecode checkout; a copy
        // replaces such a link rather than writing through it.
        let linked = role_file
            && matches!(
                file::stat(ctx, self.dest, self.sudo).await?,
                Some(file::Stat {
                    kind: file::Kind::Link(_),
                    ..
                })
            );
        if linked {
            if ctx.check() {
                return Ok(Outcome::Changed);
            }
            file::absent(ctx, self.dest, self.sudo).await?;
        }
        let written = file::write(ctx, self.dest, &content, self.mode, self.sudo).await?;
        Ok(if linked { Outcome::Changed } else { written })
    }
}

/// `copy: src=… dest=…`, `src` among the roles' files.
pub async fn copy(
    ctx: &Ctx,
    src: &Path,
    dest: &Path,
    mode: Option<u32>,
    sudo: bool,
) -> Result<Outcome> {
    Copy::new(Source::File(src), dest)
        .mode_opt(mode)
        .sudo_if(sudo)
        .run(ctx)
        .await
}

/// `copy: content=… dest=…`.
pub async fn content(
    ctx: &Ctx,
    text: &str,
    dest: &Path,
    mode: Option<u32>,
    sudo: bool,
) -> Result<Outcome> {
    Copy::new(Source::Content(text), dest)
        .mode_opt(mode)
        .sudo_if(sudo)
        .run(ctx)
        .await
}

/// `template: src=… dest=…`, `src` among the roles' files.
pub async fn template(
    ctx: &Ctx,
    src: &Path,
    dest: &Path,
    mode: Option<u32>,
    sudo: bool,
) -> Result<Outcome> {
    Copy::new(Source::Template(src), dest)
        .mode_opt(mode)
        .sudo_if(sudo)
        .run(ctx)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure::ctx::tests::ctx;
    use crate::configure::modules::file::scratch;

    #[tokio::test]
    async fn renders_ansible_style_templates() {
        let dir = scratch("template");
        let src = dir.join("t.j2");
        std::fs::write(
            &src,
            "{#\nheader\n-#}\nrel={{ ansible_facts['distribution_release'] }}\n\
             {% if wsl2_kernel | default(false) %}\nwsl\n{% endif %}\n\
             key={{ gpg.get('key', '') if gpg is defined else 'none' }}\n\
             arch={{ {'x86_64': 'amd64'}[ansible_facts['architecture']] | default('') }}\n",
        )
        .unwrap();
        let dest = dir.join("out");
        let c = ctx(false);
        assert_eq!(
            template(&c, &src, &dest, None, false).await.unwrap(),
            Outcome::Changed
        );
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "rel=trixie\nkey=none\narch=amd64\n"
        );
        assert_eq!(
            template(&c, &src, &dest, None, false).await.unwrap(),
            Outcome::Ok
        );
    }

    #[tokio::test]
    async fn replaces_a_link_with_a_copy() {
        let dir = scratch("unlink");
        let src = dir.join("src");
        std::fs::write(&src, "same").unwrap();
        let dest = dir.join("dest");
        std::os::unix::fs::symlink(&src, &dest).unwrap();
        let c = ctx(false);
        assert_eq!(
            copy(&c, &src, &dest, None, false).await.unwrap(),
            Outcome::Changed
        );
        assert!(!dest.is_symlink());
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "same");
        assert_eq!(std::fs::read_to_string(&src).unwrap(), "same");
        assert_eq!(
            copy(&c, &src, &dest, None, false).await.unwrap(),
            Outcome::Ok
        );
    }

    #[tokio::test]
    async fn backs_up_replaced_content() {
        let dir = scratch("backup");
        let dest = dir.join("conf");
        std::fs::write(&dest, "old").unwrap();
        let c = ctx(false);
        Copy::new(Source::Content("new"), &dest)
            .backup()
            .run(&c)
            .await
            .unwrap();
        let backups = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with('~'))
            .count();
        assert_eq!(backups, 1);
    }
}
