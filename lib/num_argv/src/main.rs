// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Print argc and every argv element with its index.

use std::env;
use std::ffi::OsString;
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;

fn render(args: &[OsString]) -> Vec<u8> {
    let mut out = Vec::new();
    let arg0 = args.first().map(|a| a.as_bytes()).unwrap_or_default();
    out.extend_from_slice(format!("argc: {}\n", args.len().max(1)).as_bytes());
    // `echo first argv: $0` is unquoted: whitespace runs collapse to one space.
    out.extend_from_slice(b"first argv:");
    for word in arg0
        .split(|b| b.is_ascii_whitespace())
        .filter(|w| !w.is_empty())
    {
        out.push(b' ');
        out.extend_from_slice(word);
    }
    out.extend_from_slice(b"\n\n");
    for (i, a) in args.iter().enumerate() {
        out.extend_from_slice(format!("{i} argv: ").as_bytes());
        out.extend_from_slice(a.as_bytes());
        out.push(b'\n');
    }
    if args.is_empty() {
        out.extend_from_slice(b"0 argv: \n");
    }
    out
}

fn main() {
    let args: Vec<OsString> = env::args_os().collect();
    let mut stdout = io::stdout().lock();
    if stdout
        .write_all(&render(&args))
        .and_then(|_| stdout.flush())
        .is_err()
    {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn lists_args() {
        let args = vec!["./num_argv".into(), "a b".into(), "".into()];
        let out = String::from_utf8(render(&args)).unwrap();
        assert_eq!(
            out,
            "argc: 3\nfirst argv: ./num_argv\n\n0 argv: ./num_argv\n1 argv: a b\n2 argv: \n"
        );
    }
}
