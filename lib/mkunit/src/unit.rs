// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Unit file rendering and naming helpers.

/// Render the systemd unit file contents.
pub fn render(description: &str, exec_start: &str) -> String {
    format!(
        "[Unit]
Description={description}
After=network.target

[Service]
Type=simple
ExecStart={exec_start}
Restart=on-failure
RestartSec=5s
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
"
    )
}

/// Full unit file name, appending `.service` when missing.
pub fn full_name(unit_name: &str) -> String {
    if unit_name.ends_with(".service") {
        unit_name.to_string()
    } else {
        format!("{unit_name}.service")
    }
}

/// Display a path the way Python's `pathlib.Path` normalizes it.
pub fn display_path(p: &str) -> String {
    let prefix = if p.starts_with("//") && !p.starts_with("///") {
        "//"
    } else if p.starts_with('/') {
        "/"
    } else {
        ""
    };
    let parts: Vec<&str> = p
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    let joined = format!("{prefix}{}", parts.join("/"));
    if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_unit() {
        let u = render("Run: ls -la", "ls -la");
        assert!(u.starts_with("[Unit]\nDescription=Run: ls -la\nAfter=network.target\n"));
        assert!(u.contains("\nExecStart=ls -la\n"));
        assert!(u.ends_with("[Install]\nWantedBy=multi-user.target\n"));
    }

    #[test]
    fn full_names() {
        assert_eq!(full_name("foo"), "foo.service");
        assert_eq!(full_name("foo.service"), "foo.service");
    }

    #[test]
    fn paths() {
        assert_eq!(display_path("./x//y/"), "x/y");
        assert_eq!(
            display_path("/etc/systemd/system/a.service"),
            "/etc/systemd/system/a.service"
        );
        assert_eq!(display_path("."), ".");
        assert_eq!(display_path("../a"), "../a");
    }
}
