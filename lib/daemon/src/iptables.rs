//! Manages the DNAT/forwarding rules for the `[[service]]` entries that
//! are forward rules ([`crate::config::Service::is_forward`]). All rules
//! live in three dedicated chains, jumped to from the built-in ones, so
//! they can be applied and torn down without touching any other firewall
//! rules on the host.

use std::process::Command;

use crate::config::{Config, Service};
use crate::error::{Context, Result};
use crate::net::{join_host_port, split_host_port};
use crate::wg::run;

/// Chain names uc daemon owns exclusively, one per hook it needs.
const PRE_CHAIN: &str = "UC_DAEMON_PRE"; // nat/PREROUTING: DNAT forward rules
const POST_CHAIN: &str = "UC_DAEMON_POST"; // nat/POSTROUTING: masquerade
const FWD_CHAIN: &str = "UC_DAEMON_FWD"; // filter/FORWARD: accept tunnel traffic

/// A (table, builtin chain, owned chain) hookpoint.
const JUMPS: [(&str, &str, &str); 3] = [
    ("nat", "PREROUTING", PRE_CHAIN),
    ("nat", "POSTROUTING", POST_CHAIN),
    ("filter", "FORWARD", FWD_CHAIN),
];

/// (Re)build uc daemon's chains from `cfg`. Safe to call repeatedly; it
/// replaces any previous uc daemon ruleset atomically-enough (flush, then
/// rebuild) for a rarely-changed forwarding table.
pub fn apply(cfg: &Config) -> Result<()> {
    flush()?;

    for (table, builtin, owned) in JUMPS {
        run("iptables", &["-t", table, "-N", owned])
            .with_ctx(|| format!("create chain {owned}"))?;
        run("iptables", &["-t", table, "-A", builtin, "-j", owned])
            .with_ctx(|| format!("hook {owned} into {table}/{builtin}"))?;
    }

    for s in cfg.forwards() {
        let rule = preroute(s).with_ctx(|| format!("service {:?}", s.name))?;
        let argv: Vec<&str> = rule.iter().map(String::as_str).collect();
        run("iptables", &argv).with_ctx(|| format!("service {:?}: apply DNAT rule", s.name))?;
    }

    let iface = &cfg.interface.name;
    run(
        "iptables",
        &[
            "-t",
            "nat",
            "-A",
            POST_CHAIN,
            "-o",
            iface,
            "-j",
            "MASQUERADE",
        ],
    )
    .ctx("masquerade")?;
    run("iptables", &["-A", FWD_CHAIN, "-i", iface, "-j", "ACCEPT"]).ctx("forward accept (in)")?;
    run("iptables", &["-A", FWD_CHAIN, "-o", iface, "-j", "ACCEPT"]).ctx("forward accept (out)")?;

    Ok(())
}

/// Remove uc daemon's jump rules and chains, if present. A no-op (not an
/// error) if nothing was ever applied.
pub fn flush() -> Result<()> {
    for (table, builtin, owned) in JUMPS {
        let _ = run("iptables", &["-t", table, "-D", builtin, "-j", owned]); // may not exist
        let _ = run("iptables", &["-t", table, "-F", owned]); // must be empty before -X
        let _ = run("iptables", &["-t", table, "-X", owned]);
    }
    Ok(())
}

/// uc daemon's chains as text, for `uc daemon status`.
pub fn ruleset() -> Result<String> {
    let mut b = String::new();
    for (table, _, owned) in JUMPS {
        let out = match Command::new("iptables")
            .args(["-t", table, "-L", owned, "-n", "-v"])
            .output()
        {
            Ok(out) if out.status.success() => out,
            _ => continue, // chain doesn't exist yet - nothing forwarded
        };
        b.push_str(&String::from_utf8_lossy(&out.stdout));
        b.push('\n');
    }
    if b.is_empty() {
        bail!("no uc-daemon iptables chains found");
    }
    Ok(b)
}

fn preroute(s: &Service) -> Result<Vec<String>> {
    let (host, port) =
        split_host_port(&s.remote_bind).with_ctx(|| format!("remote_bind {:?}", s.remote_bind))?;

    let mut args: Vec<String> = ["-t", "nat", "-A", PRE_CHAIN, "-p", s.protocol_or_default()]
        .iter()
        .map(|s| s.to_string())
        .collect();
    if !host.is_empty() && host != "0.0.0.0" && host != "*" {
        args.push("-d".into());
        args.push(host);
    }
    args.extend([
        "--dport".to_string(),
        port,
        "-j".into(),
        "DNAT".into(),
        "--to-destination".into(),
        join_host_port(&s.client_address, s.local_port as u16),
        "-m".into(),
        "comment".into(),
        "--comment".into(),
        format!("uc-daemon:{}", s.name),
    ]);
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wildcard_bind_gets_no_destination_match() {
        let rule = preroute(&Service {
            name: "web".into(),
            protocol: String::new(),
            remote_bind: "0.0.0.0:443".into(),
            client_address: "10.10.0.3".into(),
            local_port: 8443,
        })
        .unwrap();

        assert!(!rule.contains(&"-d".to_string()), "{rule:?}");
        assert_eq!(rule[5], "tcp", "protocol defaults to tcp: {rule:?}");
        assert!(rule.contains(&"10.10.0.3:8443".to_string()), "{rule:?}");
        assert!(rule.contains(&"uc-daemon:web".to_string()), "{rule:?}");
    }

    #[test]
    fn a_specific_bind_address_is_matched_on() {
        let rule = preroute(&Service {
            name: "db".into(),
            protocol: "udp".into(),
            remote_bind: "203.0.113.10:5432".into(),
            client_address: "10.10.0.4".into(),
            local_port: 5432,
        })
        .unwrap();

        let d = rule.iter().position(|a| a == "-d").expect("-d missing");
        assert_eq!(rule[d + 1], "203.0.113.10");
    }
}
