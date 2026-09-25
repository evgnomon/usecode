// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `Authorization: Signature <token>` encoding shared by client and server.
//!
//! The token is base64 of compact JSON
//! `{"pubkey","timestamp","nonce","signature"}`; the signature covers
//! `tenant|nonce|timestamp|METHOD|path[|signing_key]`.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Token {
    pub pubkey: String,
    pub timestamp: String,
    pub nonce: String,
    pub signature: String,
}

/// Encode a token (without the `Signature ` scheme prefix).
pub fn build_token(t: &Token) -> String {
    B64.encode(serde_json::to_vec(t).expect("token serializes"))
}

/// Parse a `Signature <token>` header value.
pub fn parse_header(header: &str) -> Result<Token, String> {
    let (scheme, token) = header.split_once(' ').unwrap_or((header, ""));
    if scheme != "Signature" || token.is_empty() {
        return Err("Expected 'Signature <token>' format".into());
    }
    let payload = B64.decode(token).map_err(|e| e.to_string())?;
    let data: serde_json::Value = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
    for field in ["pubkey", "timestamp", "nonce", "signature"] {
        if data.get(field).is_none() {
            return Err(format!("Missing field in token: {field}"));
        }
    }
    serde_json::from_value(data).map_err(|e| e.to_string())
}

/// Message covered by the signature.
pub fn message(
    tenant: &str,
    nonce: &str,
    ts: &str,
    method: &str,
    path: &str,
    signing_key: Option<&str>,
) -> String {
    let mut m = format!("{tenant}|{nonce}|{ts}|{method}|{path}");
    if let Some(k) = signing_key {
        m.push('|');
        m.push_str(k);
    }
    m
}

/// Client timestamp: UTC, second precision, `Z` suffix.
pub fn format_timestamp(now: DateTime<Utc>) -> String {
    now.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Parse a client timestamp (ISO 8601 with offset; `Z` means UTC).
pub fn parse_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    let s = ts.replace('Z', "+00:00");
    DateTime::parse_from_rfc3339(&s)
        .or_else(|_| DateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.f%:z"))
        .or_else(|_| DateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M%:z"))
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Python `datetime.isoformat()` of an aware UTC datetime.
pub fn py_isoformat(t: DateTime<Utc>) -> String {
    if t.timestamp_subsec_micros() == 0 {
        t.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
    } else {
        t.format("%Y-%m-%dT%H:%M:%S%.6f+00:00").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn token_roundtrip() {
        let t = Token {
            pubkey: "ssh-ed25519 AAAA".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            nonce: "n".into(),
            signature: "s".into(),
        };
        let enc = build_token(&t);
        let json = String::from_utf8(B64.decode(&enc).unwrap()).unwrap();
        assert_eq!(
            json,
            r#"{"pubkey":"ssh-ed25519 AAAA","timestamp":"2026-01-01T00:00:00Z","nonce":"n","signature":"s"}"#
        );
        assert_eq!(parse_header(&format!("Signature {enc}")).unwrap(), t);
    }

    #[test]
    fn header_errors() {
        assert_eq!(
            parse_header("Bearer x").unwrap_err(),
            "Expected 'Signature <token>' format"
        );
        assert_eq!(
            parse_header("Signature").unwrap_err(),
            "Expected 'Signature <token>' format"
        );
        let enc = B64.encode(r#"{"pubkey":"a","timestamp":"b","nonce":"c"}"#);
        assert_eq!(
            parse_header(&format!("Signature {enc}")).unwrap_err(),
            "Missing field in token: signature"
        );
    }

    #[test]
    fn messages() {
        assert_eq!(
            message("t", "n", "ts", "POST", "/read", None),
            "t|n|ts|POST|/read"
        );
        assert_eq!(
            message("t", "n", "ts", "POST", "/read", Some("k")),
            "t|n|ts|POST|/read|k"
        );
    }

    #[test]
    fn timestamps() {
        let t = Utc.with_ymd_and_hms(2026, 9, 25, 10, 0, 0).unwrap();
        assert_eq!(format_timestamp(t), "2026-09-25T10:00:00Z");
        assert_eq!(parse_timestamp("2026-09-25T10:00:00Z"), Some(t));
        assert_eq!(parse_timestamp("2026-09-25T12:00:00+02:00"), Some(t));
        assert_eq!(parse_timestamp("2026-09-25T10:00:00"), None);
        assert_eq!(parse_timestamp("garbage"), None);
        assert_eq!(py_isoformat(t), "2026-09-25T10:00:00+00:00");
        let t2 = t + chrono::Duration::microseconds(1500);
        assert_eq!(py_isoformat(t2), "2026-09-25T10:00:00.001500+00:00");
    }
}
