// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! A bit of color for the model's reply as it streams to the terminal:
//! `code`, fenced blocks and names only code has (`job_output`, `fetchUrl`,
//! `uc::flag::on`, `tools()`) in cyan, paths and file names in blue, flags and
//! environment variables in yellow, and words like error, warning and passed
//! in red, yellow and green. Titles are bold: a `# heading`, a `**bold**`
//! line, or a short line like `Changes:` that starts with a capital and ends
//! in a colon.
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
const BOLD: &str = "1";

/// A line longer than this is never a title, so it isn't held back to see.
const MAX_TITLE: usize = 80;

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

pub struct Highlighter {
    pending: String,
    /// Inside a `code` span.
    code: bool,
    /// Inside a ``` fenced block.
    fence: bool,
    /// At the start of a line, where a title can begin.
    line_start: bool,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self {
            pending: String::new(),
            code: false,
            fence: false,
            line_start: true,
        }
    }
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
            if space > 0 {
                out.push_str(&rest[..space]);
                self.line_start = rest[..space].ends_with('\n');
                rest = &rest[space..];
            }
            if rest.is_empty() {
                break;
            }
            if self.line_start {
                let end = rest.find('\n');
                let line = &rest[..end.unwrap_or(rest.len())];
                if end.is_none() && !all && line.len() <= MAX_TITLE && may_be_title(line) {
                    // Wait for the end of the line to see whether it is a title.
                    break;
                }
                self.line_start = false;
                if (end.is_some() || all) && title(line) {
                    paint(&mut out, BOLD, line);
                    rest = &rest[line.len()..];
                    continue;
                }
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

/// Whether a line that begins with `start` could still turn out a title.
fn may_be_title(start: &str) -> bool {
    if start.starts_with(['#', '*']) {
        return true;
    }
    // `Changes:` has no colon, dot or backtick before its last colon, so a
    // sentence stops waiting as soon as it has one.
    start.starts_with(|c: char| c.is_uppercase())
        && !start.trim_end_matches(':').contains([':', '.', '`'])
}

/// `## Changes`, `**Changes**`, `Changes:`.
fn title(line: &str) -> bool {
    let line = line.trim_end();
    if line.len() > MAX_TITLE || line.contains("```") {
        return false;
    }
    let hashes = line.len() - line.trim_start_matches('#').len();
    if (1..=6).contains(&hashes) {
        return line[hashes..].starts_with(' ') && !line[hashes..].trim().is_empty();
    }
    if hashes > 0 {
        return false;
    }
    if line.len() > 4 && line.starts_with("**") && line.trim_end_matches(':').ends_with("**") {
        return true;
    }
    line.len() >= 3
        && line.starts_with(|c: char| c.is_uppercase())
        && line.ends_with(':')
        && !line[..line.len() - 1].contains([':', '.', '`'])
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
    if let Some(len) = call(word) {
        paint(out, CODE, &word[..len]);
        out.push_str(&word[len..]);
        return;
    }
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
    if code_name(word) {
        return Some(CODE);
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

/// How much of `word` is a call like `tools()`, `out.push(x)` or
/// `uc::flag::on("x")`, its closing punctuation left out; none when it
/// isn't one.
fn call(word: &str) -> Option<usize> {
    let open = word.find('(')?;
    let name = &word[..open];
    if !path_of_names(name) {
        return None;
    }
    let close = word.rfind(')').filter(|close| *close > open)?;
    let rest = &word[close + 1..];
    rest.chars()
        .all(|c| ".,;:!?".contains(c))
        .then_some(close + 1)
}

/// `job_output`, `fetchUrl`, `uc::flag::on`, `self.tools`: a name only
/// code has. Not a plain word, a `Title` or an `ACRONYM`.
fn code_name(word: &str) -> bool {
    if word.contains("::") {
        return path_of_names(word);
    }
    if word.len() < 3 || !ident(word) {
        return false;
    }
    let inner = &word[1..word.len() - 1];
    // snake_case, not just a leading or trailing _.
    if inner.contains('_') && word.chars().any(|c| c.is_ascii_lowercase()) {
        return true;
    }
    // camelCase.
    word.starts_with(|c: char| c.is_ascii_lowercase())
        && word.chars().any(|c| c.is_ascii_uppercase())
}

/// `tools`, `out.push`, `uc::flag::on`: names joined by `.` or `::`.
fn path_of_names(text: &str) -> bool {
    !text.is_empty()
        && text
            .split("::")
            .flat_map(|part| part.split('.'))
            .all(ident)
}

fn ident(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
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
    fn code_names_get_colors() {
        assert_eq!(all(&["call tools()."]), "call \x1b[36mtools()\x1b[0m.");
        assert_eq!(all(&["see out.push(x),"]), "see \x1b[36mout.push(x)\x1b[0m,");
        assert_eq!(all(&["use job_output"]), "use \x1b[36mjob_output\x1b[0m");
        assert_eq!(all(&["use fetchUrl"]), "use \x1b[36mfetchUrl\x1b[0m");
        assert_eq!(all(&["use uc::flag::on"]), "use \x1b[36muc::flag::on\x1b[0m");
        for text in ["Hello there", "an API", "e.g. this", "(see above)", "_x", "it works (mostly)"] {
            assert_eq!(all(&[text]), text);
        }
    }

    #[test]
    fn titles_are_bold() {
        assert_eq!(all(&["Changes:\n- a"]), "\x1b[1mChanges:\x1b[0m\n- a");
        assert_eq!(all(&["## Do", "ne\nok"]), "\x1b[1m## Done\x1b[0m\n\x1b[92mok\x1b[0m");
        assert_eq!(all(&["x\n**Next**\n"]), "x\n\x1b[1m**Next**\x1b[0m\n");
        assert_eq!(all(&["Run it: now:\n"]), "Run it: now:\n");
        assert_eq!(all(&["not a title:\n"]), "not a title:\n");
        assert_eq!(all(&["a Title:\n"]), "a Title:\n");
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
