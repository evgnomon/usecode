// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Compress every `*.jpg` / `*.jpeg` in a directory with ImageMagick's
//! `convert` into `./pressed`.

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, exit};

const OUTPUT_DIR: &str = "pressed";

/// Returns true when `q` is all digits and between 1 and 100.
fn valid_quality(q: &str) -> bool {
    !q.is_empty()
        && q.bytes().all(|b| b.is_ascii_digit())
        && q.trim_start_matches('0')
            .parse::<u32>()
            .is_ok_and(|n| (1..=100).contains(&n))
}

fn in_path(cmd: &str) -> bool {
    env::var_os("PATH").is_some_and(|p| {
        env::split_paths(&p).any(|d| {
            fs::metadata(d.join(cmd))
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
    })
}

/// Non-hidden files in `dir` whose name ends with `.{ext}`, sorted by name
/// (like the bash glob `"$DIR"/*.ext`).
fn glob_ext(dir: &str, ext: &str) -> Vec<String> {
    let suffix = format!(".{ext}");
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| !n.starts_with('.') && n.ends_with(&suffix))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let prog = args.first().map(String::as_str).unwrap_or("imgpress");
    if args.len() < 2 || args.len() > 3 {
        println!("Usage: {prog} <directory> [quality]");
        println!("Example: {prog} images 50");
        exit(1);
    }

    let dir = args[1].as_str();
    let quality = args.get(2).map(String::as_str).unwrap_or("50");

    if !valid_quality(quality) {
        println!("Error: Quality must be a number between 1 and 100");
        exit(1);
    }

    if !Path::new(dir).is_dir() {
        println!("Error: Directory '{dir}' does not exist");
        exit(1);
    }

    let _ = fs::create_dir_all(OUTPUT_DIR);

    if !in_path("convert") {
        println!("Error: ImageMagick is not installed. Please install it to use this script.");
        exit(1);
    }

    for ext in ["jpg", "jpeg"] {
        for filename in glob_ext(dir, ext) {
            let file = format!("{dir}/{filename}");
            if !Path::new(&file).is_file() {
                continue;
            }
            println!("Compressing {filename} with quality {quality}%");
            let _ = Command::new("convert")
                .arg(&file)
                .arg("-quality")
                .arg(format!("{quality}%"))
                .arg(format!("{OUTPUT_DIR}/{filename}"))
                .status();
        }
    }

    let empty = fs::read_dir(OUTPUT_DIR).map_or(true, |mut rd| rd.next().is_none());
    if empty {
        println!("No JPG files found in '{dir}'");
        let _ = fs::remove_dir(OUTPUT_DIR);
    } else {
        println!("Compressed images saved in '{OUTPUT_DIR}'");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality() {
        for ok in ["1", "50", "100", "050", "0100"] {
            assert!(valid_quality(ok), "{ok}");
        }
        for bad in ["", "0", "101", "-5", "5.5", "abc", "99999999999999999999"] {
            assert!(!valid_quality(bad), "{bad}");
        }
    }
}
