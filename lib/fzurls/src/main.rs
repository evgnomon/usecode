// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! fzurls: open a URL, file or search query in Brave, or pick a bookmark from
//! the `urls` list in `$HOME/src/github.com/$USER/config/config.yaml` with fzf.
//!
//! When the `fzurls-focus@org.evgnomon` GNOME extension is running, a Brave
//! window on the active workspace is focused first; otherwise a new window is
//! opened.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::process::{Command, Stdio, exit};

use serde_json::Value;

/// Percent-encodes `input` like Python's `urllib.parse.quote(s, safe)`.
fn quote(input: &[u8], safe: &[u8]) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input {
        if b.is_ascii_alphanumeric() || b"_.-~".contains(&b) || safe.contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Like Python's `urllib.parse.quote_plus(s)`.
fn quote_plus(input: &[u8]) -> String {
    quote(input, b" ").replace(' ', "+")
}

fn search_url(query: &[u8]) -> String {
    format!(
        "https://duckduckgo.com/?q={}&t=brave&ia=web",
        quote_plus(query)
    )
}

fn open_in_brave(url: &str) {
    let result = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.evgnomon.FzURLs",
            "--object-path",
            "/org/evgnomon/FzURLs",
            "--method",
            "org.evgnomon.FzURLs.FocusBraveOnActiveWorkspace",
        ])
        .stdin(Stdio::null())
        .output()
        .map(|o| {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            s
        })
        .unwrap_or_default();

    let mut brave = Command::new("brave-browser");
    // "true": Brave window found and focused on current workspace.
    // "false": extension running but no Brave on current workspace.
    // Otherwise the extension is not available: just open normally.
    if !result.contains("true") && result.contains("false") {
        brave.arg("--new-window");
    }
    let _ = brave
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn is_http_url(s: &[u8]) -> bool {
    s.starts_with(b"http://") || s.starts_with(b"https://")
}

/// jq string interpolation (`"\(.x)"`): strings raw, everything else as JSON.
fn jq_str(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => "null".to_string(),
    }
}

/// `jq -r '.urls[] | "\(.name)\t\(.url)"'` over the config (converted by yj).
fn url_lines(json: &Value) -> Vec<String> {
    let items: Vec<&Value> = match json.get("urls") {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(Value::Object(o)) => o.values().collect(),
        _ => Vec::new(),
    };
    items
        .into_iter()
        .map_while(|item| match item {
            Value::Object(o) => Some(format!(
                "{}\t{}",
                jq_str(o.get("name")),
                jq_str(o.get("url"))
            )),
            Value::Null => Some("null\tnull".to_string()),
            _ => None,
        })
        .collect()
}

fn config_lines() -> Vec<String> {
    let home = env::var("HOME").unwrap_or_default();
    let user = env::var("USER").unwrap_or_default();
    let path = format!("{home}/src/github.com/{user}/config/config.yaml");
    let yaml = fs::read(&path).unwrap_or_else(|e| {
        eprintln!("cat: {path}: {e}");
        Vec::new()
    });
    let Ok(mut yj) = Command::new("yj")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    else {
        eprintln!("fzurls: yj: command not found");
        return Vec::new();
    };
    if let Some(mut stdin) = yj.stdin.take() {
        let _ = stdin.write_all(&yaml);
    }
    let out = yj.wait_with_output().map(|o| o.stdout).unwrap_or_default();
    serde_json::from_slice::<Value>(&out)
        .map(|v| url_lines(&v))
        .unwrap_or_default()
}

/// Runs fzf with `args`, feeding `input` on stdin (or /dev/null), and returns
/// its stdout with trailing newlines removed.
fn fzf(args: &[&str], input: Option<String>) -> String {
    let mut cmd = Command::new("fzf");
    cmd.args(args).stdout(Stdio::piped());
    cmd.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let Ok(mut child) = cmd.spawn() else {
        eprintln!("fzurls: fzf: command not found");
        return String::new();
    };
    let writer = child.stdin.take().zip(input).map(|(mut stdin, data)| {
        std::thread::spawn(move || {
            let _ = stdin.write_all(data.as_bytes());
        })
    });
    let out = child
        .wait_with_output()
        .map(|o| o.stdout)
        .unwrap_or_default();
    if let Some(w) = writer {
        let _ = w.join();
    }
    String::from_utf8_lossy(&out)
        .trim_end_matches('\n')
        .to_string()
}

fn main() {
    let args: Vec<OsString> = env::args_os().skip(1).collect();

    if let Some(first) = args.first().filter(|a| !a.is_empty()) {
        let bytes = first.as_bytes();
        if is_http_url(bytes) {
            open_in_brave(&first.to_string_lossy());
        } else if fs::metadata(first).is_ok() {
            let real = fs::canonicalize(first).unwrap_or_else(|_| first.into());
            open_in_brave(&format!("file://{}", real.display()));
        } else {
            let joined: Vec<&[u8]> = args.iter().map(|a| a.as_bytes()).collect();
            open_in_brave(&search_url(&joined.join(&b' ')));
        }
        exit(0);
    }

    let mut input = String::new();
    for l in config_lines() {
        input.push_str(&l);
        input.push('\n');
    }
    let fzf_output = fzf(
        &["--print-query", "--with-nth=1", "--delimiter=\\t"],
        Some(input),
    );
    let mut lines = fzf_output.lines();
    let query = lines.next().unwrap_or("");
    let selected = lines.next().unwrap_or("");
    if selected.is_empty() {
        if query.is_empty() {
            exit(0);
        }
        open_in_brave(&search_url(query.as_bytes()));
        exit(0);
    }
    // cut -f2: second tab-separated field, or the whole line without a tab.
    let mut url = selected
        .split_once('\t')
        .map_or(selected, |(_, rest)| rest.split('\t').next().unwrap_or(""))
        .to_string();
    if url.contains("%s") {
        let out = fzf(&["--print-query", "--prompt=Search query: "], None);
        let query = out.lines().next().unwrap_or("");
        if query.is_empty() {
            exit(0);
        }
        url = url.replacen("%s", &quote(query.as_bytes(), b"/"), 1);
    }
    open_in_brave(&url);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn quoting_matches_python() {
        assert_eq!(quote_plus(b"hello world/x&y"), "hello+world%2Fx%26y");
        assert_eq!(quote_plus("naïve ~_.-".as_bytes()), "na%C3%AFve+~_.-");
        assert_eq!(quote(b"a b/c?d", b"/"), "a%20b/c%3Fd");
    }

    #[test]
    fn search() {
        assert_eq!(
            search_url(b"rust lang"),
            "https://duckduckgo.com/?q=rust+lang&t=brave&ia=web"
        );
    }

    #[test]
    fn http_detection() {
        assert!(is_http_url(b"https://x"));
        assert!(is_http_url(b"http://x"));
        assert!(!is_http_url(b"ftp://x"));
        assert!(!is_http_url(b"httpx://x"));
    }

    #[test]
    fn lines_from_config() {
        let v = json!({"urls": [
            {"name": "GH", "url": "https://github.com"},
            {"name": "Search", "url": "https://x/?q=%s"},
            {"name": 3},
        ]});
        assert_eq!(
            url_lines(&v),
            vec![
                "GH\thttps://github.com",
                "Search\thttps://x/?q=%s",
                "3\tnull"
            ]
        );
        assert!(url_lines(&json!({})).is_empty());
    }
}
