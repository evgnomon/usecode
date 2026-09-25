// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Small helpers shared by the server and the CLI client.

use std::io::{BufRead, IsTerminal, Write};

use chrono::{DateTime, Utc};
use serde_json::Value;

/// Format a UTC timestamp like Python's `datetime.isoformat()` for an aware
/// UTC datetime: microseconds are omitted when zero.
pub fn isoformat(t: DateTime<Utc>) -> String {
    let base = t.format("%Y-%m-%dT%H:%M:%S");
    let micros = t.timestamp_subsec_micros();
    match micros {
        0 => format!("{base}+00:00"),
        m => format!("{base}.{m:06}+00:00"),
    }
}

/// Current UTC time in isoformat.
pub fn now_iso() -> String {
    isoformat(Utc::now())
}

/// Sanitize path components to prevent directory traversal.
pub fn sanitize_path_component(component: &str) -> String {
    component.replace(['/', '\\'], "_").replace("..", "_")
}

/// Render a JSON value the way Python's `str()` would for the scalar types
/// that appear in API responses.
pub fn py_str(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "None".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(true)) => "True".to_string(),
        Some(Value::Bool(false)) => "False".to_string(),
        Some(other) => other.to_string(),
    }
}

/// ANSI foreground color code for a click color name.
fn color_code(fg: &str) -> &'static str {
    match fg {
        "red" => "31",
        "green" => "32",
        "yellow" => "33",
        _ => "37",
    }
}

/// Style text like `click.style(text, fg=...)`; styling is dropped when
/// stdout is not a terminal, mirroring `click.echo`'s ANSI stripping.
pub fn style(text: &str, fg: &str) -> String {
    match std::io::stdout().is_terminal() {
        true => format!("\x1b[{}m{}\x1b[0m", color_code(fg), text),
        false => text.to_string(),
    }
}

/// Color used for run statuses in `list` and `status` output.
pub fn status_color(status: &str) -> &'static str {
    match status {
        "completed" => "green",
        "failed" => "red",
        "running" => "yellow",
        _ => "white",
    }
}

/// Parse a click-style confirmation answer; `None` means invalid input.
pub fn parse_confirm(answer: &str) -> Option<bool> {
    match answer.trim().to_lowercase().as_str() {
        "" | "n" | "no" => Some(false),
        "y" | "yes" => Some(true),
        _ => None,
    }
}

/// Emulate `click.confirm(text, abort=True)`: exits with status 1 and
/// "Aborted!" unless the user answers yes.
pub fn confirm_or_abort(text: &str) {
    let stdin = std::io::stdin();
    loop {
        print!("{text} [y/N]: ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        let read = stdin.lock().read_line(&mut line);
        let answer = match read {
            Ok(0) | Err(_) => {
                println!();
                abort()
            }
            Ok(_) => parse_confirm(&line),
        };
        match answer {
            Some(true) => return,
            Some(false) => abort(),
            None => eprintln!("Error: invalid input"),
        }
    }
}

fn abort() -> ! {
    eprintln!("Aborted!");
    std::process::exit(1)
}

/// Emulate Python's `getpass.getuser()`.
pub fn getuser() -> String {
    ["LOGNAME", "USER", "LNAME", "USERNAME"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty())
        .or_else(|| {
            std::process::Command::new("id")
                .arg("-un")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn isoformat_matches_python() {
        let t = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        assert_eq!(isoformat(t), "2026-01-02T03:04:05+00:00");
        let t = t + chrono::Duration::microseconds(42);
        assert_eq!(isoformat(t), "2026-01-02T03:04:05.000042+00:00");
    }

    #[test]
    fn sanitize() {
        assert_eq!(sanitize_path_component("a/b\\c"), "a_b_c");
        assert_eq!(sanitize_path_component("../x"), "__x");
        assert_eq!(sanitize_path_component("..."), "_.");
        assert_eq!(sanitize_path_component("ok-name"), "ok-name");
    }

    #[test]
    fn py_str_values() {
        assert_eq!(py_str(None), "None");
        assert_eq!(py_str(Some(&Value::Null)), "None");
        assert_eq!(py_str(Some(&serde_json::json!(3))), "3");
        assert_eq!(py_str(Some(&serde_json::json!("x"))), "x");
    }

    #[test]
    fn confirm_answers() {
        assert_eq!(parse_confirm("y\n"), Some(true));
        assert_eq!(parse_confirm("YES"), Some(true));
        assert_eq!(parse_confirm("\n"), Some(false));
        assert_eq!(parse_confirm("no"), Some(false));
        assert_eq!(parse_confirm("maybe"), None);
    }
}
