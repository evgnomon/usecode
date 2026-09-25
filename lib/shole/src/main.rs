// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! shole: create a new executable script with a shebang.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::exit;

const HELP: &str = "shole: create a new executable script with a shebang
usage: shole [-h] [-s shell] [-p path] name
  -h, --help        show this help message and exit
  -s, --shell       shell/language to use: sh, bash, py, python, zig. Default is sh
  -p, --path        directory to place the script. Default is ~/.local/bin
  name              name of the script to create";

#[derive(Debug, PartialEq)]
struct Opts {
    help: bool,
    shell: String,
    path: String,
    name: String,
}

/// Parses arguments the same way the original bash loop did. Errors carry the
/// message to print on stderr.
fn parse(args: &[String], home: &str) -> Result<Opts, String> {
    let mut opts = Opts {
        help: false,
        shell: "sh".to_string(),
        path: format!("{home}/.local/bin"),
        name: String::new(),
    };
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-h" | "--help" => {
                opts.help = true;
                i += 1;
            }
            "-s" | "--shell" | "-p" | "--path" => {
                let is_shell = matches!(a, "-s" | "--shell");
                let v = args.get(i + 1).map(String::as_str).unwrap_or("");
                if v.is_empty() {
                    let flag = if is_shell { "-s/--shell" } else { "-p/--path" };
                    return Err(format!("shole: error: {flag} requires an argument"));
                }
                if is_shell {
                    opts.shell = v.to_string();
                } else {
                    opts.path = v.to_string();
                }
                i += 2;
            }
            _ if a.starts_with('-') => {
                return Err(format!("shole: error: unknown option: {a}"));
            }
            _ => {
                opts.name = a.to_string();
                i += 1;
            }
        }
    }
    Ok(opts)
}

fn shebang(shell: &str) -> &'static str {
    match shell {
        "py" | "python" => "#!/usr/bin/env python3",
        "zig" => r#"//usr/bin/env zig run "$0" -- "$@"; exit "$?""#,
        _ => "#!/usr/bin/env bash",
    }
}

/// Current process umask, read from /proc (the only std-only way that does not
/// modify it). Falls back to 022.
fn umask() -> u32 {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("Umask:"))
                .and_then(|v| u32::from_str_radix(v.trim(), 8).ok())
        })
        .unwrap_or(0o022)
}

fn fail(msg: String) -> ! {
    eprintln!("{msg}");
    exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let home = std::env::var("HOME").unwrap_or_default();
    let opts = parse(&args, &home).unwrap_or_else(|e| fail(e));

    if opts.help {
        println!("{HELP}");
        return;
    }
    if opts.name.is_empty() {
        println!("shole: error: the following arguments are required: name");
        exit(1);
    }
    let bin_path = format!("{}/{}", opts.path, opts.name);

    if let Err(e) = fs::create_dir_all(&opts.path) {
        fail(format!("shole: {}: {e}", opts.path));
    }
    if let Err(e) = fs::write(&bin_path, format!("{}\n", shebang(&opts.shell))) {
        fail(format!("shole: {bin_path}: {e}"));
    }
    // chmod +x: add execute bits not masked by the umask.
    let meta = fs::metadata(&bin_path).unwrap_or_else(|e| fail(format!("shole: {bin_path}: {e}")));
    let mut perms = meta.permissions();
    perms.set_mode(perms.mode() | (0o111 & !umask()));
    if let Err(e) = fs::set_permissions(&bin_path, perms) {
        fail(format!("shole: {bin_path}: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn defaults() {
        let o = parse(&v(&["foo"]), "/h").unwrap();
        assert_eq!(o.shell, "sh");
        assert_eq!(o.path, "/h/.local/bin");
        assert_eq!(o.name, "foo");
        assert!(!o.help);
    }

    #[test]
    fn flags() {
        let o = parse(&v(&["-s", "py", "--path", "/x", "bar", "-h"]), "/h").unwrap();
        assert_eq!(o.shell, "py");
        assert_eq!(o.path, "/x");
        assert_eq!(o.name, "bar");
        assert!(o.help);
    }

    #[test]
    fn errors() {
        assert_eq!(
            parse(&v(&["-s"]), "/h").unwrap_err(),
            "shole: error: -s/--shell requires an argument"
        );
        assert_eq!(
            parse(&v(&["--path", ""]), "/h").unwrap_err(),
            "shole: error: -p/--path requires an argument"
        );
        assert_eq!(
            parse(&v(&["-h", "-x"]), "/h").unwrap_err(),
            "shole: error: unknown option: -x"
        );
    }

    #[test]
    fn shebangs() {
        assert_eq!(shebang("python"), "#!/usr/bin/env python3");
        assert_eq!(shebang("sh"), "#!/usr/bin/env bash");
        assert_eq!(shebang("ruby"), "#!/usr/bin/env bash");
        assert_eq!(
            shebang("zig"),
            "//usr/bin/env zig run \"$0\" -- \"$@\"; exit \"$?\""
        );
    }
}
