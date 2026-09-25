// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

mod exec;

use std::io::Write;
use std::path::Path;
use std::process;
use std::time::Duration;

use clap::{CommandFactory, Parser};

const EXAMPLES: &str =
    "annabelle — Run local script + optional file uploads on multiple remote servers via SSH/SCP

Examples:
  annabelle server1 server2 myscript.sh
  annabelle server1 server2 -f data.csv myscript.sh
  annabelle srv1 srv2 -f file1.txt -f file2.conf
  annabelle host1 -f config.yaml:/etc/myapp/config.yaml setup.sh";

/// Run a local script (and optionally upload files first) on multiple remote servers
#[derive(Parser)]
#[command(name = "annabelle", after_help = EXAMPLES)]
struct Cli {
    /// Server(s) followed by optional script file
    #[arg(value_name = "SERVER [SCRIPT]")]
    targets: Vec<String>,
    /// File to upload (src:dst format, dst defaults to ~/.annabelle/filename)
    #[arg(short = 'f', long = "file", value_name = "SRC[:DST]")]
    upload_files: Vec<String>,
    /// Maximum number of concurrent connections
    #[arg(short, long, default_value_t = 5, value_parser = clap::value_parser!(u16).range(1..))]
    parallel: u16,
}

/// Parent directory as Python's `str(pathlib.PurePosixPath(p).parent)`.
fn py_parent(p: &str) -> String {
    let root = p.starts_with('/');
    let mut parts: Vec<&str> = p
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    parts.pop();
    match (root, parts.is_empty()) {
        (true, _) => format!("/{}", parts.join("/")),
        (false, true) => ".".into(),
        (false, false) => parts.join("/"),
    }
}

/// Upload one file to one server with scp, creating the remote dir first.
fn scp_upload(server: &str, local: &str, remote: Option<&str>) -> (String, i32, String) {
    let remote = match remote {
        Some(r) => r.to_string(),
        None => {
            let name = Path::new(local)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("~/.annabelle/{name}")
        }
    };
    let attempt = || -> Result<(i32, String), String> {
        let mut remote_dir = py_parent(&remote);
        if !remote_dir.is_empty() && remote_dir != "." {
            // Replace ~ with $HOME for proper expansion inside double quotes
            if let Some(rest) = remote_dir.strip_prefix('~') {
                remote_dir = format!("$HOME{rest}");
            }
            let args = [
                "ssh".into(),
                "-q".into(),
                server.into(),
                format!("mkdir -p \"{remote_dir}\""),
            ];
            let (rc, _, err) = exec::run(&args, None, Duration::from_secs(30))?;
            if rc != 0 {
                return Ok((rc, err));
            }
        }
        let args = [
            "scp".into(),
            "-q".into(),
            local.into(),
            format!("{server}:{remote}"),
        ];
        let (rc, _, err) = exec::run(&args, None, Duration::from_secs(120))?;
        Ok((rc, err))
    };
    match attempt() {
        Ok((rc, err)) => (server.into(), rc, err),
        Err(e) => (server.into(), 1, format!("Exception during scp: {e}")),
    }
}

/// Execute script content remotely via `ssh bash -s`.
fn run_script(server: &str, script: &str) -> exec::Output {
    let args = ["ssh".into(), "-q".into(), server.into(), "bash -s".into()];
    exec::run(&args, Some(script), Duration::from_secs(300))
        .unwrap_or_else(|e| (1, String::new(), format!("Exception during ssh: {e}")))
}

fn flush() {
    let _ = std::io::stdout().flush();
}

fn run(cli: Cli) -> i32 {
    if cli.targets.is_empty() && cli.upload_files.is_empty() {
        let _ = Cli::command().print_help();
        return 1;
    }

    // Heuristic: last argument is the script if it exists locally.
    let (servers, script_path) = match cli.targets.split_last() {
        Some((last, rest)) if Path::new(last).is_file() => (rest.to_vec(), Some(last.clone())),
        _ => (cli.targets.clone(), None),
    };
    if servers.is_empty() {
        eprintln!("Error: No servers specified");
        return 1;
    }

    let workers = usize::from(cli.parallel);
    let mut overall_exit = 0;

    if !cli.upload_files.is_empty() {
        println!("Uploading files...\n");
        let mut tasks = Vec::new();
        for spec in &cli.upload_files {
            let (src, dst) = match spec.split_once(':') {
                Some((s, d)) => (s.to_string(), Some(d.to_string())),
                None => (spec.clone(), None),
            };
            if !Path::new(&src).is_file() {
                eprintln!("Error: Cannot read file: {src}");
                overall_exit = 1;
                continue;
            }
            for srv in &servers {
                tasks.push((srv.clone(), src.clone(), dst.clone()));
            }
        }
        exec::pool(
            tasks,
            workers,
            |(srv, src, dst)| (scp_upload(&srv, &src, dst.as_deref()), src),
            |((server, rc, err), local_name)| {
                let status = match rc {
                    0 => "OK",
                    _ => "FAILED",
                };
                print!("  [{server}] {local_name:<20} → {status}");
                if rc != 0 && !err.trim().is_empty() {
                    flush();
                    eprintln!("  ({})", err.trim());
                } else {
                    println!();
                }
            },
        );
        println!();

        if overall_exit != 0 {
            flush();
            eprintln!("Some uploads failed → continuing anyway\n");
        }
    }

    if let Some(script_path) = script_path {
        let content = match std::fs::read_to_string(&script_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error reading script {script_path}: {e}");
                return 1;
            }
        };
        let name = Path::new(&script_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        println!("Executing {name} on {} server(s)...\n", servers.len());

        let bar = "═".repeat(10);
        exec::pool(
            servers,
            workers,
            |srv: String| {
                let res = run_script(&srv, &content);
                (srv, res)
            },
            |(server, (rc, out, err))| {
                println!("{bar} {server} {bar}");
                if !out.trim().is_empty() {
                    print!("{out}");
                }
                flush();
                if !err.trim().is_empty() {
                    eprint!("{err}");
                }
                if rc != 0 {
                    eprintln!("[exit {rc}]");
                    overall_exit = overall_exit.max(rc);
                }
                println!();
            },
        );
    }

    if overall_exit != 0 {
        flush();
        eprintln!("\nFinished with errors (exit {overall_exit})");
    }
    overall_exit
}

fn main() {
    let code = run(Cli::parse());
    flush();
    process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_like_pathlib() {
        assert_eq!(py_parent("~/.annabelle/x.txt"), "~/.annabelle");
        assert_eq!(py_parent("/etc/app/c.yaml"), "/etc/app");
        assert_eq!(py_parent("c.yaml"), ".");
        assert_eq!(py_parent(""), ".");
        assert_eq!(py_parent("dir/"), ".");
        assert_eq!(py_parent("/x"), "/");
        assert_eq!(py_parent("/"), "/");
        assert_eq!(py_parent("a//b/./c"), "a/b");
    }

    #[test]
    fn interleaved_args() {
        let cli = Cli::try_parse_from(["annabelle", "s1", "-f", "x", "s2", "run.sh"]).unwrap();
        assert_eq!(cli.targets, ["s1", "s2", "run.sh"]);
        assert_eq!(cli.upload_files, ["x"]);
        assert_eq!(cli.parallel, 5);
    }
}
