// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Minimal port of python-dotenv's `dotenv_values`: same parsing rules
//! (quotes, escapes, `export`, comments) and `${VAR}` / `${VAR:-default}`
//! interpolation, with file values taking precedence over the process env.

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

fn re(pattern: &str) -> Regex {
    Regex::new(&format!("^(?:{pattern})")).unwrap()
}

static MULTILINE_WS: LazyLock<Regex> = LazyLock::new(|| re(r"\s*"));
static WS: LazyLock<Regex> = LazyLock::new(|| re(r"[^\S\r\n]*"));
static EXPORT: LazyLock<Regex> = LazyLock::new(|| re(r"(?:export[^\S\r\n]+)?"));
static SQ_KEY: LazyLock<Regex> = LazyLock::new(|| re(r"'([^']+)'"));
static UQ_KEY: LazyLock<Regex> = LazyLock::new(|| re(r"([^=#\s]+)"));
static EQUAL: LazyLock<Regex> = LazyLock::new(|| re(r"(=[^\S\r\n]*)"));
static SQ_VALUE: LazyLock<Regex> = LazyLock::new(|| re(r"(?s)'((?:\\.|[^'\\])*)'"));
static DQ_VALUE: LazyLock<Regex> = LazyLock::new(|| re(r#"(?s)"((?:\\.|[^"\\])*)""#));
static UQ_VALUE: LazyLock<Regex> = LazyLock::new(|| re(r"([^\r\n]*)"));
static COMMENT: LazyLock<Regex> = LazyLock::new(|| re(r"(?:[^\S\r\n]*#[^\r\n]*)?"));
static EOL: LazyLock<Regex> = LazyLock::new(|| re(r"[^\S\r\n]*(?:\r\n|\n|\r|$)"));
static REST_OF_LINE: LazyLock<Regex> = LazyLock::new(|| re(r"[^\r\n]*(?:\r|\n|\r\n)?"));
static INLINE_COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+#.*").unwrap());
static VARIABLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{([^}:]*)(?::-([^}]*))?\}").unwrap());

struct Reader<'a> {
    s: &'a str,
    pos: usize,
}

impl<'a> Reader<'a> {
    fn peek(&self) -> Option<char> {
        self.s[self.pos..].chars().next()
    }

    /// Match `regex` anchored at the current position; returns group 1 (or "").
    fn read(&mut self, regex: &Regex) -> Option<&'a str> {
        let caps = regex.captures(&self.s[self.pos..])?;
        let whole = caps.get(0)?;
        let group = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        self.pos += whole.end();
        Some(group)
    }
}

fn decode_double(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            let decoded = match chars.peek() {
                Some('\\') => Some('\\'),
                Some('\'') => Some('\''),
                Some('"') => Some('"'),
                Some('a') => Some('\x07'),
                Some('b') => Some('\x08'),
                Some('f') => Some('\x0c'),
                Some('n') => Some('\n'),
                Some('r') => Some('\r'),
                Some('t') => Some('\t'),
                Some('v') => Some('\x0b'),
                _ => None,
            };
            if let Some(d) = decoded {
                chars.next();
                out.push(d);
                continue;
            }
        }
        out.push(c);
    }
    out
}

fn decode_single(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(&n) = chars.peek()
            && (n == '\\' || n == '\'')
        {
            chars.next();
            out.push(n);
            continue;
        }
        out.push(c);
    }
    out
}

fn parse_value(r: &mut Reader) -> Option<String> {
    match r.peek() {
        Some('\'') => r.read(&SQ_VALUE).map(decode_single),
        Some('"') => r.read(&DQ_VALUE).map(decode_double),
        None | Some('\n') | Some('\r') => Some(String::new()),
        _ => {
            let part = r.read(&UQ_VALUE)?;
            Some(INLINE_COMMENT.replace(part, "").trim_end().to_string())
        }
    }
}

/// Parse one binding; `Err(())` means the statement could not be parsed.
fn parse_binding(r: &mut Reader) -> Result<Option<(String, Option<String>)>, ()> {
    r.read(&MULTILINE_WS).ok_or(())?;
    if r.pos >= r.s.len() {
        return Ok(None);
    }
    r.read(&EXPORT).ok_or(())?;
    let key = match r.peek() {
        Some('#') => None,
        Some('\'') => Some(r.read(&SQ_KEY).ok_or(())?.to_string()),
        _ => Some(r.read(&UQ_KEY).ok_or(())?.to_string()),
    };
    r.read(&WS).ok_or(())?;
    let value = if r.peek() == Some('=') {
        r.read(&EQUAL).ok_or(())?;
        Some(parse_value(r).ok_or(())?)
    } else {
        None
    };
    r.read(&COMMENT).ok_or(())?;
    r.read(&EOL).ok_or(())?;
    Ok(key.map(|k| (k, value)))
}

/// Raw bindings in file order (no interpolation).
pub fn parse(text: &str) -> Vec<(String, Option<String>)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut r = Reader { s: text, pos: 0 };
    let mut out = Vec::new();
    while r.pos < r.s.len() {
        let start = r.pos;
        match parse_binding(&mut r) {
            Ok(Some(b)) => out.push(b),
            Ok(None) => {}
            Err(()) => {
                r.read(&REST_OF_LINE);
            }
        }
        if r.pos == start {
            break;
        }
    }
    out
}

fn interpolate(value: &str, env: &HashMap<String, Option<String>>) -> String {
    VARIABLE
        .replace_all(value, |c: &regex::Captures| {
            let default = c.get(2).map(|m| m.as_str()).unwrap_or("");
            match env.get(&c[1]) {
                Some(Some(v)) => v.clone(),
                Some(None) => String::new(),
                None => default.to_string(),
            }
        })
        .into_owned()
}

/// `dotenv_values(text)`: keys mapped to interpolated values (None for bare keys).
pub fn values_from_str(text: &str) -> HashMap<String, Option<String>> {
    let mut resolved: HashMap<String, Option<String>> = HashMap::new();
    for (key, value) in parse(text) {
        let result = value.map(|v| {
            let mut env: HashMap<String, Option<String>> =
                std::env::vars().map(|(k, v)| (k, Some(v))).collect();
            env.extend(resolved.iter().map(|(k, v)| (k.clone(), v.clone())));
            interpolate(&v, &env)
        });
        resolved.insert(key, result);
    }
    resolved
}

/// `dotenv_values(file)`; a missing or unreadable file yields no values.
pub fn values(file: &Path) -> HashMap<String, Option<String>> {
    std::fs::read(file)
        .map(|b| values_from_str(&String::from_utf8_lossy(&b)))
        .unwrap_or_default()
}

/// `read_env`: like `values` but without keys that have no value.
pub fn read_env(file: &Path) -> HashMap<String, String> {
    values(file)
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(text: &str, key: &str) -> Option<Option<String>> {
        values_from_str(text).get(key).cloned()
    }

    #[test]
    fn basic() {
        let text = "# comment\nA=1\nexport B = two \nC=x #note\nD\nE=\n\n";
        let v = values_from_str(text);
        assert_eq!(v["A"].as_deref(), Some("1"));
        assert_eq!(v["B"].as_deref(), Some("two"));
        assert_eq!(v["C"].as_deref(), Some("x"));
        assert_eq!(v["D"], None);
        assert_eq!(v["E"].as_deref(), Some(""));
        assert_eq!(v.len(), 5);
    }

    #[test]
    fn quotes() {
        assert_eq!(get("A='it\\'s ${X}'", "A"), Some(Some("it's ".to_string())));
        assert_eq!(get("A=\"a\\nb\" # c", "A"), Some(Some("a\nb".into())));
        assert_eq!(get("A=\"multi\nline\"\nB=2", "B"), Some(Some("2".into())));
        assert_eq!(get("A=a#b", "A"), Some(Some("a#b".into())));
    }

    #[test]
    fn interpolation() {
        let v = values_from_str("A=1\nB=${A}-${MISSING_PLAT_TEST:-d}-${MISSING_PLAT_TEST}\n");
        assert_eq!(v["B"].as_deref(), Some("1-d-"));
    }

    #[test]
    fn invalid_lines_skipped() {
        let v = values_from_str("=bad\nA=\"unterminated\nB=2\n");
        assert_eq!(v.get("B").cloned(), Some(Some("2".into())));
    }

    /// Expected values produced by python-dotenv 1.x for the same input.
    #[test]
    fn matches_python_dotenv() {
        let text = "# c\nexport A=1\nB = two words  # trailing\nC='sq \\'x\\' ${A}'\n\
D=\"dq\\t${A}\\n${NOPE_PLAT_TEST:-def}\"\nE\n=bad\nF=\"multi\nline\" \nG=\"bad\" junk\n\
H=${HOME_X_PLAT_TEST:-${A}}\n'K'=v\nI=a#b\nJ=\n";
        let v = values_from_str(text);
        let s = |k: &str| v[k].as_deref();
        assert_eq!(s("A"), Some("1"));
        assert_eq!(s("B"), Some("two words"));
        assert_eq!(s("C"), Some("sq 'x' 1"));
        assert_eq!(s("D"), Some("dq\t1\ndef"));
        assert_eq!(s("E"), None);
        assert_eq!(s("F"), Some("multi\nline"));
        assert_eq!(s("H"), Some("${A}"));
        assert_eq!(s("I"), Some("a#b"));
        assert_eq!(s("J"), Some(""));
        assert_eq!(s("K"), Some("v"));
        assert!(!v.contains_key("G"));
        assert_eq!(v.len(), 10);
    }
}
