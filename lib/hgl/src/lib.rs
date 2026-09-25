// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! hgl — detect and insert the project license header.
//!
//! Every first-party text file carries two lines, written in the comment
//! syntax of its format:
//!
//! ```text
//! License-Identifier: HGL
//! Copyright (C) The Usecode Authors (see AUTHORS)
//! ```
//!
//! Formats that cannot hold a comment (strict JSON, lock files, keys,
//! checksums, license texts) and binary files are skipped.

pub const LICENSE: &str = "License-Identifier: HGL";
pub const COPYRIGHT: &str = "Copyright (C) The Usecode Authors (see AUTHORS)";

/// Number of leading lines searched for an existing header.
const HEADER_WINDOW: usize = 8;
/// Number of leading lines searched for legacy headers to replace.
const LEGACY_WINDOW: usize = 5;
/// Number of leading bytes inspected to tell text from binary, as `grep -I`.
const BINARY_WINDOW: usize = 8000;

/// Comment syntax used to write the header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// `# …` — shell, Python, YAML, TOML, INI, Makefile, Dockerfile, …
    Hash,
    /// `// …` — Rust, JavaScript, SCSS, JSONC.
    Slash,
    /// `/* … */` — CSS.
    Block,
    /// `<!-- … -->` — Markdown, HTML, SVG, XML.
    Html,
    /// `{# … -#}` — Jinja templates; stripped from the rendered output.
    Jinja,
    /// `" …` — Vim script.
    Vim,
    /// `.. …` — reStructuredText.
    Rst,
    /// `.\" …` — roff man pages.
    Roff,
    /// `@REM …` — Windows batch files.
    Bat,
    /// Bare lines — plain-text READMEs.
    Text,
}

impl Style {
    /// Line prefix for single-line styles.
    fn prefix(self) -> &'static str {
        match self {
            Style::Hash => "#",
            Style::Slash => "//",
            Style::Vim => "\"",
            Style::Rst => "..",
            Style::Roff => ".\\\"",
            Style::Bat => "@REM",
            Style::Block | Style::Html | Style::Jinja | Style::Text => "",
        }
    }

    /// Header lines in this style.
    pub fn header(self) -> Vec<String> {
        let block = |open: &str, lead: &str, close: &str| {
            vec![
                open.to_string(),
                format!("{lead}{LICENSE}"),
                format!("{lead}{COPYRIGHT}"),
                close.to_string(),
            ]
        };
        match self {
            Style::Block => block("/*", " * ", " */"),
            Style::Html => block("<!--", "", "-->"),
            Style::Jinja => block("{#", "", "-#}"),
            Style::Text => vec![LICENSE.to_string(), COPYRIGHT.to_string()],
            _ => {
                let p = self.prefix();
                vec![format!("{p} {LICENSE}"), format!("{p} {COPYRIGHT}")]
            }
        }
    }
}

/// Basenames that never get a header: license texts, generated files,
/// checksums, keys, data and files whose consumers reject comments.
const SKIP_NAMES: &[&str] = &[
    "COPYING",
    "DCO",
    "AUTHORS",
    "SHA512SUMS",
    ".python-version",
    ".keep",
    "extensions.txt",
    "nouser",
];

/// Extensions that never get a header.
const SKIP_EXTS: &[&str] = &["lock", "asc", "pub", "gpg", "gz", "csv"];

/// JSON files whose consumers accept comments.
fn is_jsonc(path: &str, base: &str) -> bool {
    path.contains("Code/User/")
        || base.starts_with("devcontainer")
        || base == ".oxlintrc.json"
        || base == "coc-settings.json"
}

/// Choose the header style for `path` with `content`, or `None` if the file
/// must not carry a header.
pub fn style_for(path: &str, content: &str) -> Option<Style> {
    let base = path.rsplit('/').next().unwrap_or(path);
    let ext = base
        .rfind('.')
        .filter(|&i| i > 0)
        .map_or("", |i| &base[i + 1..]);

    if SKIP_NAMES.contains(&base) || SKIP_EXTS.contains(&ext) {
        return None;
    }
    if content.starts_with("#!") {
        return Some(Style::Hash);
    }
    Some(match ext {
        "json" if is_jsonc(path, base) => Style::Slash,
        "json" => return None,
        "j2" | "jinja2" => Style::Jinja,
        "rs" | "js" | "jsx" | "c" | "h" | "scss" | "jsonc" => Style::Slash,
        "css" => Style::Block,
        "md" | "html" | "svg" | "xml" => Style::Html,
        "vim" => Style::Vim,
        "rst" => Style::Rst,
        "bat" => Style::Bat,
        e if e.len() == 1 && e.as_bytes()[0].is_ascii_digit() && e != "0" => Style::Roff,
        _ => match base {
            ".vimrc" | ".ideavimrc" => Style::Vim,
            "README" => Style::Text,
            _ => Style::Hash,
        },
    })
}

/// True if `bytes` look like text: free of NUL bytes. Empty files are text.
pub fn is_text(bytes: &[u8]) -> bool {
    !bytes[..bytes.len().min(BINARY_WINDOW)].contains(&0)
}

/// True if `content` already carries the header.
pub fn has_header(content: &str) -> bool {
    content
        .lines()
        .take(HEADER_WINDOW)
        .any(|l| l.contains(LICENSE))
}

/// Legacy header lines replaced by the current header.
fn is_legacy(line: &str) -> bool {
    (line.starts_with("# Copyright (C) ") && line.ends_with("All rights reserved."))
        || line.starts_with("# License: HGL General License")
        || line == "#SPDX-License-Identifier: MIT-0"
}

/// First lines that must stay first: interpreter lines, cloud-init markers,
/// XML declarations and HTML doctypes.
fn is_pinned(line: &str, style: Style) -> bool {
    line.starts_with("#!")
        || line.starts_with("#cloud-config")
        || line.starts_with("<?xml")
        || (style == Style::Html && line.to_ascii_lowercase().starts_with("<!doctype"))
}

/// Return `content` with the header inserted in `style`. Legacy headers in
/// the first lines are removed. The result always ends with a newline.
pub fn apply(content: &str, style: Style) -> String {
    let mut lines: Vec<&str> = content.split('\n').collect();
    if content.is_empty() || content.ends_with('\n') {
        lines.pop();
    }
    let mut i = 0;
    lines.retain(|l| {
        i += 1;
        i > LEGACY_WINDOW || !is_legacy(l)
    });

    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 6);
    let mut rest = lines.as_slice();
    if let Some((first, tail)) = rest.split_first()
        && is_pinned(first, style)
    {
        out.push(first.to_string());
        rest = tail;
    }
    out.extend(style.header());
    if let Some(next) = rest.first()
        && !next.is_empty()
        && *next != style.prefix()
        && style != Style::Roff
    {
        out.push(String::new());
    }
    out.extend(rest.iter().map(|l| l.to_string()));

    let mut s = out.join("\n");
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_by_path() {
        assert_eq!(style_for("src/main.rs", ""), Some(Style::Slash));
        assert_eq!(style_for("a/b.py", ""), Some(Style::Hash));
        assert_eq!(style_for("Makefile", ""), Some(Style::Hash));
        assert_eq!(style_for("x/app.css", ""), Some(Style::Block));
        assert_eq!(style_for("README.md", ""), Some(Style::Html));
        assert_eq!(style_for("t/base.html.jinja2", ""), Some(Style::Jinja));
        assert_eq!(style_for("lib/vim/.vimrc", ""), Some(Style::Vim));
        assert_eq!(style_for("man1/mkdeb.1", ""), Some(Style::Roff));
        assert_eq!(style_for("docs/make.bat", ""), Some(Style::Bat));
        assert_eq!(style_for("migrations/README", ""), Some(Style::Text));
        assert_eq!(
            style_for(".config/Code/User/settings.json", ""),
            Some(Style::Slash)
        );
        assert_eq!(
            style_for(".devcontainer/devcontainer.json", ""),
            Some(Style::Slash)
        );
        assert_eq!(style_for("bin/tool", "#!/bin/sh\n"), Some(Style::Hash));
        assert_eq!(style_for("t/run.sh.j2", "#!/bin/sh\n"), Some(Style::Hash));
    }

    #[test]
    fn skipped_files() {
        for p in [
            "COPYING",
            "lib/roles/COPYING",
            "Cargo.lock",
            "package.json",
            "keys/docker.asc",
            "data/cash.csv",
            ".python-version",
        ] {
            assert_eq!(style_for(p, ""), None, "{p}");
        }
    }

    #[test]
    fn inserts_after_shebang() {
        let out = apply("#!/bin/sh\necho hi\n", Style::Hash);
        assert_eq!(
            out,
            format!("#!/bin/sh\n# {LICENSE}\n# {COPYRIGHT}\n\necho hi\n")
        );
    }

    #[test]
    fn inserts_after_doctype() {
        let out = apply("<!doctype html>\n<html>\n", Style::Html);
        assert!(out.starts_with("<!doctype html>\n<!--\n"));
    }

    #[test]
    fn keeps_cloud_config_first() {
        let out = apply("#cloud-config\nusers: []\n", Style::Hash);
        assert!(out.starts_with(&format!("#cloud-config\n# {LICENSE}\n")));
    }

    #[test]
    fn replaces_legacy_header() {
        let src = "# Copyright (C) <2025> Someone. All rights reserved.\n\
                   # License: HGL General License <http://evgnomon.org/docs/hgl>\n\
                   #\n# body\n";
        let out = apply(src, Style::Hash);
        assert_eq!(out, format!("# {LICENSE}\n# {COPYRIGHT}\n#\n# body\n"));
    }

    #[test]
    fn replaces_spdx_stub() {
        let out = apply("#SPDX-License-Identifier: MIT-0\n---\n", Style::Hash);
        assert_eq!(out, format!("# {LICENSE}\n# {COPYRIGHT}\n\n---\n"));
    }

    #[test]
    fn roff_has_no_blank_line() {
        let out = apply(".TH X 1\n", Style::Roff);
        assert_eq!(
            out,
            format!(".\\\" {LICENSE}\n.\\\" {COPYRIGHT}\n.TH X 1\n")
        );
    }

    #[test]
    fn block_styles() {
        assert_eq!(
            apply("a {}\n", Style::Block),
            format!("/*\n * {LICENSE}\n * {COPYRIGHT}\n */\n\na {{}}\n")
        );
        assert!(apply("x\n", Style::Jinja).starts_with("{#\n"));
    }

    #[test]
    fn empty_file() {
        assert_eq!(
            apply("", Style::Hash),
            format!("# {LICENSE}\n# {COPYRIGHT}\n")
        );
    }

    #[test]
    fn result_has_header() {
        let once = apply("fn main() {}", Style::Slash);
        assert!(has_header(&once));
        assert!(once.ends_with('\n'));
    }

    #[test]
    fn detects_binary() {
        assert!(is_text(b"hello"));
        assert!(is_text(b""));
        assert!(!is_text(b"\x00\x01"));
    }
}
