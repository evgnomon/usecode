// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Argument splitting that mirrors the original argparse `REMAINDER` layout:
//! options are only recognized before the unit name; everything after the
//! unit name belongs to the command verbatim.

/// Long options taking a value.
const VALUE_LONGS: [&str; 2] = ["description", "output"];
/// Short options taking a value.
const VALUE_SHORTS: [char; 2] = ['d', 'o'];

/// Result of splitting argv (without the program name).
#[derive(Debug, PartialEq, Eq)]
pub struct Split {
    /// Leading arguments (options and the unit name) for the option parser.
    pub head: Vec<String>,
    /// The command to run, with separators stripped.
    pub command: Vec<String>,
}

fn is_negative_number(arg: &str) -> bool {
    let Some(rest) = arg.strip_prefix('-') else {
        return false;
    };
    let (int, frac) = rest.split_once('.').unwrap_or((rest, ""));
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    digits(int) && digits(frac) && !(int.is_empty() && frac.is_empty())
}

/// Number of argv entries an option at `arg` occupies (1 or 2).
fn option_width(arg: &str) -> usize {
    if let Some(long) = arg.strip_prefix("--") {
        if long.contains('=') || long.is_empty() {
            return 1;
        }
        let takes_value = VALUE_LONGS.iter().any(|name| name.starts_with(long));
        return if takes_value { 2 } else { 1 };
    }
    for (i, c) in arg.char_indices().skip(1) {
        if VALUE_SHORTS.contains(&c) {
            // Attached value (`-dX`) or value in the next argument (`-d X`).
            return if i + c.len_utf8() == arg.len() { 2 } else { 1 };
        }
    }
    1
}

/// Split argv into the option/unit-name head and the command tail.
///
/// argparse drops one `--` directly after the unit name unless a `--` was
/// already consumed before it; the original tool then strips one more.
pub fn split(args: &[String]) -> Split {
    let mut i = 0;
    let mut escaped = false;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--" {
            escaped = true;
            i += 1;
            break;
        }
        if a.len() > 1 && a.starts_with('-') && !is_negative_number(a) {
            i += option_width(a);
            continue;
        }
        break;
    }
    let head_end = (i + 1).min(args.len());
    let head = args[..head_end].to_vec();
    let mut command = &args[head_end..];
    if !escaped && command.first().is_some_and(|a| a == "--") {
        command = &command[1..];
    }
    if command.first().is_some_and(|a| a == "--") {
        command = &command[1..];
    }
    Split {
        head,
        command: command.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    fn check(input: &str, head: &str, command: &str) {
        assert_eq!(
            split(&v(input)),
            Split {
                head: v(head),
                command: v(command)
            },
            "input: {input}"
        );
    }

    #[test]
    fn splits() {
        check("foo ls -la", "foo", "ls -la");
        check("-d D foo ls -la", "-d D foo", "ls -la");
        check("foo -d D ls", "foo", "-d D ls");
        check("-dX -ie foo ls", "-dX -ie foo", "ls");
        check("-id X foo ls", "-id X foo", "ls");
        check("--desc X foo ls", "--desc X foo", "ls");
        check("--output=o foo ls", "--output=o foo", "ls");
        check("-5 ls", "-5", "ls");
        check("foo", "foo", "");
        check("", "", "");
    }

    #[test]
    fn separators() {
        check("foo -- ls", "foo", "ls");
        check("foo --", "foo", "");
        check("foo -- -- a", "foo", "a");
        check("-- foo -- -- ls", "-- foo", "-- ls");
        check("-d D -- foo -- ls", "-d D -- foo", "ls");
        check("foo ls -- x", "foo", "ls -- x");
    }
}
