// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Mirror /media/$USER/archive/ to box:/home/archive/ with rsync.
//! Arguments are ignored, as in the original script.

use std::env;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, exit};

fn rsync_args(user: &str) -> Vec<String> {
    vec![
        "-av".into(),
        "--delete".into(),
        format!("--exclude-from=/media/{user}/archive/.exclude-list"),
        format!("/media/{user}/archive/"),
        "box:/home/archive/".into(),
    ]
}

fn main() {
    let user = env::var("USER").unwrap_or_default();
    let err = Command::new("rsync").args(rsync_args(&user)).exec();
    eprintln!("backup_archive: rsync: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}

#[cfg(test)]
mod tests {
    use super::rsync_args;

    #[test]
    fn paths_use_user() {
        let a = rsync_args("bob");
        assert_eq!(a[2], "--exclude-from=/media/bob/archive/.exclude-list");
        assert_eq!(a[3], "/media/bob/archive/");
        assert_eq!(a[4], "box:/home/archive/");
    }
}
