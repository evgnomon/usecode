//! Adds a key to a nested mapping in a YAML file without reformatting
//! the rest of it.
//!
//! hosts.yml is a hand-maintained Ansible file: its comments explain the
//! mesh, and losing them to a round-trip through a YAML parser would
//! cost more than the edit is worth. Since the only change portman ever
//! makes is "put one more host in a group", it is done on the text
//! itself - the line is inserted where it belongs and every other byte
//! of the file is left exactly as it was.

use crate::error::{Error, Result};

/// Insert `leaf: {}` under the mapping at `path`, creating any level of
/// `path` that does not exist yet. Returns the new file contents.
pub fn insert_key(text: &str, path: &[&str], leaf: &str) -> Result<String> {
    let ends_with_newline = text.is_empty() || text.ends_with('\n');
    let mut lines: Vec<String> = text.lines().map(String::from).collect();

    // Each step either finishes the job or makes the document one level
    // closer to having the path, so a rescan after every change keeps
    // the line arithmetic honest without tracking shifting indices.
    loop {
        match locate(&lines, path)? {
            Located::Found(block) => {
                let at = insert_point(&lines, &block);
                lines.insert(at, format!("{}{leaf}: {{}}", " ".repeat(block.indent)));
                break;
            }
            Located::Inline { line, indent, key } => {
                // `hosts: {}` - an empty flow mapping would render the
                // new entry inline; make it a block so hosts stay one
                // per line.
                lines[line] = format!("{}{key}:", " ".repeat(indent));
            }
            Located::Missing { block, key } => {
                let at = insert_point(&lines, &block);
                lines.insert(at, format!("{}{key}:", " ".repeat(block.indent)));
            }
        }
    }

    let mut out = lines.join("\n");
    if ends_with_newline {
        out.push('\n');
    }
    Ok(out)
}

/// A block mapping: the half-open line range holding its entries, and
/// the column its keys start at.
#[derive(Clone, Debug)]
struct Block {
    start: usize,
    end: usize,
    indent: usize,
}

enum Located {
    /// Every level of the path exists; here is the innermost mapping.
    Found(Block),
    /// A level is written as an inline `{}` and has to become a block.
    Inline {
        line: usize,
        indent: usize,
        key: String,
    },
    /// `key` is missing from the mapping `block`.
    Missing { block: Block, key: String },
}

fn locate(lines: &[String], path: &[&str]) -> Result<Located> {
    let mut block = Block {
        start: 0,
        end: lines.len(),
        indent: top_indent(lines, 0, lines.len()),
    };

    for key in path {
        let Some(found) = find_key(lines, &block, key) else {
            return Ok(Located::Missing {
                block,
                key: key.to_string(),
            });
        };

        let value = value_of(&lines[found.line]);
        if !value.is_empty() {
            if value == "{}" || value == "{ }" {
                return Ok(Located::Inline {
                    line: found.line,
                    indent: found.indent,
                    key: key.to_string(),
                });
            }
            return Err(Error(format!("{key:?} is not a mapping")));
        }

        block = children_of(lines, &block, &found);
    }

    Ok(Located::Found(block))
}

struct KeyPos {
    line: usize,
    indent: usize,
}

/// The entries of the mapping opened by the key at `found`.
fn children_of(lines: &[String], parent: &Block, found: &KeyPos) -> Block {
    let start = found.line + 1;
    let mut end = start;
    while end < parent.end {
        let line = &lines[end];
        if !line.trim().is_empty() && indent_of(line) <= found.indent {
            break;
        }
        end += 1;
    }
    Block {
        start,
        end,
        indent: top_indent(lines, start, end).max(found.indent + 2),
    }
}

/// The indentation shared by the entries of a block, or 0 if it has none
/// yet (the caller widens that to the parent's indent plus two).
fn top_indent(lines: &[String], start: usize, end: usize) -> usize {
    lines[start..end]
        .iter()
        .find(|l| is_entry(l))
        .map(|l| indent_of(l))
        .unwrap_or(0)
}

fn find_key(lines: &[String], block: &Block, key: &str) -> Option<KeyPos> {
    (block.start..block.end)
        .filter(|&i| is_entry(&lines[i]) && indent_of(&lines[i]) == block.indent)
        .find(|&i| key_of(&lines[i]).as_deref() == Some(key))
        .map(|i| KeyPos {
            line: i,
            indent: block.indent,
        })
}

/// Where a new entry goes: at the end of the block, before any blank
/// lines that separate it from whatever follows.
fn insert_point(lines: &[String], block: &Block) -> usize {
    let mut at = block.end.min(lines.len());
    while at > block.start && lines[at - 1].trim().is_empty() {
        at -= 1;
    }
    at
}

/// Whether a line carries structure, as opposed to being blank, a
/// comment, or a document marker.
fn is_entry(line: &str) -> bool {
    let t = line.trim();
    !(t.is_empty() || t.starts_with('#') || t == "---" || t == "...")
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The key a mapping entry declares, if it is one.
fn key_of(line: &str) -> Option<String> {
    let t = line.trim_start();
    if t.starts_with('-') {
        return None; // a sequence item, not a mapping key
    }
    let (key, _) = t.split_once(':')?;
    if key.is_empty() {
        return None;
    }
    Some(key.trim().trim_matches(['"', '\'']).to_string())
}

/// Whatever follows the colon on a mapping entry's line, comment
/// stripped.
fn value_of(line: &str) -> String {
    let after = match line.split_once(':') {
        Some((_, rest)) => rest,
        None => return String::new(),
    };
    match after.trim().split_once(" #") {
        Some((v, _)) => v.trim().to_string(),
        None => after.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: [&str; 4] = ["all", "children", "portman", "hosts"];

    fn insert(text: &str) -> String {
        insert_key(text, &PATH, "laptop").unwrap()
    }

    #[test]
    fn appends_to_a_populated_group() {
        let got = insert(
            "---\n# keep me\nall:\n  children:\n    portman:\n      hosts:\n        edge: {}\n",
        );
        assert_eq!(
            got,
            "---\n# keep me\nall:\n  children:\n    portman:\n      hosts:\n        edge: {}\n        laptop: {}\n"
        );
    }

    #[test]
    fn expands_an_empty_flow_mapping() {
        let got = insert("all:\n  children:\n    portman:\n      hosts: {}\n");
        assert_eq!(
            got,
            "all:\n  children:\n    portman:\n      hosts:\n        laptop: {}\n"
        );
    }

    #[test]
    fn fills_in_a_null_hosts_key() {
        let got = insert("all:\n  children:\n    portman:\n      hosts:\n");
        assert_eq!(
            got,
            "all:\n  children:\n    portman:\n      hosts:\n        laptop: {}\n"
        );
    }

    #[test]
    fn creates_the_levels_that_are_missing() {
        let got = insert("all:\n  children:\n    portman:\n");
        assert_eq!(
            got,
            "all:\n  children:\n    portman:\n      hosts:\n        laptop: {}\n"
        );

        let got = insert("all:\n  hosts: {}\n");
        assert_eq!(
            got,
            "all:\n  hosts: {}\n  children:\n    portman:\n      hosts:\n        laptop: {}\n"
        );
    }

    #[test]
    fn keeps_what_follows_the_group() {
        let got = insert(
            "all:\n  children:\n    portman:\n      hosts:\n        edge: {}\n\n    other:\n      hosts:\n        box: {}\n",
        );
        assert!(
            got.contains("        edge: {}\n        laptop: {}\n\n    other:"),
            "{got}"
        );
        assert!(got.contains("        box: {}"), "{got}");
    }

    #[test]
    fn refuses_a_level_that_is_not_a_mapping() {
        let err = insert_key("all:\n  children: nope\n", &PATH, "laptop").unwrap_err();
        assert!(err.to_string().contains("not a mapping"), "{err}");
    }
}
