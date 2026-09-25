// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The port of the `inflate` role: mirror a template tree onto a target
//! directory. `.j2` files are rendered without the suffix, everything else is
//! copied, files in a `bin` directory are made executable, and trees landing
//! in `/etc` or `/root` are written through sudo.

use crate::configure::ctx::Ctx;
use crate::configure::engine::Outcome;
use crate::configure::modules::{copy, walk_files};
use anyhow::Result;
use std::path::Path;

/// Inflates every file under `src` whose relative path passes `filter`.
pub async fn inflate(
    ctx: &Ctx,
    src: &Path,
    target: &Path,
    filter: impl Fn(&Path) -> bool,
) -> Result<Outcome> {
    let sudo = target == Path::new("/etc") || target == Path::new("/root");
    let mut outcome = Outcome::Ok;
    for rel in walk_files(src)? {
        if !filter(&rel) {
            continue;
        }
        let in_bin = rel
            .parent()
            .and_then(|p| p.file_name())
            .is_some_and(|name| name == "bin");
        let mode = in_bin.then_some(0o755);
        let from = src.join(&rel);
        let step = match rel.to_str().and_then(|r| r.strip_suffix(".j2")) {
            Some(stem) => copy::template(ctx, &from, &target.join(stem), mode, sudo).await?,
            None => copy::copy(ctx, &from, &target.join(&rel), mode, sudo).await?,
        };
        outcome = outcome.and(step);
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure::ctx::tests::ctx;
    use crate::configure::modules::file::scratch;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn mirrors_a_tree() {
        let dir = scratch("inflate");
        let src = dir.join("src");
        std::fs::create_dir_all(src.join(".local/bin")).unwrap();
        std::fs::create_dir_all(src.join(".config")).unwrap();
        std::fs::write(
            src.join(".local/bin/tool.j2"),
            "#!/bin/sh\necho {{ blueprint_user }}\n",
        )
        .unwrap();
        std::fs::write(src.join(".config/plain"), "plain\n").unwrap();
        let target = dir.join("home");
        let c = ctx(false);

        assert_eq!(
            inflate(&c, &src, &target, |_| true).await.unwrap(),
            Outcome::Changed
        );
        let tool = target.join(".local/bin/tool");
        assert_eq!(
            std::fs::read_to_string(&tool).unwrap(),
            "#!/bin/sh\necho tester\n"
        );
        assert_eq!(tool.metadata().unwrap().permissions().mode() & 0o777, 0o755);
        assert_eq!(
            std::fs::read_to_string(target.join(".config/plain")).unwrap(),
            "plain\n"
        );
        assert_eq!(
            inflate(&c, &src, &target, |_| true).await.unwrap(),
            Outcome::Ok
        );
    }
}
