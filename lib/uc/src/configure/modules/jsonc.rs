// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Edits of one `"key": "value"` line in a JSON-with-comments file, such as
//! VS Code's or opencode's settings, that keep the comments and every other
//! line as they are.

/// The string value of a top-level `"key": "value"` line.
pub fn string(text: &str, key: &str) -> Option<String> {
    let quoted = format!("\"{key}\"");
    text.lines().find_map(|l| {
        let rest = l
            .trim()
            .strip_prefix(&quoted)?
            .trim_start()
            .strip_prefix(':')?;
        let value = rest.trim().trim_end_matches(',').trim();
        Some(value.strip_prefix('"')?.strip_suffix('"')?.to_string())
    })
}

/// `text` with the `"key": …` line set to the string `value`, or one added
/// after the opening brace.
pub fn set_string(text: &str, key: &str, value: &str) -> String {
    let quoted = format!("\"{key}\"");
    let mut out: Vec<String> = Vec::new();
    let mut done = false;
    for l in text.lines() {
        let t = l.trim_start();
        if !done && t.starts_with(&quoted) {
            let indent = &l[..l.len() - t.len()];
            let comma = if t.trim_end().ends_with(',') { "," } else { "" };
            out.push(format!("{indent}{quoted}: \"{value}\"{comma}"));
            done = true;
            continue;
        }
        out.push(l.to_string());
    }
    if !done {
        return match text.find('{') {
            Some(at) if !text[at + 1..].trim_start().starts_with('}') => {
                let (head, tail) = text.split_at(at + 1);
                format!("{head}\n    {quoted}: \"{value}\",{tail}")
            }
            _ => format!("{{\n    {quoted}: \"{value}\"\n}}\n"),
        };
    }
    out.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_sets_a_string() {
        let t = "// c\n{\n  \"theme\": \"a\",\n  \"x\": 1\n}\n";
        assert_eq!(string(t, "theme").as_deref(), Some("a"));
        assert_eq!(
            set_string(t, "theme", "b"),
            "// c\n{\n  \"theme\": \"b\",\n  \"x\": 1\n}\n"
        );
        assert_eq!(
            set_string("{}", "theme", "b"),
            "{\n    \"theme\": \"b\"\n}\n"
        );
    }
}
