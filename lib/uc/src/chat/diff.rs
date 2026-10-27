// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Diffs of the edits uc is about to make, drawn the way delta draws them:
//! hunks with a few lines of context and line numbers, removed lines on a
//! red background and added ones on a green one, and within a changed line
//! the words that changed on a brighter one.

use std::fmt::Write;

/// Lines of unchanged text shown around a change.
const CONTEXT: usize = 3;
/// Above this many cells the line diff gives up on finding what stayed and
/// shows the changed middle as removed, then added.
const MAX_CELLS: usize = 4_000_000;
/// Paired lines sharing less than this part of their text are too unlike
/// for their changed words to be worth marking.
const MIN_LIKENESS: f64 = 0.4;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Same(usize, usize),
    Del(usize),
    Ins(usize),
}

struct Palette {
    del: &'static str,
    del_emph: &'static str,
    ins: &'static str,
    ins_emph: &'static str,
}

const TRUECOLOR: Palette = Palette {
    del: "\x1b[48;2;63;0;1m",
    del_emph: "\x1b[48;2;144;16;17m",
    ins: "\x1b[48;2;0;40;0m",
    ins_emph: "\x1b[48;2;0;96;0m",
};

const COLOR256: Palette = Palette {
    del: "\x1b[48;5;52m",
    del_emph: "\x1b[48;5;124m",
    ins: "\x1b[48;5;22m",
    ins_emph: "\x1b[48;5;28m",
};

/// Whether diffs on stderr get colors.
pub fn color() -> bool {
    use std::io::IsTerminal;
    std::env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal()
}

fn palette() -> &'static Palette {
    match std::env::var("COLORTERM").as_deref() {
        Ok("truecolor" | "24bit") => &TRUECOLOR,
        _ => &COLOR256,
    }
}

/// The diff from `old` to `new`, whole files, as hunks indented two spaces.
/// Empty when they are the same.
pub fn render(old: &str, new: &str, color: bool) -> String {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let ops = diff(&a, &b);
    let width = a.len().max(b.len()).max(1).to_string().len();
    let colors = color.then(palette);
    let mut out = String::new();
    for (n, (start, end)) in hunks(&ops).into_iter().enumerate() {
        if n > 0 {
            let dots = " ".repeat(width * 2 + 1);
            if color {
                let _ = writeln!(out, "  \x1b[2m{dots} ⋮\x1b[0m");
            } else {
                let _ = writeln!(out, "  {dots} ⋮");
            }
        }
        let mut i = start;
        while i < end {
            if let Op::Same(x, y) = ops[i] {
                gutter(&mut out, Some(x + 1), Some(y + 1), width, ' ', color);
                let _ = writeln!(out, "{}", tabs(a[x]));
                i += 1;
                continue;
            }
            let mut dels = Vec::new();
            let mut inss = Vec::new();
            while i < end {
                match ops[i] {
                    Op::Del(x) => dels.push(x),
                    Op::Ins(y) => inss.push(y),
                    Op::Same(..) => break,
                }
                i += 1;
            }
            let pairs = dels.len().min(inss.len());
            let mut emph: Vec<Option<(Vec<Span>, Vec<Span>)>> = (0..pairs)
                .map(|k| colors.and_then(|_| words(a[dels[k]], b[inss[k]])))
                .collect();
            for (k, &x) in dels.iter().enumerate() {
                gutter(&mut out, Some(x + 1), None, width, '-', color);
                let spans = emph.get(k).and_then(|e| e.as_ref()).map(|e| &e.0);
                line(&mut out, a[x], spans, colors.map(|p| (p.del, p.del_emph)));
            }
            for (k, &y) in inss.iter().enumerate() {
                gutter(&mut out, None, Some(y + 1), width, '+', color);
                let spans = emph.get_mut(k).and_then(|e| e.take()).map(|e| e.1);
                line(
                    &mut out,
                    b[y],
                    spans.as_ref(),
                    colors.map(|p| (p.ins, p.ins_emph)),
                );
            }
        }
    }
    out
}

fn gutter(
    out: &mut String,
    old: Option<usize>,
    new: Option<usize>,
    width: usize,
    mark: char,
    color: bool,
) {
    let num = |n: Option<usize>| n.map_or_else(|| " ".repeat(width), |n| format!("{n:>width$}"));
    let (old, new) = (num(old), num(new));
    if color {
        let fg = match mark {
            '-' => "\x1b[31m",
            '+' => "\x1b[32m",
            _ => "\x1b[2m",
        };
        let _ = write!(out, "  {fg}{old} {new} │\x1b[0m ");
    } else {
        let _ = write!(out, "  {old} {new} │{mark}");
    }
}

/// A run of a line's bytes and whether it changed.
type Span = (usize, usize, bool);

fn line(out: &mut String, text: &str, spans: Option<&Vec<Span>>, colors: Option<(&str, &str)>) {
    let Some((bg, emph)) = colors else {
        let _ = writeln!(out, "{}", tabs(text));
        return;
    };
    out.push_str(bg);
    match spans {
        Some(spans) => {
            for &(s, e, changed) in spans {
                if changed {
                    let _ = write!(out, "{emph}{}{bg}", tabs(&text[s..e]));
                } else {
                    out.push_str(&tabs(&text[s..e]));
                }
            }
        }
        None => out.push_str(&tabs(text)),
    }
    // Erasing to the end of the line with the background set fills it in.
    out.push_str("\x1b[K\x1b[0m\n");
}

fn tabs(text: &str) -> String {
    text.replace('\t', "    ")
}

/// Which words of two paired lines changed, or None when the lines are too
/// unlike for it to help.
fn words(old: &str, new: &str) -> Option<(Vec<Span>, Vec<Span>)> {
    let a = tokens(old);
    let b = tokens(new);
    let ta: Vec<&str> = a.iter().map(|&(s, e)| &old[s..e]).collect();
    let tb: Vec<&str> = b.iter().map(|&(s, e)| &new[s..e]).collect();
    let ops = diff(&ta, &tb);
    let mut sa = vec![true; a.len()];
    let mut sb = vec![true; b.len()];
    let mut same = 0;
    for op in &ops {
        if let Op::Same(x, y) = *op {
            sa[x] = false;
            sb[y] = false;
            same += ta[x].len();
        }
    }
    let total = old.len() + new.len();
    if total == 0 || (2 * same) as f64 / (total as f64) < MIN_LIKENESS {
        return None;
    }
    Some((merge(&a, &sa), merge(&b, &sb)))
}

/// Joins neighbouring tokens that are both changed or both not.
fn merge(tokens: &[(usize, usize)], changed: &[bool]) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    for (&(s, e), &c) in tokens.iter().zip(changed) {
        match spans.last_mut() {
            Some(last) if last.2 == c && last.1 == s => last.1 = e,
            _ => spans.push((s, e, c)),
        }
    }
    spans
}

/// A line cut into words, runs of spaces and single other characters.
fn tokens(text: &str) -> Vec<(usize, usize)> {
    #[derive(PartialEq)]
    enum Kind {
        Word,
        Space,
        Other,
    }
    let kind = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            Kind::Word
        } else if c.is_whitespace() {
            Kind::Space
        } else {
            Kind::Other
        }
    };
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut last: Option<Kind> = None;
    for (i, c) in text.char_indices() {
        let k = kind(c);
        let end = i + c.len_utf8();
        match out.last_mut() {
            Some(t) if last.as_ref() == Some(&k) && k != Kind::Other => t.1 = end,
            _ => out.push((i, end)),
        }
        last = Some(k);
    }
    out
}

/// The shortest edit from `a` to `b`, by a longest common subsequence after
/// the common start and end are set aside.
fn diff<T: PartialEq>(a: &[T], b: &[T]) -> Vec<Op> {
    let pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suf = a[pre..]
        .iter()
        .rev()
        .zip(b[pre..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (ma, mb) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
    let mut ops: Vec<Op> = (0..pre).map(|i| Op::Same(i, i)).collect();
    let (n, m) = (ma.len(), mb.len());
    if n * m > MAX_CELLS {
        ops.extend((0..n).map(|i| Op::Del(pre + i)));
        ops.extend((0..m).map(|j| Op::Ins(pre + j)));
    } else {
        // lcs[i][j]: the longest common subsequence of ma[i..] and mb[j..].
        let w = m + 1;
        let mut lcs = vec![0u32; (n + 1) * w];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i * w + j] = if ma[i] == mb[j] {
                    lcs[(i + 1) * w + j + 1] + 1
                } else {
                    lcs[(i + 1) * w + j].max(lcs[i * w + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && ma[i] == mb[j] {
                ops.push(Op::Same(pre + i, pre + j));
                i += 1;
                j += 1;
            } else if j == m || (i < n && lcs[(i + 1) * w + j] >= lcs[i * w + j + 1]) {
                ops.push(Op::Del(pre + i));
                i += 1;
            } else {
                ops.push(Op::Ins(pre + j));
                j += 1;
            }
        }
    }
    let (ea, eb) = (a.len() - suf, b.len() - suf);
    ops.extend((0..suf).map(|k| Op::Same(ea + k, eb + k)));
    ops
}

/// The ranges of ops to show: each change with CONTEXT lines around it,
/// ranges that touch merged.
fn hunks(ops: &[Op]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        if matches!(op, Op::Same(..)) {
            continue;
        }
        let start = i.saturating_sub(CONTEXT);
        let end = (i + 1 + CONTEXT).min(ops.len());
        match out.last_mut() {
            Some(last) if start <= last.1 => last.1 = end,
            _ => out.push((start, end)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_diff_has_context_and_numbers() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\n";
        let new = "a\nb\nc\nd\nE\nf\ng\nh\ni\n";
        let out = render(old, new, false);
        assert_eq!(
            out,
            "  2 2 │ b\n  3 3 │ c\n  4 4 │ d\n  5   │-e\n    5 │+E\n  6 6 │ f\n  7 7 │ g\n  8 8 │ h\n"
        );
    }

    #[test]
    fn far_changes_are_separate_hunks() {
        let old: String = (1..=20).map(|n| format!("{n}\n")).collect();
        let new = old.replace("2\n", "two\n").replace("19\n", "nineteen\n");
        let out = render(&old, &new, false);
        assert_eq!(out.matches('⋮').count(), 1);
        assert!(out.contains("│-2\n") && out.contains("│+nineteen\n"));
    }

    #[test]
    fn same_text_renders_nothing() {
        assert_eq!(render("x\ny\n", "x\ny\n", false), "");
    }

    #[test]
    fn changed_words_are_marked() {
        let (a, b) = words("let x = foo(1);", "let x = bar(1);").unwrap();
        assert_eq!(a, vec![(0, 8, false), (8, 11, true), (11, 15, false)]);
        assert_eq!(b, vec![(0, 8, false), (8, 11, true), (11, 15, false)]);
        assert!(words("completely", "different words here").is_none());
    }

    #[test]
    fn colored_lines_fill_the_background() {
        let out = render("foo(1)\n", "bar(1)\n", true);
        assert!(out.contains("\x1b[K\x1b[0m\n"));
        assert_eq!(out.lines().count(), 2);
    }
}
