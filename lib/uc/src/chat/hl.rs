// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! A bit of color for the model's reply as it streams to the terminal:
//! `code` and fenced blocks in cyan, paths and file names in blue, flags and
//! environment variables in yellow, and words like error, warning and passed
//! in red, yellow and green.
//!
//! The reply comes in pieces that can end in the middle of a word, so the
//! last word waits for the next piece (or the end of the reply) before it is
//! colored and printed. Every colored run is reset right after it, so
//! nothing printed in between ever picks up a color.

const CODE: &str = "36";
const PATH: &str = "94";
const FLAG: &str = "33";
const BAD: &str = "91";
const WARN: &str = "93";
const GOOD: &str = "92";
const DIM: &str = "2";

/// Above this many bytes without a space, a word is printed as it is rather
/// than waiting longer.
const MAX_WORD: usize = 256;

const EXTENSIONS: &[&str] = &[
    "rs", "md", "toml", "yaml", "yml", "json", "jsonc", "sh", "bash", "py", "go", "js", "ts",
    "tsx", "jsx", "html", "css", "txt", "lock", "mk", "service", "conf", "deb", "c", "h", "cc",
    "cpp", "hpp", "sql", "lua", "vim", "env", "ini", "cfg", "log", "pem", "crt", "key", "tar",
    "gz", "zip", "png", "svg", "nix", "zig", "rb", "java", "kt", "swift", "proto",
];
const BAD_WORDS: &[&str] = &[
    "error", "errors", "failed", "fails", "failing", "failure", "failures", "fail", "panic",
    "panicked", "broken",
];
const WARN_WORDS: &[&str] = &["warning", "warnings", "deprecated"];
const GOOD_WORDS: &[&str] = &["ok", "passed", "passes", "passing", "succeeded", "success"];

#[derive(Default)]
pub struct Highlighter {
    pending: String,
    /// Inside a `code` span.
    code: bool,
    /// Inside a ``` fenced block.
    fence: bool,
}

impl Highlighter {
    /// The piece colored, less a word it may end in the middle of.
    pub fn push(&mut self, text: &str) -> String {
        self.pending.push_str(text);
        self.take(false)
    }

    /// The rest, at the end of the reply.
    pub fn finish(&mut self) -> String {
        let out = self.take(true);
        *self = Self::default();
        out
    }

    fn take(&mut self, all: bool) -> String {
        let pending = std::mem::take(&mut self.pending);
        let mut rest = pending.as_str();
        let mut out = String::new();
        while !rest.is_empty() {
            if self.fence {
                match rest.find("```") {
                    Some(i) => {
                        paint(&mut out, CODE, &rest[..i]);
                        paint(&mut out, DIM, "```");
                        rest = &rest[i + 3..];
                        self.fence = false;
                    }
                    None => {
                        // Keep a trailing ` or `` that may be the start of the fence's end.
                        let keep = if all { 0 } else { trailing_ticks(rest) };
                        paint(&mut out, CODE, &rest[..rest.len() - keep]);
                        rest = &rest[rest.len() - keep..];
                        break;
                    }
                }
                continue;
            }
            if self.code {
                match rest.find(['`', '\n']) {
                    Some(i) if rest[i..].starts_with('`') => {
                        paint(&mut out, CODE, &rest[..=i]);
                        rest = &rest[i + 1..];
                        self.code = false;
                    }
                    // A span doesn't run past the end of its line.
                    Some(i) => {
                        paint(&mut out, CODE, &rest[..i]);
                        rest = &rest[i..];
                        self.code = false;
                    }
                    None => {
                        paint(&mut out, CODE, rest);
                        rest = "";
                    }
                }
                continue;
            }
            let space = rest.len() - rest.trim_start().len();
            out.push_str(&rest[..space]);
            rest = &rest[space..];
            if rest.is_empty() {
                break;
            }
            if rest.starts_with('`') {
                if rest.starts_with("```") {
                    paint(&mut out, DIM, "```");
                    rest = &rest[3..];
                    self.fence = true;
                } else if !all && trailing_ticks(rest) == rest.len() {
                    // ` or ``: wait to see whether a fence starts here.
                    break;
                } else {
                    paint(&mut out, CODE, "`");
                    rest = &rest[1..];
                    self.code = true;
                }
                continue;
            }
            match rest.find(|c: char| c.is_whitespace() || c == '`') {
                Some(end) => {
                    word(&mut out, &rest[..end]);
                    rest = &rest[end..];
                }
                None if all || rest.len() > MAX_WORD => {
                    word(&mut out, rest);
                    rest = "";
                }
                None => break,
            }
        }
        self.pending = rest.to_string();
        out
    }
}

/// How many backticks, at most two, `text` ends in.
fn trailing_ticks(text: &str) -> usize {
    text.bytes()
        .rev()
        .take(2)
        .take_while(|b| *b == b'`')
        .count()
}

fn paint(out: &mut String, color: &str, text: &str) {
    if !text.is_empty() {
        out.push_str(&format!("\x1b[{color}m{text}\x1b[0m"));
    }
}

/// A word with no space in it, colored by what it is, its punctuation left
/// as it is.
fn word(out: &mut String, word: &str) {
    let core = word.trim_start_matches(['(', '[', '{', '"', '\'', '<']);
    let lead = &word[..word.len() - core.len()];
    let core = core.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}', '"', '\'', '>']);
    let trail = &word[lead.len() + core.len()..];
    out.push_str(lead);
    match kind(core) {
        Some(color) => paint(out, color, core),
        None => out.push_str(core),
    }
    out.push_str(trail);
}

fn kind(word: &str) -> Option<&'static str> {
    if word.is_empty() {
        return None;
    }
    let lower = word.to_lowercase();
    if BAD_WORDS.contains(&lower.as_str()) {
        return Some(BAD);
    }
    if WARN_WORDS.contains(&lower.as_str()) {
        return Some(WARN);
    }
    if GOOD_WORDS.contains(&lower.as_str()) {
        return Some(GOOD);
    }
    if flag(word) || env_var(word) {
        return Some(FLAG);
    }
    if path(word) {
        return Some(PATH);
    }
    None
}

/// `--no-shell`, `-y`, `-rf`.
fn flag(word: &str) -> bool {
    let name = match word.strip_prefix("--") {
        Some(name) => name,
        None => match word.strip_prefix('-') {
            Some(name) if name.len() <= 3 => name,
            _ => return false,
        },
    };
    let name = name.split('=').next().unwrap_or_default();
    name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// `UC_NO_SHELL`, `$HOME`.
fn env_var(word: &str) -> bool {
    let (dollar, name) = match word.strip_prefix('$') {
        Some(name) => (true, name.trim_start_matches('{').trim_end_matches('}')),
        None => (false, word),
    };
    name.len() >= 2
        && name.starts_with(|c: char| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && (dollar || name.contains('_'))
}

/// `lib/uc/src/chat.rs`, `~/.config/usecode`, `/etc/hosts`, `Cargo.toml`,
/// a URL; not `and/or`.
fn path(word: &str) -> bool {
    if word.starts_with("http://") || word.starts_with("https://") {
        return true;
    }
    let pathy = |c: char| c.is_alphanumeric() || "/._-~+@:%=".contains(c);
    if !word.chars().all(pathy) || !word.chars().any(char::is_alphanumeric) {
        return false;
    }
    let name = word.rsplit('/').next().unwrap_or_default();
    let ext = name
        .rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty() || name.starts_with('.'))
        .map(|(_, ext)| ext.to_lowercase());
    if ext.is_some_and(|ext| EXTENSIONS.contains(&ext.as_str())) {
        return true;
    }
    // A dotfile: .gitignore, .env.
    if name.len() > 2
        && name.starts_with('.')
        && name[1..].starts_with(|c: char| c.is_ascii_alphabetic())
    {
        return true;
    }
    let slashes = word.matches('/').count();
    match slashes {
        0 => false,
        1 => {
            ["/", "~/", "./", "../"].iter().any(|p| word.starts_with(p))
                || word.ends_with('/')
                || !word.chars().all(|c| c.is_alphabetic() || c == '/')
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> String {
        let mut out = String::new();
        let mut esc = false;
        for c in text.chars() {
            match (esc, c) {
                (false, '\x1b') => esc = true,
                (true, 'm') => esc = false,
                (true, _) => {}
                (false, c) => out.push(c),
            }
        }
        out
    }

    fn all(pieces: &[&str]) -> String {
        let mut hl = Highlighter::default();
        let mut out: String = pieces.iter().map(|p| hl.push(p)).collect();
        out += &hl.finish();
        out
    }

    #[test]
    fn text_is_kept_whatever_the_pieces() {
        let text = "Run `make install` in lib/uc/src, see AGENTS.md.\n```bash\nls -la\n```\nAll tests passed (3 failed).";
        for size in 1..text.len() {
            let pieces: Vec<&str> = text
                .as_bytes()
                .chunks(size)
                .map(|c| std::str::from_utf8(c).unwrap())
                .collect();
            assert_eq!(plain(&all(&pieces)), text, "pieces of {size}");
        }
    }

    #[test]
    fn keywords_get_colors() {
        assert_eq!(all(&["it failed."]), "it \x1b[91mfailed\x1b[0m.");
        assert_eq!(
            all(&["see (Cargo.toml)"]),
            "see (\x1b[94mCargo.toml\x1b[0m)"
        );
        assert_eq!(all(&["use --no-shell"]), "use \x1b[33m--no-shell\x1b[0m");
        assert_eq!(all(&["set UC_NO_SHELL"]), "set \x1b[33mUC_NO_SHELL\x1b[0m");
        assert_eq!(all(&["and/or - Hello"]), "and/or - Hello");
        assert_eq!(all(&["a `x` b"]), "a \x1b[36m`\x1b[0m\x1b[36mx`\x1b[0m b");
    }

    #[test]
    fn a_word_waits_for_its_end() {
        let mut hl = Highlighter::default();
        assert_eq!(hl.push("it fai"), "it ");
        assert_eq!(hl.push("led "), "\x1b[91mfailed\x1b[0m ");
    }

    #[test]
    fn paths() {
        for p in [
            "lib/uc/src/chat.rs",
            "~/.config/usecode",
            "/etc/hosts",
            "lib/uc/",
            "src/",
            ".gitignore",
            "https://x.dev",
        ] {
            assert!(path(p), "{p}");
        }
        for p in ["and/or", "e.g", "1.5", "Hello", "/"] {
            assert!(!path(p), "{p}");
        }
    }
}
