// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! CSV reader mirroring Python's `_csv.reader` fed with `str.splitlines()`
//! output (so line breaks inside quoted fields are dropped, as in the
//! original tool).

use crate::sniff::Dialect;

/// Python's default `csv.field_size_limit()`.
const FIELD_LIMIT: usize = 131_072;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    StartRecord,
    StartField,
    InField,
    InQuotedField,
    QuoteInQuotedField,
}

/// Python `str.splitlines()` without keepends.
pub fn split_lines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let is_break = matches!(
            c,
            '\n' | '\r'
                | '\x0b'
                | '\x0c'
                | '\x1c'
                | '\x1d'
                | '\x1e'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        );
        if !is_break {
            continue;
        }
        out.push(&s[start..i]);
        let mut next = i + c.len_utf8();
        if c == '\r' && chars.peek().is_some_and(|&(_, n)| n == '\n') {
            chars.next();
            next += 1;
        }
        start = next;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

struct Parser {
    dialect: Dialect,
    state: State,
    field: String,
    field_len: usize,
    fields: Vec<String>,
}

impl Parser {
    fn save_field(&mut self) {
        self.fields.push(std::mem::take(&mut self.field));
        self.field_len = 0;
    }

    fn add_char(&mut self, c: char) -> Result<(), String> {
        if self.field_len >= FIELD_LIMIT {
            return Err(format!("field larger than field limit ({FIELD_LIMIT})"));
        }
        self.field.push(c);
        self.field_len += 1;
        Ok(())
    }

    /// Process one character; `None` marks end of line.
    fn process(&mut self, c: Option<char>) -> Result<(), String> {
        let d = self.dialect;
        match self.state {
            State::StartRecord => {
                if c.is_none() {
                    return Ok(());
                }
                self.state = State::StartField;
                self.process(c)?;
            }
            State::StartField => match c {
                None => {
                    self.save_field();
                    self.state = State::StartRecord;
                }
                Some(c) if c == d.quotechar => self.state = State::InQuotedField,
                Some(' ') if d.skipinitialspace => {}
                Some(c) if c == d.delimiter => self.save_field(),
                Some(c) => {
                    self.add_char(c)?;
                    self.state = State::InField;
                }
            },
            State::InField => match c {
                None => {
                    self.save_field();
                    self.state = State::StartRecord;
                }
                Some(c) if c == d.delimiter => {
                    self.save_field();
                    self.state = State::StartField;
                }
                Some(c) => self.add_char(c)?,
            },
            State::InQuotedField => match c {
                None => {}
                Some(c) if c == d.quotechar => {
                    self.state = if d.doublequote {
                        State::QuoteInQuotedField
                    } else {
                        State::InField
                    };
                }
                Some(c) => self.add_char(c)?,
            },
            State::QuoteInQuotedField => match c {
                Some(c) if c == d.quotechar => {
                    self.add_char(c)?;
                    self.state = State::InQuotedField;
                }
                Some(c) if c == d.delimiter => {
                    self.save_field();
                    self.state = State::StartField;
                }
                None => {
                    self.save_field();
                    self.state = State::StartRecord;
                }
                Some(c) => {
                    self.add_char(c)?;
                    self.state = State::InField;
                }
            },
        }
        Ok(())
    }
}

/// Parse `lines` into records.
pub fn read(lines: &[&str], dialect: Dialect) -> Result<Vec<Vec<String>>, String> {
    let mut p = Parser {
        dialect,
        state: State::StartRecord,
        field: String::new(),
        field_len: 0,
        fields: Vec::new(),
    };
    let mut rows = Vec::new();
    for line in lines {
        for c in line.chars() {
            p.process(Some(c))?;
        }
        p.process(None)?;
        if p.state == State::StartRecord {
            rows.push(std::mem::take(&mut p.fields));
        }
    }
    if p.field_len != 0 || p.state == State::InQuotedField {
        p.save_field();
        rows.push(p.fields);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(lines: &[&str], dialect: Dialect) -> Vec<Vec<String>> {
        read(lines, dialect).unwrap()
    }

    fn v(rows: &[&[&str]]) -> Vec<Vec<String>> {
        rows.iter()
            .map(|r| r.iter().map(|s| s.to_string()).collect())
            .collect()
    }

    #[test]
    fn splitlines() {
        assert_eq!(split_lines("a\r\nb\rc\nd"), vec!["a", "b", "c", "d"]);
        assert_eq!(split_lines("a\n\nb\n"), vec!["a", "", "b"]);
        assert_eq!(split_lines(""), Vec::<&str>::new());
        assert_eq!(split_lines("\n"), vec![""]);
    }

    #[test]
    fn quoted_newline_is_dropped() {
        assert_eq!(parse(&["\"a", "b\""], Dialect::EXCEL), v(&[&["ab"]]));
    }

    #[test]
    fn quirks() {
        let rows = parse(
            &["\"a\"x,b", "\"a\"\"b\",\"\"\"\"", "a\"b\"c,\"unterminated"],
            Dialect::EXCEL,
        );
        assert_eq!(
            rows,
            v(&[&["ax", "b"], &["a\"b", "\""], &["a\"b\"c", "unterminated"]])
        );
    }

    #[test]
    fn skip_initial_space_and_empty_lines() {
        let d = Dialect {
            skipinitialspace: true,
            ..Dialect::EXCEL
        };
        assert_eq!(
            parse(&["a, b,  \"c\" ,d", "", "  "], d),
            v(&[&["a", "b", "c ", "d"], &[], &[""]])
        );
    }

    #[test]
    fn no_doublequote() {
        let d = Dialect {
            doublequote: false,
            ..Dialect::EXCEL
        };
        assert_eq!(parse(&["\"a\"\"b\",c"], d), v(&[&["a\"b\"", "c"]]));
    }
}
