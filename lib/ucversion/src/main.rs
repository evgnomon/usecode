// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Print the version for the current checkout: the latest tag (without a
//! leading `v`) on master, otherwise the branch name.

use std::process::{Command, Stdio, exit};

fn code(status: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

/// Like `$(cmd)`: stdout with trailing newlines removed.
fn trim_newlines(mut s: String) -> String {
    while s.ends_with('\n') {
        s.pop();
    }
    s
}

fn version(branch: &str, tag: &str) -> String {
    if branch == "master" && !tag.is_empty() {
        tag.strip_prefix('v').unwrap_or(tag).to_string()
    } else {
        branch.to_string()
    }
}

fn main() {
    let out = match Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .stderr(Stdio::inherit())
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!("ucversion: git: {e}");
            exit(127);
        }
    };
    if !out.status.success() {
        exit(code(out.status));
    }
    let branch = trim_newlines(String::from_utf8_lossy(&out.stdout).into_owned());

    let tag = if branch == "master" {
        Command::new("git")
            .args(["describe", "--tags", "--abbrev=0"])
            .stderr(Stdio::null())
            .output()
            .map(|o| trim_newlines(String::from_utf8_lossy(&o.stdout).into_owned()))
            .unwrap_or_default()
    } else {
        String::new()
    };
    println!("{}", version(&branch, &tag));
}

#[cfg(test)]
mod tests {
    use super::version;

    #[test]
    fn picks_tag_on_master() {
        assert_eq!(version("master", "v1.2.3"), "1.2.3");
        assert_eq!(version("master", "1.2.3"), "1.2.3");
        assert_eq!(version("master", "vv1"), "v1");
        assert_eq!(version("master", ""), "master");
        assert_eq!(version("dev", "v1.0"), "dev");
    }
}
