// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Column alignment of parsed CSV rows.

/// Character width; each char counts as 1, as in the original tool.
fn display_width(s: &str) -> usize {
    s.chars().count()
}

/// Python `str.isspace()` for `rstrip()`.
fn is_py_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

/// Render rows as aligned text. Returns `None` for no rows.
pub fn align(rows: &[Vec<String>], separator: &str, padding: i64) -> Option<String> {
    let col_count = rows.iter().map(Vec::len).max()?;

    let mut widths = vec![0usize; col_count];
    for row in rows {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = (*w).max(display_width(cell));
        }
    }

    let mut out = String::new();
    for row in rows {
        let parts: Vec<String> = (0..col_count)
            .map(|i| {
                let cell = row.get(i).map_or("", String::as_str);
                if i == col_count - 1 {
                    return cell.to_string();
                }
                let pad = widths[i] as i64 - display_width(cell) as i64 + padding;
                format!("{cell}{}", " ".repeat(pad.max(0) as usize))
            })
            .collect();
        out.push_str(parts.join(separator).trim_end_matches(is_py_space));
        out.push('\n');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(r: &[&[&str]]) -> Vec<Vec<String>> {
        r.iter()
            .map(|r| r.iter().map(|s| s.to_string()).collect())
            .collect()
    }

    #[test]
    fn aligns_columns() {
        let r = rows(&[
            &["name", "age", "city"],
            &["alice", "3"],
            &["bo", "42", "x"],
        ]);
        assert_eq!(
            align(&r, " | ", 2).unwrap(),
            "name    | age   | city\nalice   | 3     |\nbo      | 42    | x\n"
        );
    }

    #[test]
    fn negative_padding_and_empty() {
        let r = rows(&[&["ab", "c"], &["a", "d"]]);
        assert_eq!(align(&r, ",", -5).unwrap(), "ab,c\na,d\n");
        assert_eq!(align(&[], ",", 2), None);
        assert_eq!(align(&[vec![]], ",", 2).unwrap(), "\n");
    }
}
