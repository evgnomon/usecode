// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The host firewall: a default-deny inbound policy the daemon owns, and
//! the settings file on the host that shapes it.
//!
//! Everything lives in one chain this module owns (`UC_FIREWALL`), jumped
//! to from `INPUT` in both families - IPv4 and IPv6 - so it can be rebuilt
//! on every converge without touching any other firewall rule on the host.
//! What is not explicitly allowed - SSH, the loopback interface,
//! established traffic, the DHCP client, the WireGuard mesh the daemon may
//! also run, what a Kubernetes controller needs when the host is one
//! ([`kube::inbound_rules`]), and whatever [`Settings::allow`] lists - is
//! dropped.
//!
//! The settings are an ordinary file on the host, [`SETTINGS_PATH`],
//! written by `uc net firewall` and read here. No file is not an error: it
//! means the default policy, which is exactly the default-deny inbound the
//! daemon is supposed to apply.

use std::net::IpAddr;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::{Context, Result};
use crate::kube;
use crate::net::parse_cidr;

/// Where the firewall settings live on a host.
pub const SETTINGS_PATH: &str = "/etc/uc/firewall.toml";

/// The chain this module owns in the filter table, in each family. Only
/// its contents are ever rebuilt, so rules anything else adds (other
/// chains, the rest of `INPUT`, nat/mangle) are left alone.
pub const CHAIN: &str = "UC_FIREWALL";

/// The address families the policy is applied to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    V4,
    V6,
}

/// Both, in the order they are applied.
pub const FAMILIES: [Family; 2] = [Family::V4, Family::V6];

impl Family {
    /// The tool that edits this family's rules.
    pub fn command(self) -> &'static str {
        match self {
            Family::V4 => "iptables",
            Family::V6 => "ip6tables",
        }
    }

    fn icmp(self) -> &'static str {
        match self {
            Family::V4 => "icmp",
            Family::V6 => "ipv6-icmp",
        }
    }
}

/// The inbound policy, as the settings file on a host spells it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Whether the default-deny policy is in force. Defaults to on, so a
    /// host with no settings file still closes everything but SSH.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// The port `sshd` listens on. Left out, the module asks `sshd -T`
    /// and falls back to 22.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
    /// Extra inbound exceptions, `"<tcp|udp>:<port>[:<source>]"`, e.g.
    /// `"tcp:443"` or `"udp:8000-8100:10.0.0.0/8"`. A missing source, or
    /// `*`, means any host; an IPv4 source only opens the IPv4 rule, an
    /// IPv6 source only the IPv6 one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
}

fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            enabled: true,
            ssh_port: None,
            allow: Vec::new(),
        }
    }
}

impl Settings {
    /// Read the settings at `path`. A missing file is the default policy,
    /// not an error.
    pub fn load(path: &str) -> Result<Settings> {
        let body = match std::fs::read_to_string(path) {
            Ok(body) => body,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
            Err(e) => bail!("read {path}: {e}"),
        };
        Settings::parse(&body).with_ctx(|| format!("parse {path}"))
    }

    pub fn parse(body: &str) -> Result<Settings> {
        let settings: Settings = toml::from_str(body).ctx("parse firewall settings")?;
        settings.validate()?;
        Ok(settings)
    }

    /// Encode the settings the way [`SETTINGS_PATH`] holds them, with the
    /// comment that says who writes the file.
    pub fn encode(&self) -> Result<String> {
        self.validate()?;
        let body = toml::to_string(self).ctx("encode firewall settings")?;
        Ok(format!(
            "# Written by `uc net firewall`; the usecode daemon reads it.\n\
             #\n\
             # Default-deny inbound, IPv4 and IPv6: only what is listed here\n\
             # (plus SSH, loopback, established traffic and the WireGuard mesh)\n\
             # gets in. Edit with `uc net firewall`, not by hand.\n\n{body}"
        ))
    }

    pub fn validate(&self) -> Result<()> {
        if self.ssh_port == Some(0) {
            bail!("ssh_port must be a port number (1-65535)");
        }
        for rule in &self.allow {
            parse_allow(rule)?;
        }
        Ok(())
    }
}

/// One parsed entry of [`Settings::allow`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allow {
    /// `tcp` or `udp`.
    pub proto: String,
    /// iptables-ready, e.g. `"443"` or `"9000:9100"`.
    pub port: String,
    /// A source address/CIDR, or `None` for any host.
    pub source: Option<String>,
}

impl Allow {
    /// The family a source restricts the rule to, or `None` when it
    /// applies to both.
    pub fn family(&self) -> Option<Family> {
        self.source.as_deref().and_then(source_family)
    }
}

/// Parse `"<tcp|udp>:<port>[:<source>]"`. `port` may be a range written
/// `first-last`; `source` may be `*` or empty for any host.
pub fn parse_allow(spec: &str) -> Result<Allow> {
    // `splitn`, not `split`: an IPv6 source carries colons of its own.
    let parts: Vec<&str> = spec.splitn(3, ':').collect();
    let (proto, port, source) = match parts.as_slice() {
        [proto, port] => (*proto, *port, ""),
        [proto, port, source] => (*proto, *port, *source),
        _ => bail!("bad rule {spec:?}: want <tcp|udp>:<port>[:<source>]"),
    };

    let proto = proto.to_ascii_lowercase();
    if !matches!(proto.as_str(), "tcp" | "udp") {
        bail!("bad rule {spec:?}: protocol must be tcp or udp");
    }

    let port = parse_port(port).with_ctx(|| format!("bad rule {spec:?}"))?;
    let source = match source {
        "" | "*" => None,
        s => {
            if source_family(s).is_none() {
                bail!("bad rule {spec:?}: source {s:?} is not an IP address or CIDR");
            }
            Some(s.to_string())
        }
    };

    Ok(Allow {
        proto,
        port,
        source,
    })
}

fn parse_port(s: &str) -> Result<String> {
    let (first, last) = match s.split_once('-') {
        Some((a, b)) => (port_number(a)?, port_number(b)?),
        None => {
            let p = port_number(s)?;
            (p, p)
        }
    };
    if first == 0 || last == 0 {
        bail!("port {s:?} must be 1-65535");
    }
    if first > last {
        bail!("port range {s:?} is backwards");
    }
    Ok(if first == last {
        first.to_string()
    } else {
        format!("{first}:{last}")
    })
}

fn port_number(s: &str) -> Result<u16> {
    s.parse::<u16>()
        .map_err(|_| crate::error::Error(format!("port {s:?} is not a number")))
}

/// The family an address or CIDR belongs to, or `None` if it is neither.
fn source_family(s: &str) -> Option<Family> {
    if let Ok((ip, _)) = parse_cidr(s) {
        return Some(family_of(ip));
    }
    s.parse::<IpAddr>().ok().map(family_of)
}

fn family_of(ip: IpAddr) -> Family {
    if ip.is_ipv4() { Family::V4 } else { Family::V6 }
}

/// The rules that make up the policy for one family, in the order they
/// are appended to [`CHAIN`], ending with the terminal drop. Kept pure so
/// the policy can be tested without touching a host's firewall.
pub fn inbound_rules(
    family: Family,
    settings: &Settings,
    ssh_port: u16,
    cfg: &Config,
    kube: &kube::Settings,
) -> Result<Vec<Vec<String>>> {
    let mut rules: Vec<Vec<String>> = vec![
        rule(&["-i", "lo", "-j", "ACCEPT"]),
        rule(&[
            "-m",
            "conntrack",
            "--ctstate",
            "ESTABLISHED,RELATED",
            "-j",
            "ACCEPT",
        ]),
        // ICMP has no ports; letting it through keeps ping, path-MTU
        // discovery and (for IPv6) neighbour discovery working.
        rule(&["-p", family.icmp(), "-j", "ACCEPT"]),
    ];

    // Keep the DHCP client working: its servers answer unicast, which
    // conntrack does not tie back to the multicast request.
    match family {
        Family::V4 => rules.push(rule(&[
            "-p", "udp", "--sport", "67", "--dport", "68", "-j", "ACCEPT",
        ])),
        Family::V6 => rules.push(rule(&[
            "-p", "udp", "--sport", "547", "--dport", "546", "-j", "ACCEPT",
        ])),
    }

    if cfg.mesh_enabled() {
        // The WireGuard handshake arrives on the physical interface, and
        // peer traffic on the tunnel itself, so neither may be dropped
        // just because the firewall only knows about SSH.
        if cfg.interface.listen_port > 0 {
            let port = cfg.interface.listen_port.to_string();
            rules.push(rule(&["-p", "udp", "--dport", &port, "-j", "ACCEPT"]));
        }
        rules.push(rule(&["-i", cfg.interface.name.as_str(), "-j", "ACCEPT"]));
    }

    rules.extend(kube::inbound_rules(kube, family == Family::V4));

    rules.push(rule(&[
        "-p",
        "tcp",
        "--dport",
        &ssh_port.to_string(),
        "-j",
        "ACCEPT",
    ]));

    for spec in &settings.allow {
        let allow = parse_allow(spec)?;
        if let Some(src_family) = allow.family()
            && src_family != family
        {
            continue;
        }
        let mut r = rule(&["-p", allow.proto.as_str(), "--dport", allow.port.as_str()]);
        if let Some(source) = &allow.source {
            r.push("-s".into());
            r.push(source.clone());
        }
        r.push("-j".into());
        r.push("ACCEPT".into());
        rules.push(r);
    }

    rules.push(rule(&["-j", "DROP"]));
    Ok(rules)
}

fn rule(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

/// The port to keep open for SSH: the setting, or what `sshd -T` reports,
/// or 22 as a last resort.
pub fn ssh_port(settings: &Settings) -> u16 {
    settings.ssh_port.or_else(detect_ssh_port).unwrap_or(22)
}

fn detect_ssh_port() -> Option<u16> {
    let out = Command::new("sshd").arg("-T").output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|line| {
            let line = line.trim();
            let value = line
                .strip_prefix("port ")
                .or_else(|| line.strip_prefix("Port "))?;
            value.trim().parse().ok()
        })
}

/// Make sure both families' tools are there to apply with.
pub fn prepare() -> Result<()> {
    crate::daemon::host::ensure_tools(&[("iptables", "iptables"), ("ip6tables", "iptables")])
}

/// Put a delivered policy in place on this host ([`SETTINGS_PATH`]),
/// reports whether anything changed. The daemon picks it up on reload.
pub fn accept(settings: &Settings) -> Result<bool> {
    let body = settings.encode()?;
    crate::daemon::host::put(SETTINGS_PATH, body.as_bytes(), 0o644)
}

/// Bring [`CHAIN`] to match `settings` in both families: create and hook
/// it if needed, flush it, then append the policy's rules. Safe to call
/// repeatedly.
pub fn apply(
    settings: &Settings,
    ssh_port: u16,
    cfg: &Config,
    kube: &kube::Settings,
) -> Result<()> {
    for family in FAMILIES {
        apply_family(family, settings, ssh_port, cfg, kube)?;
    }
    Ok(())
}

fn apply_family(
    family: Family,
    settings: &Settings,
    ssh_port: u16,
    cfg: &Config,
    kube: &kube::Settings,
) -> Result<()> {
    let rules = inbound_rules(family, settings, ssh_port, cfg, kube)?;

    ensure_chain(family)?;
    run(family, &["-F", CHAIN]).with_ctx(|| format!("flush {} {CHAIN}", family.command()))?;
    for r in &rules {
        let argv = append_args(r);
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        run(family, &refs).with_ctx(|| format!("add rule: {}", r.join(" ")))?;
    }
    Ok(())
}

/// Remove [`CHAIN`] and its jump in both families, leaving the host's
/// firewall as it was. A no-op when nothing was ever applied.
pub fn flush() -> Result<()> {
    for family in FAMILIES {
        while probe(family, &["-C", "INPUT", "-j", CHAIN]) {
            let _ = run(family, &["-D", "INPUT", "-j", CHAIN]);
        }
        let _ = run(family, &["-F", CHAIN]);
        let _ = run(family, &["-X", CHAIN]);
    }
    Ok(())
}

/// [`CHAIN`] as text for both families, for `uc net firewall status`.
pub fn ruleset() -> Result<String> {
    let mut out = String::new();
    for family in FAMILIES {
        if let Ok(o) = Command::new(family.command()).args(["-S", CHAIN]).output()
            && o.status.success()
        {
            out.push_str(&format!("# {}\n", family.command()));
            out.push_str(&String::from_utf8_lossy(&o.stdout));
            out.push('\n');
        }
    }
    if out.is_empty() {
        bail!("no {CHAIN} chain (the firewall is not applied)");
    }
    Ok(out)
}

fn ensure_chain(family: Family) -> Result<()> {
    if !probe(family, &["-n", "-L", CHAIN]) {
        run(family, &["-N", CHAIN]).with_ctx(|| format!("create chain {CHAIN}"))?;
    }
    if !probe(family, &["-C", "INPUT", "-j", CHAIN]) {
        run(family, &["-I", "INPUT", "1", "-j", CHAIN])
            .with_ctx(|| format!("hook {CHAIN} into INPUT"))?;
    }
    Ok(())
}

/// A policy rule as the full command arguments that append it to
/// [`CHAIN`].
fn append_args(rule: &[String]) -> Vec<String> {
    let mut argv = vec!["-A".to_string(), CHAIN.to_string()];
    argv.extend(rule.iter().cloned());
    argv
}

fn run(family: Family, args: &[&str]) -> Result<()> {
    crate::wg::run(family.command(), args)
}

/// Whether a rule tool invocation succeeds, without its output.
fn probe(family: Family, args: &[&str]) -> bool {
    Command::new(family.command())
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Interface;

    fn mesh_config() -> Config {
        Config {
            interface: Interface {
                name: "wg-uc".into(),
                address: "10.10.0.2/24".into(),
                listen_port: 51820,
                mtu: 0,
            },
            ..Config::default()
        }
    }

    fn joined(rules: &[Vec<String>]) -> Vec<String> {
        rules.iter().map(|r| r.join(" ")).collect()
    }

    #[test]
    fn a_missing_settings_file_means_default_deny() {
        let settings = Settings::default();
        assert!(settings.enabled);
        assert!(settings.allow.is_empty());
    }

    #[test]
    fn a_rule_parses_proto_port_and_source() {
        let a = parse_allow("tcp:443").unwrap();
        assert_eq!(a.proto, "tcp");
        assert_eq!(a.port, "443");
        assert_eq!(a.source, None);
        assert_eq!(a.family(), None);

        let a = parse_allow("UDP:8000-8100:10.0.0.0/8").unwrap();
        assert_eq!(a.proto, "udp");
        assert_eq!(a.port, "8000:8100");
        assert_eq!(a.source.as_deref(), Some("10.0.0.0/8"));
        assert_eq!(a.family(), Some(Family::V4));

        assert_eq!(parse_allow("tcp:443:*").unwrap().source, None);
        assert_eq!(
            parse_allow("tcp:443:2001:db8::/32").unwrap().family(),
            Some(Family::V6)
        );
    }

    #[test]
    fn bad_rules_are_refused() {
        for spec in [
            "",
            "sctp:443",
            "tcp:notaport",
            "tcp:0",
            "tcp:9000-8000",
            "tcp:443:not-an-address",
            "tcp:1:2:3:4",
        ] {
            assert!(parse_allow(spec).is_err(), "{spec:?} was accepted");
        }
    }

    #[test]
    fn a_rule_is_appended_to_the_chain() {
        let argv = append_args(&rule(&["-i", "lo", "-j", "ACCEPT"]));
        assert_eq!(argv.join(" "), "-A UC_FIREWALL -i lo -j ACCEPT");
    }

    #[test]
    fn the_ipv4_policy_keeps_ssh_mesh_and_loopback_and_drops_the_rest() {
        let mut settings = Settings::default();
        settings.allow.push("tcp:443".into());
        settings.allow.push("udp:53:10.0.0.0/8".into());

        let rules = joined(
            &inbound_rules(
                Family::V4,
                &settings,
                2657,
                &mesh_config(),
                &kube::Settings::default(),
            )
            .unwrap(),
        );

        assert!(rules[0].contains("-i lo"), "{rules:?}");
        assert!(
            rules
                .iter()
                .any(|r| r.contains("--ctstate ESTABLISHED,RELATED"))
        );
        assert!(rules.iter().any(|r| r.contains("--dport 51820")));
        assert!(rules.iter().any(|r| r == "-i wg-uc -j ACCEPT"));
        assert!(rules.iter().any(|r| r.contains("--dport 2657")));
        assert!(
            rules
                .iter()
                .any(|r| r.contains("--dport 443") && r.contains("-j ACCEPT"))
        );
        assert!(
            rules
                .iter()
                .any(|r| r.contains("--dport 53") && r.contains("-s 10.0.0.0/8"))
        );
        assert_eq!(rules.last().unwrap(), "-j DROP");
    }

    #[test]
    fn the_ipv6_policy_uses_icmpv6_and_skips_ipv4_only_rules() {
        let mut settings = Settings::default();
        settings.allow.push("tcp:443".into());
        settings.allow.push("tcp:8080:10.0.0.0/8".into());
        settings.allow.push("tcp:8443:2001:db8::/32".into());

        let rules = joined(
            &inbound_rules(
                Family::V6,
                &settings,
                22,
                &Config::default(),
                &kube::Settings::default(),
            )
            .unwrap(),
        );

        assert!(
            rules.iter().any(|r| r.contains("-p ipv6-icmp")),
            "{rules:?}"
        );
        assert!(rules.iter().any(|r| r.contains("--sport 547")));
        assert!(rules.iter().any(|r| r.contains("--dport 22")));
        // Source-less rule applies to both families.
        assert!(rules.iter().any(|r| r.contains("--dport 443")));
        // An IPv4 source only exists in the IPv4 policy.
        assert!(
            !rules.iter().any(|r| r.contains("--dport 8080")),
            "{rules:?}"
        );
        // An IPv6 source exists here.
        assert!(
            rules
                .iter()
                .any(|r| r.contains("--dport 8443") && r.contains("-s 2001:db8::/32"))
        );
        assert_eq!(rules.last().unwrap(), "-j DROP");
    }

    #[test]
    fn a_host_without_the_mesh_only_opens_ssh() {
        let rules = joined(
            &inbound_rules(
                Family::V4,
                &Settings::default(),
                22,
                &Config::default(),
                &kube::Settings::default(),
            )
            .unwrap(),
        );
        assert!(!rules.iter().any(|r| r.contains("51820")), "{rules:?}");
        assert!(!rules.iter().any(|r| r.contains("wg-uc")), "{rules:?}");
        assert!(rules.iter().any(|r| r.contains("--dport 22")));
        assert!(rules.iter().any(|r| r.contains("--sport 67")));
    }

    #[test]
    fn a_kubernetes_controller_keeps_its_api_open_before_the_drop() {
        let kube = kube::Settings {
            enabled: true,
            ..kube::Settings::default()
        };
        let rules = joined(
            &inbound_rules(
                Family::V4,
                &Settings::default(),
                22,
                &Config::default(),
                &kube,
            )
            .unwrap(),
        );
        assert!(
            rules.iter().any(|r| r.contains("--dport 6443")),
            "{rules:?}"
        );
        assert!(rules.iter().any(|r| r == "-i cni0 -j ACCEPT"), "{rules:?}");
        assert_eq!(rules.last().unwrap(), "-j DROP");
    }

    #[test]
    fn settings_round_trip_through_toml() {
        let mut settings = Settings {
            ssh_port: Some(2657),
            ..Settings::default()
        };
        settings.allow.push("tcp:443".into());

        let text = settings.encode().unwrap();
        let back = Settings::parse(&text).unwrap();
        assert!(back.enabled);
        assert_eq!(back.ssh_port, Some(2657));
        assert_eq!(back.allow, vec!["tcp:443".to_string()]);
    }
}
