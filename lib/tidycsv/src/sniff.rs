// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Port of Python's `csv.Sniffer.sniff` (CPython 3.14) restricted to what
//! tidycsv needs: guessing delimiter, quote character, double-quoting and
//! initial-space skipping from a text sample.

/// Reader dialect (Python `csv.Dialect` subset; no escape character).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dialect {
    pub delimiter: char,
    pub quotechar: char,
    pub doublequote: bool,
    pub skipinitialspace: bool,
}

impl Dialect {
    /// Python's `csv.excel` dialect.
    pub const EXCEL: Dialect = Dialect {
        delimiter: ',',
        quotechar: '"',
        doublequote: true,
        skipinitialspace: false,
    };
}

const PREFERRED: [char; 5] = [',', '\t', ';', ' ', ':'];

/// Insertion-ordered counter; `max` returns the first key with the highest
/// count, like Python's `max(d, key=d.get)`.
#[derive(Default)]
struct Counter(Vec<(char, usize)>);

impl Counter {
    fn add(&mut self, k: char) {
        match self.0.iter_mut().find(|(c, _)| *c == k) {
            Some((_, n)) => *n += 1,
            None => self.0.push((k, 1)),
        }
    }

    fn get(&self, k: char) -> usize {
        self.0.iter().find(|(c, _)| *c == k).map_or(0, |(_, n)| *n)
    }

    fn max(&self) -> Option<char> {
        let mut best: Option<(char, usize)> = None;
        for &(c, n) in &self.0 {
            if best.is_none_or(|(_, b)| n > b) {
                best = Some((c, n));
            }
        }
        best.map(|(c, _)| c)
    }
}

/// Sniff the dialect of `sample`; `None` when no delimiter can be found.
pub fn sniff(sample: &str, delimiters: &str) -> Option<Dialect> {
    let data: Vec<char> = sample
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .collect();

    let (quotechar, doublequote, mut delimiter, mut skipinitialspace) =
        guess_quote_and_delimiter(&data, delimiters);
    if delimiter.is_none() {
        (delimiter, skipinitialspace) = guess_delimiter(&data, delimiters);
    }
    Some(Dialect {
        delimiter: delimiter?,
        quotechar: quotechar.unwrap_or('"'),
        doublequote,
        skipinitialspace,
    })
}

fn is_quote(c: char) -> bool {
    c == '"' || c == '\''
}

/// `[^\w\n"']`
fn is_delim_candidate(c: char) -> bool {
    !(c.is_alphanumeric() || c == '_' || c == '\n' || is_quote(c))
}

/// At `bol`: is `p` a line start (`^` with MULTILINE)?
fn at_line_start(data: &[char], p: usize) -> bool {
    p == 0 || data[p - 1] == '\n'
}

/// At `p`: does `$` with MULTILINE match?
fn at_line_end(data: &[char], p: usize) -> bool {
    p == data.len() || data[p] == '\n'
}

/// Scan a quoted field whose opening quote is at `q`: the body is
/// possessively `(QQ | not-Q)*`, followed by a closing quote. Returns the
/// index of the closing quote.
fn closing_quote(data: &[char], q: usize) -> Option<usize> {
    let quote = data[q];
    let n = data.len();
    let mut i = q + 1;
    loop {
        if i + 1 < n && data[i] == quote && data[i + 1] == quote {
            i += 2;
        } else if i < n && data[i] != quote {
            i += 1;
        } else {
            break;
        }
    }
    (i < n).then_some(i)
}

/// One `findall` match: (quote, delimiter group, space group).
struct QuoteMatch {
    quote: char,
    delim: Option<(char, bool)>,
}

/// `(?P<delim>D)(?P<space> ?)(?P<quote>Q)` at `p`; returns (delim, space,
/// quote index).
fn lead_delim(data: &[char], p: usize) -> Option<(char, bool, usize)> {
    let d = *data.get(p)?;
    if !is_delim_candidate(d) {
        return None;
    }
    let mut j = p + 1;
    let space = data.get(j) == Some(&' ');
    if space {
        j += 1;
    }
    is_quote(*data.get(j)?).then_some((d, space, j))
}

/// `(?:^|\n)(?P<quote>Q)` at `p`; returns the quote index.
fn lead_line(data: &[char], p: usize) -> Option<usize> {
    if at_line_start(data, p) && data.get(p).is_some_and(|&c| is_quote(c)) {
        return Some(p);
    }
    if data.get(p) == Some(&'\n') && data.get(p + 1).is_some_and(|&c| is_quote(c)) {
        return Some(p + 1);
    }
    None
}

type Matcher = fn(&[char], usize) -> Option<(QuoteMatch, usize)>;

/// `,"...",`
fn pattern_delim_delim(data: &[char], p: usize) -> Option<(QuoteMatch, usize)> {
    let (d, space, q) = lead_delim(data, p)?;
    let e = closing_quote(data, q)?;
    (data.get(e + 1) == Some(&d)).then_some((
        QuoteMatch {
            quote: data[q],
            delim: Some((d, space)),
        },
        e + 2,
    ))
}

/// `^"...",`
fn pattern_line_delim(data: &[char], p: usize) -> Option<(QuoteMatch, usize)> {
    let q = lead_line(data, p)?;
    let e = closing_quote(data, q)?;
    let d = *data.get(e + 1)?;
    if !is_delim_candidate(d) {
        return None;
    }
    let space = data.get(e + 2) == Some(&' ');
    let end = e + 2 + usize::from(space);
    Some((
        QuoteMatch {
            quote: data[q],
            delim: Some((d, space)),
        },
        end,
    ))
}

/// `,"..."$`
fn pattern_delim_line(data: &[char], p: usize) -> Option<(QuoteMatch, usize)> {
    let (d, space, q) = lead_delim(data, p)?;
    let e = closing_quote(data, q)?;
    at_line_end(data, e + 1).then_some((
        QuoteMatch {
            quote: data[q],
            delim: Some((d, space)),
        },
        e + 1,
    ))
}

/// `^"..."$`
fn pattern_line_line(data: &[char], p: usize) -> Option<(QuoteMatch, usize)> {
    let q = lead_line(data, p)?;
    let e = closing_quote(data, q)?;
    at_line_end(data, e + 1).then_some((
        QuoteMatch {
            quote: data[q],
            delim: None,
        },
        e + 1,
    ))
}

/// Python `re.findall` over non-overlapping matches.
fn find_all(data: &[char], m: Matcher) -> Vec<QuoteMatch> {
    let mut out = Vec::new();
    let mut p = 0;
    while p <= data.len() {
        match m(data, p) {
            Some((qm, end)) => {
                out.push(qm);
                p = end.max(p + 1);
            }
            None => p += 1,
        }
    }
    out
}

fn guess_quote_and_delimiter(
    data: &[char],
    delimiters: &str,
) -> (Option<char>, bool, Option<char>, bool) {
    let patterns: [Matcher; 4] = [
        pattern_delim_delim,
        pattern_line_delim,
        pattern_delim_line,
        pattern_line_line,
    ];
    let matches = patterns
        .iter()
        .map(|m| find_all(data, *m))
        .find(|found| !found.is_empty());
    let Some(matches) = matches else {
        return (None, false, None, false);
    };

    let mut quotes = Counter::default();
    let mut delims = Counter::default();
    let mut spaces = 0;
    for m in &matches {
        quotes.add(m.quote);
        let Some((d, space)) = m.delim else {
            continue;
        };
        if delimiters.contains(d) {
            delims.add(d);
        }
        if space {
            spaces += 1;
        }
    }

    let quotechar = quotes.max();
    let (delim, skipinitialspace) = match delims.max() {
        Some(d) => (Some(d), delims.get(d) == spaces),
        None => (None, false),
    };

    let doublequote = match (delim, quotechar) {
        (Some(d), Some(q)) => has_doubled_quote(data, d, q),
        _ => false,
    };
    (quotechar, doublequote, delim, skipinitialspace)
}

/// Any whole quoted field (`(?<=D)|^` + spaces + `Q body Q` + `D|$`) whose
/// body contains a doubled quote.
fn has_doubled_quote(data: &[char], delim: char, quote: char) -> bool {
    let n = data.len();
    let mut p = 0;
    while p < n {
        if let Some((end, doubled)) = doubled_field_at(data, p, delim, quote) {
            if doubled {
                return true;
            }
            p = end.max(p + 1);
        } else {
            p += 1;
        }
    }
    false
}

fn doubled_field_at(data: &[char], p: usize, delim: char, quote: char) -> Option<(usize, bool)> {
    if !(at_line_start(data, p) || data[p - 1] == delim) {
        return None;
    }
    let mut j = p;
    if delim != ' ' {
        while data.get(j) == Some(&' ') {
            j += 1;
        }
    }
    if data.get(j) != Some(&quote) {
        return None;
    }
    let e = closing_quote(data, j)?;
    let body = &data[j + 1..e];
    let doubled = body.windows(2).any(|w| w[0] == quote && w[1] == quote);
    if data.get(e + 1) == Some(&delim) {
        Some((e + 2, doubled))
    } else if at_line_end(data, e + 1) {
        Some((e + 1, doubled))
    } else {
        None
    }
}

/// `data[0].count(d) == data[0].count(d + " ")`
fn skips_space(line: &[char], d: char) -> bool {
    let all = line.iter().filter(|&&c| c == d).count();
    let mut with_space = 0;
    let mut i = 0;
    while i + 1 < line.len() {
        if line[i] == d && line[i + 1] == ' ' {
            with_space += 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    all == with_space
}

/// Frequency-consistency based delimiter guess (`Sniffer._guess_delimiter`).
fn guess_delimiter(data: &[char], delimiters: &str) -> (Option<char>, bool) {
    let lines: Vec<&[char]> = data
        .split(|&c| c == '\n')
        .filter(|l| !l.is_empty())
        .collect();
    const ASCII: usize = 127;

    let chunk = lines.len().min(10);
    let mut iteration = 0;
    // Per ASCII char: (frequency, rows) in first-seen order.
    let mut char_frequency: Vec<Vec<(usize, i64)>> = vec![Vec::new(); ASCII];
    let mut modes: Vec<Option<(usize, i64)>> = vec![None; ASCII];
    let mut delims: Vec<(char, (usize, i64))> = Vec::new();
    let (mut start, mut end) = (0, chunk);

    while start < lines.len() {
        iteration += 1;
        for line in &lines[start..end.min(lines.len())] {
            let mut counts = [0usize; ASCII];
            for &c in line.iter() {
                if (c as u32) < ASCII as u32 {
                    counts[c as usize] += 1;
                }
            }
            for (meta, &freq) in char_frequency.iter_mut().zip(counts.iter()) {
                match meta.iter_mut().find(|(f, _)| *f == freq) {
                    Some((_, n)) => *n += 1,
                    None => meta.push((freq, 1)),
                }
            }
        }

        for (c, meta) in char_frequency.iter().enumerate() {
            if meta.len() == 1 && meta[0].0 == 0 {
                continue;
            }
            if meta.len() > 1 {
                let mut best = 0;
                for (i, item) in meta.iter().enumerate() {
                    if item.1 > meta[best].1 {
                        best = i;
                    }
                }
                let others: i64 = meta
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != best)
                    .map(|(_, item)| item.1)
                    .sum();
                modes[c] = Some((meta[best].0, meta[best].1 - others));
            } else {
                modes[c] = meta.first().copied();
            }
        }

        let total = (chunk * iteration).min(lines.len()) as f64;
        let mut consistency = 1.0f64;
        let threshold = 0.9f64;
        while delims.is_empty() && consistency >= threshold {
            for (c, mode) in modes.iter().enumerate() {
                let Some((freq, rows)) = *mode else { continue };
                let ch = char::from(c as u8);
                if freq > 0
                    && rows > 0
                    && rows as f64 / total >= consistency
                    && delimiters.contains(ch)
                    && !delims.iter().any(|(k, _)| *k == ch)
                {
                    delims.push((ch, (freq, rows)));
                }
            }
            consistency -= 0.01;
        }

        if delims.len() == 1 {
            let d = delims[0].0;
            return (Some(d), skips_space(lines[0], d));
        }

        start = end;
        end += chunk;
    }

    if delims.is_empty() {
        return (None, false);
    }

    if delims.len() > 1
        && let Some(&d) = PREFERRED
            .iter()
            .find(|p| delims.iter().any(|(k, _)| k == *p))
    {
        return (Some(d), skips_space(lines[0], d));
    }

    let (d, _) = delims
        .iter()
        .max_by(|a, b| (a.1, a.0).cmp(&(b.1, b.0)))
        .copied()
        .expect("non-empty");
    (Some(d), skips_space(lines[0], d))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELIMS: &str = ",;\t|";

    fn d(delimiter: char, quotechar: char, doublequote: bool, skip: bool) -> Option<Dialect> {
        Some(Dialect {
            delimiter,
            quotechar,
            doublequote,
            skipinitialspace: skip,
        })
    }

    #[test]
    fn plain_comma() {
        assert_eq!(sniff("a,b,c\n1,2,3\n", DELIMS), d(',', '"', false, false));
    }

    #[test]
    fn semicolon_with_spaces() {
        assert_eq!(
            sniff("a; b; c\n1; 2; 3\n", DELIMS),
            d(';', '"', false, true)
        );
    }

    #[test]
    fn quoted_with_doubled_quotes() {
        let s = "name,note\n\"x\",\"say \"\"hi\"\"\"\n";
        assert_eq!(sniff(s, DELIMS), d(',', '"', true, false));
    }

    #[test]
    fn single_quotes_pipe() {
        let s = "a|'b c'|d\n1|'2'|3\n";
        assert_eq!(sniff(s, DELIMS), d('|', '\'', false, false));
    }

    #[test]
    fn single_column_fails() {
        assert_eq!(sniff("abc\ndef\n", DELIMS), None);
    }

    #[test]
    fn prefers_comma_on_tie() {
        assert_eq!(sniff("a,b;c\n1,2;3\n", DELIMS), d(',', '"', false, false));
    }
}
