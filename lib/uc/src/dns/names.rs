// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Names and record contents as you'd type them (`www`, `@`,
//! `10 mail`, `v=spf1 -all`), turned into what PowerDNS's API wants:
//! absolute names with the trailing dot, quoted TXT strings.

use anyhow::{Result, bail};

/// `example.com.` from `Example.com` or `example.com.`.
pub fn zone(raw: &str) -> Result<String> {
    let z = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    if z.is_empty() || z.contains(char::is_whitespace) || z.contains("..") {
        bail!("'{raw}' is not a zone name");
    }
    Ok(format!("{z}."))
}

/// The absolute name of `name` in `zone`: `@` is the zone itself, a name
/// ending in a dot or in the zone is taken as it is, anything else is
/// under the zone (`www` is `www.example.com.`).
pub fn name(raw: &str, zone: &str) -> String {
    let n = raw.trim().to_ascii_lowercase();
    let bare = zone.trim_end_matches('.');
    if n == "@" || n.is_empty() {
        zone.to_string()
    } else if n.ends_with('.') {
        n
    } else if n == bare || n.ends_with(&format!(".{bare}")) {
        format!("{n}.")
    } else {
        format!("{n}.{zone}")
    }
}

/// A name a record points at: a name without a dot is in `zone` (`mail`
/// is `mail.example.com.`), one with dots is absolute.
fn target(raw: &str, zone: &str) -> String {
    let t = raw.trim();
    if t == "@" {
        zone.to_string()
    } else if t.ends_with('.') {
        t.to_string()
    } else if t.contains('.') {
        format!("{t}.")
    } else {
        format!("{t}.{zone}")
    }
}

/// The record type, upper case.
pub fn kind(raw: &str) -> Result<String> {
    let k = raw.trim().to_ascii_uppercase();
    if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric()) {
        bail!("'{raw}' is not a record type");
    }
    if k == "SOA" {
        bail!("PowerDNS keeps the SOA record itself");
    }
    Ok(k)
}

/// The content of a `kind` record in `zone` as PowerDNS wants it.
pub fn content(kind: &str, raw: &str, zone: &str) -> Result<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        bail!("a {kind} record needs content");
    }
    let fields: Vec<&str> = raw.split_whitespace().collect();
    // The field holding a name, for the types that point at one.
    let last_name = |at: usize| -> Result<String> {
        if fields.len() != at + 1 {
            bail!("a {kind} record is {} fields, not '{raw}'", at + 1);
        }
        let mut f: Vec<String> = fields.iter().map(|f| f.to_string()).collect();
        f[at] = target(&f[at], zone);
        Ok(f.join(" "))
    };
    Ok(match kind {
        "CNAME" | "NS" | "PTR" | "DNAME" | "ALIAS" => last_name(0)?,
        "MX" => last_name(1)?,
        "SRV" => last_name(3)?,
        "TXT" | "SPF" if !raw.starts_with('"') => quote(raw),
        "A" => {
            raw.parse::<std::net::Ipv4Addr>()
                .map_err(|_| anyhow::anyhow!("'{raw}' is not an IPv4 address"))?;
            raw.to_string()
        }
        "AAAA" => {
            raw.parse::<std::net::Ipv6Addr>()
                .map_err(|_| anyhow::anyhow!("'{raw}' is not an IPv6 address"))?;
            raw.to_string()
        }
        _ => raw.to_string(),
    })
}

/// Text as DNS strings: quoted, escaped, in pieces of at most 255 bytes.
fn quote(text: &str) -> String {
    let mut pieces = Vec::new();
    let mut piece = String::new();
    let mut len = 0;
    for c in text.chars() {
        if len + c.len_utf8() > 255 {
            pieces.push(std::mem::take(&mut piece));
            len = 0;
        }
        if c == '"' || c == '\\' {
            piece.push('\\');
        }
        piece.push(c);
        len += c.len_utf8();
    }
    pieces.push(piece);
    pieces
        .iter()
        .map(|p| format!("\"{p}\""))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const Z: &str = "example.com.";

    #[test]
    fn names_are_under_the_zone_unless_absolute() {
        assert_eq!(zone("Example.COM").unwrap(), Z);
        assert_eq!(name("@", Z), Z);
        assert_eq!(name("www", Z), "www.example.com.");
        assert_eq!(name("WWW.example.com", Z), "www.example.com.");
        assert_eq!(name("example.com", Z), Z);
        assert_eq!(name("other.org.", Z), "other.org.");
        assert!(zone("a..b").is_err());
    }

    #[test]
    fn targets_get_their_dot() {
        assert_eq!(content("CNAME", "web", Z).unwrap(), "web.example.com.");
        assert_eq!(content("CNAME", "host.net", Z).unwrap(), "host.net.");
        assert_eq!(content("MX", "10 mail", Z).unwrap(), "10 mail.example.com.");
        assert_eq!(
            content("SRV", "0 5 993 mail.example.com", Z).unwrap(),
            "0 5 993 mail.example.com."
        );
        assert_eq!(content("NS", "@", Z).unwrap(), Z);
        assert!(content("MX", "mail", Z).is_err());
    }

    #[test]
    fn text_is_quoted_and_split() {
        assert_eq!(content("TXT", "v=spf1 -all", Z).unwrap(), "\"v=spf1 -all\"");
        assert_eq!(content("TXT", "\"as is\"", Z).unwrap(), "\"as is\"");
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        let long = "x".repeat(300);
        assert_eq!(quote(&long).matches('"').count(), 4);
    }

    #[test]
    fn addresses_are_checked() {
        assert!(content("A", "192.0.2.1", Z).is_ok());
        assert!(content("A", "2001:db8::1", Z).is_err());
        assert!(content("AAAA", "2001:db8::1", Z).is_ok());
        assert_eq!(kind("aaaa").unwrap(), "AAAA");
        assert!(kind("soa").is_err());
    }
}
