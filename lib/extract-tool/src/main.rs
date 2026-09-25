// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Extract a tool from evgnomon/flow into its own repo at evgnomon/<tool>.
//!
//! Usage: extract-tool <tool-name> [--dry-run]
//!
//! What it does:
//!   1. Uses git subtree split to extract the tool's history into a branch
//!   2. Creates a new repo at ../<tool> (sibling to flow)
//!   3. Pulls the split branch into the new repo
//!   4. Removes the tool directory from flow
//!
//! The flow directory is the directory this program was invoked from
//! (`dirname "$0"`), like the original script.

use std::env;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio, exit};

fn usage(prog: &str) -> ! {
    println!("Usage: {prog} <tool-name> [--dry-run]");
    println!();
    println!("Extract a tool from evgnomon/flow into evgnomon/<tool>");
    println!();
    println!("Options:");
    println!("  --dry-run   Show what would be done without making changes");
    exit(1);
}

/// Logical (symlink-preserving) absolute path with `.`/`..` resolved
/// lexically, like `cd "$dir" && pwd`.
fn logical_abs(p: &Path) -> PathBuf {
    let base = env::var("PWD")
        .ok()
        .map(PathBuf::from)
        .filter(|b| b.is_absolute() && same_file(b, Path::new(".")))
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"));
    let mut out = PathBuf::from("/");
    for c in base.join(p).components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(n) => out.push(n),
            _ => {}
        }
    }
    out
}

fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (a.metadata(), b.metadata()) {
        (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
        _ => false,
    }
}

/// The path bash would put in `$0`: argv[0] if it contains a slash, otherwise
/// the first executable match on `$PATH`.
fn script_path(argv0: &str) -> PathBuf {
    if argv0.contains('/') {
        return PathBuf::from(argv0);
    }
    env::var_os("PATH")
        .and_then(|p| {
            env::split_paths(&p).map(|d| d.join(argv0)).find(|f| {
                fs::metadata(f).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            })
        })
        .unwrap_or_else(|| PathBuf::from(argv0))
}

fn dirname(p: &Path) -> PathBuf {
    match p.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        Some(_) => PathBuf::from("."),
        None => PathBuf::from("/"),
    }
}

/// Runs `cmd` in `dir`, exiting with its status on failure (`set -e`).
fn run(dir: &Path, cmd: &str, args: &[&str]) {
    match Command::new(cmd).args(args).current_dir(dir).status() {
        Ok(s) if s.success() => {}
        Ok(s) => exit(s.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("extract-tool: {cmd}: {e}");
            exit(127);
        }
    }
}

fn s(p: &Path) -> String {
    p.display().to_string()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let argv0 = args.first().map(String::as_str).unwrap_or("extract-tool");
    let script = script_path(argv0);

    if args.len() < 2 {
        usage(&s(&script));
    }

    let flow_dir = logical_abs(&dirname(&script));
    let base_dir = dirname(&flow_dir); // .../evgnomon/
    let zm_dir = base_dir.join("zm");
    let license_file = zm_dir.join("COPYING");

    let tool = args[1].as_str();
    let dry_run = args.get(2).is_some_and(|a| a == "--dry-run");

    let tool_dir = PathBuf::from(format!("{}/{tool}", s(&flow_dir)));
    let target_dir = PathBuf::from(format!("{}/{tool}", s(&base_dir)));

    if !tool_dir.is_dir() {
        println!("Error: tool directory not found: {}", s(&tool_dir));
        exit(1);
    }

    if !license_file.is_file() {
        println!("Error: license file not found: {}", s(&license_file));
        println!("Clone evgnomon/zm first.");
        exit(1);
    }

    if target_dir.is_dir() {
        println!("Error: target directory already exists: {}", s(&target_dir));
        exit(1);
    }

    let split_branch = format!("split-{tool}");

    println!("==> Extracting '{tool}' from flow");
    println!("    Source:  {}", s(&tool_dir));
    println!("    Target:  {}", s(&target_dir));
    println!();

    if dry_run {
        println!("[dry-run] Would run: git subtree split -P {tool} -b {split_branch}");
        println!("[dry-run] Would create repo at: {}", s(&target_dir));
        println!("[dry-run] Would pull split branch into new repo");
        println!("[dry-run] Would copy license from {}", s(&license_file));
        println!(
            "[dry-run] Would remove {} from flow and commit",
            s(&tool_dir)
        );
        exit(0);
    }

    let home = env::var("HOME").unwrap_or_default();
    let target = s(&target_dir);
    let flow = s(&flow_dir);

    // Step 1: Create the new repo
    println!("==> Creating new repo at {target}...");
    if let Err(e) = fs::create_dir_all(&target_dir) {
        eprintln!("extract-tool: {target}: {e}");
        exit(1);
    }
    run(&target_dir, "git", &["init"]);

    // Step 2: Configure git identity and signing
    println!("==> Configuring git with set_git_conf...");
    let set_git_conf = format!("{home}/.local/bin/set_git_conf");
    run(&target_dir, &set_git_conf, &["--public", &target]);

    // Step 3: Try subtree split to preserve history, fall back to copy
    println!("==> Splitting subtree for '{tool}'...");
    let split_ok = Command::new("git")
        .args(["subtree", "split", "-P", tool, "-b", &split_branch])
        .current_dir(&flow_dir)
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|st| st.success());
    if split_ok {
        println!("==> Pulling split history...");
        run(&target_dir, "git", &["pull", &flow, &split_branch]);

        // Clean up the split branch
        run(&flow_dir, "git", &["branch", "-D", &split_branch]);
    } else {
        println!(
            "==> subtree split failed (path type changed in history), copying current files..."
        );
        let src = format!("{}/.", s(&tool_dir));
        run(&flow_dir, "cp", &["-a", &src, &target]);
        run(&target_dir, "git", &["add", "-A"]);
        let msg = format!("Import {tool} from evgnomon/flow");
        run(&target_dir, "git", &["commit", "-m", &msg]);
    }

    // Step 4: Add the license from evgnomon/zm
    println!("==> Adding license from {}...", s(&license_file));
    let copying = format!("{target}/COPYING");
    run(&target_dir, "cp", &[&s(&license_file), &copying]);
    run(&target_dir, "git", &["add", "COPYING"]);
    run(&target_dir, "git", &["commit", "-m", "Add HGL license"]);

    // Step 5: Symlink bin files to ~/.local/bin
    println!("==> Linking bin files to ~/.local/bin...");
    let bin_dir = PathBuf::from(format!("{target}/bin"));
    if bin_dir.is_dir() {
        let mut names: Vec<String> = fs::read_dir(&bin_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|n| !n.starts_with('.'))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        for bin_name in names {
            let bin_file = format!("{target}/bin/{bin_name}");
            if !Path::new(&bin_file).is_file() {
                continue;
            }
            let link = format!("{home}/.local/bin/{bin_name}");
            let _ = fs::remove_file(&link);
            if let Err(e) = symlink(&bin_file, &link) {
                eprintln!("ln: failed to create symbolic link '{link}': {e}");
                exit(1);
            }
            println!("    {link} -> {bin_file}");
        }
    }

    // Step 6: Remove the tool from flow (last step)
    println!("==> Removing '{tool}' from flow...");
    run(&flow_dir, "git", &["rm", "-rf", tool]);
    let msg = format!("Extract {tool} into its own repo");
    run(&flow_dir, "git", &["commit", "-m", &msg]);
    run(&flow_dir, "git", &["push", "origin", "main"]);

    println!();
    println!("Done! '{tool}' is now at: {target}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirname_like_shell() {
        assert_eq!(dirname(Path::new("/a/b/c")), PathBuf::from("/a/b"));
        assert_eq!(dirname(Path::new("c")), PathBuf::from("."));
        assert_eq!(dirname(Path::new("/c")), PathBuf::from("/"));
        assert_eq!(dirname(Path::new("/")), PathBuf::from("/"));
    }

    #[test]
    fn logical_abs_normalizes() {
        assert_eq!(logical_abs(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
    }

    #[test]
    fn script_path_with_slash() {
        assert_eq!(script_path("./x/y"), PathBuf::from("./x/y"));
    }
}
