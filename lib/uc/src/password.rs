// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Random passwords for `uc secret gen`.

use anyhow::{Result, bail};

pub const LETTERS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
pub const DIGITS: &str = "0123456789";
/// The same set as Python's `string.punctuation`.
pub const SYMBOLS: &str = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

/// Which character sets a password draws from.
#[derive(Debug, Clone, Copy)]
pub struct Charset {
    pub letters: bool,
    pub digits: bool,
    pub symbols: bool,
}

impl Charset {
    fn chars(self) -> Vec<u8> {
        let mut chars = Vec::new();
        for (on, set) in [
            (self.letters, LETTERS),
            (self.digits, DIGITS),
            (self.symbols, SYMBOLS),
        ] {
            if on {
                chars.extend_from_slice(set.as_bytes());
            }
        }
        chars
    }
}

/// A password of `length` characters drawn uniformly from `charset` using the
/// operating system's random source.
pub fn generate(length: usize, charset: Charset) -> Result<String> {
    if length < 1 {
        bail!("password length must be at least 1");
    }
    let chars = charset.chars();
    if chars.is_empty() {
        bail!("at least one character set must be selected");
    }
    // Rejection sampling: only bytes below the largest multiple of the set
    // size are used, so every character is equally likely.
    let limit = 256 - 256 % chars.len();
    let mut password = String::with_capacity(length);
    let mut buf = [0u8; 64];
    while password.len() < length {
        getrandom::fill(&mut buf).map_err(|err| anyhow::anyhow!("reading random bytes: {err}"))?;
        for &byte in &buf {
            if (byte as usize) < limit && password.len() < length {
                password.push(chars[byte as usize % chars.len()] as char);
            }
        }
    }
    Ok(password)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Charset = Charset {
        letters: true,
        digits: true,
        symbols: true,
    };

    #[test]
    fn has_the_requested_length() {
        assert_eq!(generate(32, ALL).unwrap().len(), 32);
        assert_eq!(generate(1, ALL).unwrap().len(), 1);
        assert_eq!(generate(500, ALL).unwrap().len(), 500);
    }

    #[test]
    fn draws_only_from_the_selected_sets() {
        let digits = Charset {
            letters: false,
            digits: true,
            symbols: false,
        };
        assert!(
            generate(200, digits)
                .unwrap()
                .bytes()
                .all(|b| b.is_ascii_digit())
        );
        let no_symbols = Charset {
            symbols: false,
            ..ALL
        };
        assert!(
            generate(200, no_symbols)
                .unwrap()
                .bytes()
                .all(|b| b.is_ascii_alphanumeric())
        );
    }

    #[test]
    fn rejects_an_empty_request() {
        assert!(generate(0, ALL).is_err());
        let none = Charset {
            letters: false,
            digits: false,
            symbols: false,
        };
        assert!(generate(8, none).is_err());
    }
}
