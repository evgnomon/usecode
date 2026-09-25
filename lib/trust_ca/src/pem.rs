// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! PEM chain handling and openssl/certutil output parsing.

use std::sync::LazyLock;

use regex::Regex;

use crate::sh::{capture, trim_nl};

const BEGIN: &str = "-----BEGIN CERTIFICATE-----";

/// Keep only the lines inside BEGIN/END CERTIFICATE blocks
/// (`sed -ne '/-BEGIN CERTIFICATE-/,/-END CERTIFICATE-/p'`).
pub fn extract_blocks(text: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in text.lines() {
        if !inside && line.contains("-BEGIN CERTIFICATE-") {
            inside = true;
            out.push_str(line);
            out.push('\n');
        } else if inside {
            out.push_str(line);
            out.push('\n');
            if line.contains("-END CERTIFICATE-") {
                inside = false;
            }
        }
    }
    out
}

/// Split a bundle before every BEGIN CERTIFICATE line, dropping empty pieces
/// (`csplit -z ... '/-----BEGIN CERTIFICATE-----/' '{*}'`).
pub fn split_bundle(text: &str) -> Vec<String> {
    let mut pieces = vec![String::new()];
    for line in text.split_inclusive('\n') {
        if line.contains(BEGIN) {
            pieces.push(String::new());
        }
        pieces.last_mut().unwrap().push_str(line);
    }
    pieces.retain(|p| !p.is_empty());
    pieces
}

/// Run `openssl x509 -noout <flag>` on a PEM cert.
pub fn x509(pem: &str, flag: &str) -> Option<String> {
    let (ok, out) = capture("openssl", &["x509", "-noout", flag], Some(pem));
    ok.then(|| trim_nl(&out).to_string())
}

pub fn is_ca(pem: &str) -> bool {
    let (_, out) = capture("openssl", &["x509", "-noout", "-text"], Some(pem));
    out.contains("CA:TRUE")
}

/// Strip the trust-attribute column from a `certutil -L` line
/// (`sed 's/\s*[a-zA-Z,]*\s*$//' | sed 's/\s*$//'`).
pub fn nickname(line: &str) -> String {
    static ATTRS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*[a-zA-Z,]*\s*$").unwrap());
    ATTRS.replace(line, "").trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----\n";
    const B: &str = "-----BEGIN CERTIFICATE-----\nBBB\n-----END CERTIFICATE-----\n";

    #[test]
    fn extracts_blocks() {
        let text = format!("CONNECTED\n 0 s:CN=x\n{A}---\n 1 s:CN=y\n{B}---\nDONE\n");
        assert_eq!(extract_blocks(&text), format!("{A}{B}"));
    }

    #[test]
    fn splits_bundle() {
        assert_eq!(split_bundle(&format!("{A}{B}")), vec![A, B]);
        assert_eq!(split_bundle(&format!("# pre\n{A}")), vec!["# pre\n", A]);
        assert!(split_bundle("").is_empty());
    }

    #[test]
    fn nicknames() {
        assert_eq!(
            nickname("cr-main-sys-0                                                CT,C,C"),
            "cr-main-sys-0"
        );
        assert_eq!(
            nickname("My Root CA                                                   C,,  "),
            "My Root CA"
        );
        assert_eq!(nickname("foo bar   u,u,u"), "foo bar");
    }
}
