// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Dispatch to per-blueprint plat scripts and whole-host operations.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio, exit};
use std::sync::LazyLock;

use regex::Regex;

use crate::dotenv;
use crate::env::{Pod, ensure_env, env_path, set_env_var};
use crate::paths::Paths;

/// Return the set of podman container names on this host.
pub fn list_containers(running_only: bool) -> HashSet<String> {
    let mut args = vec!["ps", "--format", "{{.Names}}"];
    if !running_only {
        args.insert(1, "-a");
    }
    let out = Command::new("podman")
        .args(&args)
        .stdin(Stdio::inherit())
        .stderr(Stdio::piped())
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|n| !n.trim().is_empty())
            .map(String::from)
            .collect(),
        _ => HashSet::new(),
    }
}

pub fn template_dir(paths: &Paths, service: &str) -> PathBuf {
    let dir = paths.templates.join(service);
    if !dir.is_dir() {
        eprintln!("Error: no template found at {}", dir.display());
        exit(1);
    }
    dir
}

/// Prepare env file and values; optionally persist STATUS from the action.
fn prepare_dispatch(
    paths: &Paths,
    pod: &Pod,
    service: &str,
    actions: &[&str],
    persist: bool,
) -> (PathBuf, PathBuf, HashMap<String, String>) {
    let plat_dir = template_dir(paths, service);
    let env_file = env_path(paths, pod, service);
    let mut values = ensure_env(paths, &env_file, pod);

    if persist {
        let status = match actions.first().copied() {
            Some("up" | "restart") => Some("up"),
            Some("down" | "destroy") => Some("down"),
            _ => None,
        };
        if let Some(status) = status {
            set_env_var(&env_file, "STATUS", status);
            values.insert("STATUS".into(), status.into());
        }
    }
    (plat_dir, env_file, values)
}

fn plat_command(
    plat_dir: &Path,
    env_file: &Path,
    values: &HashMap<String, String>,
    actions: &[&str],
) -> Command {
    let script = plat_dir.join("scripts").join("plat");
    let mut cmd = Command::new(script);
    cmd.args(actions).envs(values).env("ENV_FILE", env_file);
    cmd
}

/// Set up env and exec into the per-service plat script.
pub fn dispatch(paths: &Paths, pod: &Pod, service: &str, actions: &[&str]) -> ! {
    let (plat_dir, env_file, values) = prepare_dispatch(paths, pod, service, actions, true);
    let mut cmd = plat_command(&plat_dir, &env_file, &values, actions);
    let _ = std::io::stdout().flush();
    let err = cmd.exec();
    eprintln!(
        "Error: {}: {err}",
        plat_dir.join("scripts").join("plat").display()
    );
    exit(1);
}

/// Like `dispatch`, but runs the script as a child and returns success.
fn run_dispatch(paths: &Paths, pod: &Pod, service: &str, actions: &[&str], persist: bool) -> bool {
    let (plat_dir, env_file, values) = prepare_dispatch(paths, pod, service, actions, persist);
    match plat_command(&plat_dir, &env_file, &values, actions).status() {
        Ok(s) => s.success(),
        Err(e) => {
            eprintln!(
                "Error: {}: {e}",
                plat_dir.join("scripts").join("plat").display()
            );
            false
        }
    }
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default()
}

/// Sorted `$PLAT_HOME/*/*/*/*.env` regular files.
pub fn env_files(plat_home: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for t in subdirs(plat_home) {
        for n in subdirs(&t) {
            for p in subdirs(&n) {
                let Ok(entries) = fs::read_dir(&p) else {
                    continue;
                };
                out.extend(
                    entries
                        .filter_map(Result::ok)
                        .filter(|e| e.file_name().to_string_lossy().ends_with(".env"))
                        .map(|e| e.path())
                        .filter(|f| f.is_file()),
                );
            }
        }
    }
    out.sort();
    out
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn field<'a>(values: &'a HashMap<String, String>, key: &str) -> &'a str {
    values.get(key).map(String::as_str).unwrap_or("")
}

/// Walk env files under PLAT_HOME and converge each to its STATUS (or `force`).
///
/// With `force` set (global up/down), STATUS in env files is NOT modified.
pub fn do_apply(paths: &Paths, force: Option<&str>) -> i32 {
    if !paths.plat_home.is_dir() {
        println!("No PLAT_HOME at {}", paths.plat_home.display());
        return 0;
    }
    let containers = list_containers(false);
    let mut rc = 0;
    let persist = force.is_none();
    for env_file in env_files(&paths.plat_home) {
        let values = dotenv::read_env(&env_file);
        let action = match force {
            Some(f) => f.to_string(),
            None => values
                .get("STATUS")
                .cloned()
                .unwrap_or_else(|| "down".into()),
        };
        let tenant = field(&values, "TENANT");
        let namespace = field(&values, "NAMESPACE");
        let pod = field(&values, "POD");
        let service = stem(&env_file);

        if action != "up" && action != "down" {
            println!(
                "skip {}: STATUS={action} (expected up|down)",
                env_file.display()
            );
            continue;
        }
        let primary = format!("{tenant}_{namespace}_{pod}_{service}");
        if action == "down" && !containers.contains(&primary) {
            continue;
        }
        println!("==> {tenant}/{namespace}/{pod} {service} => {action}");
        let target = Pod {
            tenant: tenant.into(),
            namespace: namespace.into(),
            pod: pod.into(),
        };
        if !run_dispatch(paths, &target, &service, &[&action], persist) {
            rc = 1;
            println!("!!  failed: {service} ({action})");
        }
    }
    rc
}

/// Format rows as left-aligned columns separated by two spaces.
pub fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let widths: Vec<usize> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            rows.iter()
                .map(|r| r[i].chars().count())
                .chain([h.chars().count()])
                .max()
                .unwrap_or(0)
        })
        .collect();
    let line = |cells: Vec<&str>| {
        cells
            .iter()
            .zip(&widths)
            .map(|(c, w)| format!("{c:<w$}"))
            .collect::<Vec<_>>()
            .join("  ")
    };
    let mut out = vec![line(headers.to_vec())];
    out.extend(
        rows.iter()
            .map(|r| line(r.iter().map(String::as_str).collect())),
    );
    out.join("\n")
}

/// Show declared and actual state of every service under PLAT_HOME.
pub fn ps(paths: &Paths) {
    if !paths.plat_home.is_dir() {
        println!("No PLAT_HOME at {}", paths.plat_home.display());
        return;
    }
    let running = list_containers(true);
    let all = list_containers(false);
    let rows: Vec<Vec<String>> = env_files(&paths.plat_home)
        .iter()
        .map(|env_file| {
            let v = dotenv::read_env(env_file);
            let (t, n, p) = (
                field(&v, "TENANT"),
                field(&v, "NAMESPACE"),
                field(&v, "POD"),
            );
            let service = stem(env_file);
            let primary = format!("{t}_{n}_{p}_{service}");
            let state = if running.contains(&primary) {
                "running"
            } else if all.contains(&primary) {
                "stopped"
            } else {
                "-"
            };
            vec![
                format!("{t}/{n}/{p}"),
                service,
                field(&v, "STATUS").into(),
                state.into(),
                field(&v, "PORT").into(),
            ]
        })
        .collect();
    if rows.is_empty() {
        println!("No services defined.");
        return;
    }
    println!(
        "{}",
        table(&["POD", "SERVICE", "STATUS", "STATE", "PORT"], &rows)
    );
}

/// Extract `IP:port:port` mappings quoted in a rendered compose file.
pub fn port_mappings(rendered: &str) -> Vec<&str> {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""(\d+\.\d+\.\d+\.\d+:\d+:\d+)""#).unwrap());
    RE.captures_iter(rendered)
        .map(|c| c.get(1).unwrap().as_str())
        .collect()
}

/// Print resolved IP:port:port mappings for SERVICE.
pub fn port(paths: &Paths, pod: &Pod, service: &str) {
    let plat_dir = template_dir(paths, service);
    let values = ensure_env(paths, &env_path(paths, pod, service), pod);

    let compose = plat_dir
        .join("deploy")
        .join("podman")
        .join("podman-compose.yaml");
    if !compose.is_file() {
        eprintln!("Error: no compose file at {}", compose.display());
        exit(1);
    }
    let text = match fs::read_to_string(&compose) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error: {}: {e}", compose.display());
            exit(1);
        }
    };
    let child = Command::new("envsubst")
        .envs(&values)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            if let Some(mut stdin) = c.stdin.take() {
                stdin.write_all(text.as_bytes())?;
            }
            c.wait_with_output()
        });
    let rendered = match child {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        Ok(o) => {
            eprintln!("Error: envsubst exited with {}", o.status);
            exit(1);
        }
        Err(e) => {
            eprintln!("Error: envsubst: {e}");
            exit(1);
        }
    };
    for m in port_mappings(&rendered) {
        println!("{m}");
    }
}

/// Remove SERVICE's env file.
pub fn undefine(paths: &Paths, pod: &Pod, service: &str) {
    let env_file = env_path(paths, pod, service);
    if env_file.is_file() {
        if let Err(e) = fs::remove_file(&env_file) {
            eprintln!("Error: {}: {e}", env_file.display());
            exit(1);
        }
        println!("Removed {}", env_file.display());
    } else {
        println!("No env file at {}", env_file.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mappings() {
        let y = "ports:\n  - \"127.0.0.1:12345:80\"\n  - '1.2.3.4:1:2'\n  - \"10.0.0.1:5:6\"\n";
        assert_eq!(port_mappings(y), ["127.0.0.1:12345:80", "10.0.0.1:5:6"]);
    }

    #[test]
    fn table_format() {
        let rows = vec![vec!["sys/main/web".to_string(), "x".into()]];
        assert_eq!(
            table(&["POD", "SERVICE"], &rows),
            "POD           SERVICE\nsys/main/web  x      "
        );
    }
}
