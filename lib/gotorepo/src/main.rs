// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Open the current repository's origin remote in the browser via `fzurls`.

use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio, exit};

/// Converts an SSH remote URL to its HTTPS web URL.
fn to_https(url: &str) -> String {
    let url = url.strip_suffix(".git").unwrap_or(url);
    if let Some(rest) = url.strip_prefix("git@") {
        // git@host:owner/repo -> https://host/owner/repo
        format!("https://{}", rest.replacen(':', "/", 1))
    } else if let Some(rest) = url.strip_prefix("ssh://") {
        let rest = rest.split_once('@').map_or(rest, |(_, r)| r);
        format!("https://{rest}")
    } else {
        url.to_string()
    }
}

fn main() {
    let out = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .stderr(Stdio::null())
        .output();
    let url = match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .trim_end_matches('\n')
            .to_string(),
        _ => {
            eprintln!("Not a git repository or no origin remote");
            exit(1);
        }
    };

    let err = Command::new("fzurls").arg(to_https(&url)).exec();
    eprintln!("gotorepo: fzurls: {err}");
    exit(127);
}

#[cfg(test)]
mod tests {
    use super::to_https;

    #[test]
    fn scp_style() {
        assert_eq!(
            to_https("git@github.com:evgnomon/usecode.git"),
            "https://github.com/evgnomon/usecode"
        );
        assert_eq!(to_https("git@gitlab.com:a/b"), "https://gitlab.com/a/b");
    }

    #[test]
    fn ssh_scheme() {
        assert_eq!(
            to_https("ssh://git@github.com/evgnomon/usecode.git"),
            "https://github.com/evgnomon/usecode"
        );
        assert_eq!(
            to_https("ssh://github.com/evgnomon/usecode"),
            "https://github.com/evgnomon/usecode"
        );
    }

    #[test]
    fn https_unchanged() {
        assert_eq!(
            to_https("https://github.com/evgnomon/usecode.git"),
            "https://github.com/evgnomon/usecode"
        );
        assert_eq!(to_https("/local/path"), "/local/path");
    }
}
