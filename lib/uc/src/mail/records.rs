// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The DNS records a mail server and its domains need for mail to arrive
//! and to be trusted where it goes, worked out from what the cluster and
//! Stalwart have, not from what Stalwart suggests.
//!
//! The choices, made for delivery:
//!
//! * The mail name has A records, never a CNAME (an MX may not point at
//!   one), and only for the node that answers on the mail ports: with
//!   `externalTrafficPolicy: Local` any other node drops the connection.
//! * SPF names every worker Stalwart can move to, not only the one it runs
//!   on, so mail sent right after a move still passes before the A records
//!   follow. Mechanisms someone else added (an `include:` of a newsletter
//!   service, say) are kept.
//! * Every DKIM key Stalwart signs with, or is about to, or just stopped
//!   signing with, is published under its selector.
//! * DMARC is `p=reject` by default (`mail_dmarc_policy`): the domain's mail
//!   is all signed, so anything else is someone pretending to be it.
//! * SRV records let mail apps find the server from an address alone.

use std::fmt;
use std::net::Ipv4Addr;

/// What a record holds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Data {
    A(Ipv4Addr),
    Mx {
        preference: u16,
        host: String,
    },
    Txt(String),
    Srv {
        priority: u16,
        weight: u16,
        port: u16,
        target: String,
    },
}

impl Data {
    pub fn kind(&self) -> &'static str {
        match self {
            Data::A(_) => "A",
            Data::Mx { .. } => "MX",
            Data::Txt(_) => "TXT",
            Data::Srv { .. } => "SRV",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Record {
    pub name: String,
    pub data: Data,
}

impl Record {
    fn new(name: impl Into<String>, data: Data) -> Record {
        Record {
            name: name.into(),
            data,
        }
    }

    /// The record as a zone file line, for a DNS host uc doesn't manage.
    pub fn zone_line(&self) -> String {
        format!(
            "{}. 3600 IN {} {}",
            self.name,
            self.data.kind(),
            self.zone_data()
        )
    }

    /// Its data as a zone file has it, with absolute names: what
    /// PowerDNS's API wants too.
    pub fn zone_data(&self) -> String {
        match &self.data {
            Data::A(ip) => ip.to_string(),
            Data::Mx { preference, host } => format!("{preference} {host}."),
            Data::Txt(text) => quoted(text),
            Data::Srv {
                priority,
                weight,
                port,
                target,
            } => format!("{priority} {weight} {port} {target}."),
        }
    }
}

impl fmt::Display for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let data = match &self.data {
            Data::A(ip) => ip.to_string(),
            Data::Mx { preference, host } => format!("{preference} {host}"),
            Data::Txt(text) => text.clone(),
            Data::Srv {
                priority,
                weight,
                port,
                target,
            } => format!("{priority} {weight} {port} {target}"),
        };
        write!(f, "{} {} {data}", self.data.kind(), self.name)
    }
}

/// A TXT value as DNS strings: quoted, at most 255 bytes each, so a 2048 bit
/// DKIM key fits.
pub fn quoted(text: &str) -> String {
    text.as_bytes()
        .chunks(255)
        .map(|c| format!("\"{}\"", String::from_utf8_lossy(c).replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A TXT value as one string again, however a DNS API quoted and split it:
/// `"v=DKIM1; p=MIIB" "IjAN"` is `v=DKIM1; p=MIIBIjAN`.
pub fn unquoted(text: &str) -> String {
    let text = text.trim();
    if !text.starts_with('"') {
        return text.to_string();
    }
    let mut out = String::new();
    let mut inside = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (c, inside) {
            ('"', _) => inside = !inside,
            ('\\', true) => out.extend(chars.next()),
            (c, true) => out.push(c),
            _ => {}
        }
    }
    out
}

/// What the domains' records come from.
#[derive(Debug, Clone, Default)]
pub struct Server {
    /// The name the server answers on, e.g. `mail.example.com`.
    pub host: String,
    /// Where the mail ports answer now: the mail name's A records.
    pub answers_on: Vec<Ipv4Addr>,
    /// Every address Stalwart may send from: the workers it can move to.
    pub sends_from: Vec<Ipv4Addr>,
    /// `none`, `quarantine` or `reject`.
    pub dmarc_policy: String,
}

/// A DKIM public key Stalwart has for a domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DkimKey {
    pub selector: String,
    /// `rsa` or `ed25519`.
    pub algorithm: String,
    pub public_key: String,
}

/// The mail name's own records: where it answers, and SPF for the name it
/// greets other servers with.
pub fn host_records(s: &Server) -> Vec<Record> {
    let mut records: Vec<Record> = s
        .answers_on
        .iter()
        .map(|ip| Record::new(&s.host, Data::A(*ip)))
        .collect();
    records.push(Record::new(&s.host, Data::Txt(spf(s, &[]))));
    records
}

/// Everything `domain` needs for Stalwart to take and send its mail.
/// `spf_kept` are the mechanisms of its current SPF record that someone
/// else added, see [`foreign_spf`].
pub fn domain_records(
    s: &Server,
    domain: &str,
    dkim: &[DkimKey],
    spf_kept: &[String],
) -> Vec<Record> {
    let postmaster = format!("mailto:postmaster@{domain}");
    let mut records = vec![
        Record::new(
            domain,
            Data::Mx {
                preference: 10,
                host: s.host.clone(),
            },
        ),
        Record::new(domain, Data::Txt(spf(s, spf_kept))),
        Record::new(
            format!("_dmarc.{domain}"),
            Data::Txt(format!("v=DMARC1; p={}; rua={postmaster}", s.dmarc_policy)),
        ),
        Record::new(
            format!("_smtp._tls.{domain}"),
            Data::Txt(format!("v=TLSRPTv1; rua={postmaster}")),
        ),
    ];
    for key in dkim {
        records.push(Record::new(
            format!("{}._domainkey.{domain}", key.selector),
            Data::Txt(format!(
                "v=DKIM1; k={}; p={}",
                key.algorithm, key.public_key
            )),
        ));
    }
    for (service, port) in SERVICES {
        records.push(Record::new(
            format!("_{service}._tcp.{domain}"),
            Data::Srv {
                priority: 0,
                weight: 1,
                port,
                target: s.host.clone(),
            },
        ));
    }
    records
}

/// What mail apps find through SRV (RFC 6186, 6764, 8314, 8620): TLS only.
const SERVICES: [(&str, u16); 7] = [
    ("submissions", 465),
    ("submission", 587),
    ("imaps", 993),
    ("pop3s", 995),
    ("jmap", 443),
    ("caldavs", 443),
    ("carddavs", 443),
];

fn spf(s: &Server, kept: &[String]) -> String {
    let mut terms = vec!["v=spf1".to_string()];
    terms.extend(s.sends_from.iter().map(|ip| format!("ip4:{ip}")));
    terms.extend(kept.iter().cloned());
    terms.push("-all".into());
    terms.join(" ")
}

/// Whether a TXT value is an SPF record.
pub fn is_spf(text: &str) -> bool {
    let text = text.trim().to_ascii_lowercase();
    text == "v=spf1" || text.starts_with("v=spf1 ")
}

/// The mechanisms of an SPF record that are not uc's to manage: not `all`,
/// not `mx` or `a`, and not an `ip4:` of a node of the cluster
/// (`ours`), so a worker that's gone drops out on the next run.
pub fn foreign_spf(text: &str, ours: &[Ipv4Addr]) -> Vec<String> {
    text.split_whitespace()
        .skip(1)
        .filter(|term| {
            let bare = term
                .trim_start_matches(['+', '-', '~', '?'])
                .to_ascii_lowercase();
            let own_ip = bare
                .strip_prefix("ip4:")
                .and_then(|ip| ip.parse::<Ipv4Addr>().ok())
                .is_some_and(|ip| ours.contains(&ip));
            !(bare == "all" || bare == "mx" || bare == "a" || own_ip)
        })
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server {
            host: "mail.example.com".into(),
            answers_on: vec![Ipv4Addr::new(192, 0, 2, 1)],
            sends_from: vec![Ipv4Addr::new(192, 0, 2, 1), Ipv4Addr::new(192, 0, 2, 2)],
            dmarc_policy: "reject".into(),
        }
    }

    fn find<'a>(records: &'a [Record], name: &str, kind: &str) -> Vec<&'a Data> {
        records
            .iter()
            .filter(|r| r.name == name && r.data.kind() == kind)
            .map(|r| &r.data)
            .collect()
    }

    #[test]
    fn the_mail_name_is_a_records_where_the_ports_answer() {
        let records = host_records(&server());
        assert_eq!(
            find(&records, "mail.example.com", "A"),
            [&Data::A(Ipv4Addr::new(192, 0, 2, 1))]
        );
        assert_eq!(
            find(&records, "mail.example.com", "TXT"),
            [&Data::Txt("v=spf1 ip4:192.0.2.1 ip4:192.0.2.2 -all".into())]
        );
    }

    #[test]
    fn a_domain_gets_mx_spf_dkim_dmarc_and_srv() {
        let key = DkimKey {
            selector: "v1-rsa-20261010".into(),
            algorithm: "rsa".into(),
            public_key: "MIIB".into(),
        };
        let kept = vec!["include:_spf.example.net".to_string()];
        let records = domain_records(&server(), "example.com", &[key], &kept);
        assert_eq!(
            find(&records, "example.com", "MX"),
            [&Data::Mx {
                preference: 10,
                host: "mail.example.com".into()
            }]
        );
        assert_eq!(
            find(&records, "example.com", "TXT"),
            [&Data::Txt(
                "v=spf1 ip4:192.0.2.1 ip4:192.0.2.2 include:_spf.example.net -all".into()
            )]
        );
        assert_eq!(
            find(&records, "v1-rsa-20261010._domainkey.example.com", "TXT"),
            [&Data::Txt("v=DKIM1; k=rsa; p=MIIB".into())]
        );
        assert_eq!(
            find(&records, "_dmarc.example.com", "TXT"),
            [&Data::Txt(
                "v=DMARC1; p=reject; rua=mailto:postmaster@example.com".into()
            )]
        );
        assert_eq!(
            find(&records, "_imaps._tcp.example.com", "SRV"),
            [&Data::Srv {
                priority: 0,
                weight: 1,
                port: 993,
                target: "mail.example.com".into()
            }]
        );
        assert_eq!(find(&records, "_pop3s._tcp.example.com", "SRV").len(), 1);
    }

    #[test]
    fn keeps_the_spf_mechanisms_someone_else_added() {
        let ours = [Ipv4Addr::new(192, 0, 2, 1), Ipv4Addr::new(192, 0, 2, 9)];
        assert_eq!(
            foreign_spf(
                "v=spf1 mx a ip4:192.0.2.9 ip4:198.51.100.7 include:_spf.example.net ~all",
                &ours
            ),
            ["ip4:198.51.100.7", "include:_spf.example.net"]
        );
        assert!(is_spf("v=spf1 -all"));
        assert!(!is_spf("v=spf10"));
        assert!(!is_spf("google-site-verification=x"));
    }

    #[test]
    fn long_txt_values_are_split_into_dns_strings_and_joined_back() {
        let key = "a".repeat(400);
        let q = quoted(&key);
        assert_eq!(q.matches('"').count(), 4);
        assert_eq!(unquoted(&q), key);
        assert_eq!(unquoted("v=spf1 -all"), "v=spf1 -all");
        assert_eq!(unquoted(r#""say \"hi\"""#), r#"say "hi""#);
    }

    #[test]
    fn zone_lines_are_absolute() {
        let r = Record::new(
            "example.com",
            Data::Mx {
                preference: 10,
                host: "mail.example.com".into(),
            },
        );
        assert_eq!(
            r.zone_line(),
            "example.com. 3600 IN MX 10 mail.example.com."
        );
    }
}
