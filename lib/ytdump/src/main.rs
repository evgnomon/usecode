// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Download best audio as opus with yt-dlp, tracking downloads in `.files`.

use std::env;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, exit};

fn ytdlp_args(home: &str) -> Vec<String> {
    [
        "-f",
        "bestaudio",
        "--no-playlist",
        "--extract-audio",
        "--audio-format",
        "opus",
        "--embed-metadata",
        "--embed-thumbnail",
        "--js-runtimes",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([format!("node:{home}/.local/bin/node")])
    .chain(
        [
            "--remote-components",
            "ejs:github",
            "--match-filter",
            "duration > 60",
            "--download-archive",
            ".files",
        ]
        .iter()
        .map(|s| s.to_string()),
    )
    .collect()
}

fn main() {
    let home = env::var("HOME").unwrap_or_default();
    let err = Command::new("yt-dlp")
        .args(ytdlp_args(&home))
        .args(env::args_os().skip(1))
        .exec();
    eprintln!("ytdump: yt-dlp: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}

#[cfg(test)]
mod tests {
    use super::ytdlp_args;

    #[test]
    fn node_runtime_from_home() {
        let a = ytdlp_args("/home/u");
        let i = a.iter().position(|s| s == "--js-runtimes").unwrap();
        assert_eq!(a[i + 1], "node:/home/u/.local/bin/node");
        assert_eq!(a.last().unwrap(), ".files");
    }
}
