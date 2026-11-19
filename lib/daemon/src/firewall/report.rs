// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc net firewall status` in words: the live rules of [`CHAIN`] in both
//! families, read back from `iptables -S`, told as what gets in, from
//! where and why, rather than as iptables arguments.
//!
//! Kept pure: the caller gathers the rules and what is known about the
//! host (its SSH port, mesh and Kubernetes settings, the names of the
//! addresses), and this turns them into text.

use std::collections::{BTreeSet, HashMap};

use super::{CHAIN, Family, parse_allow};
use crate::kube;

/// One rule of the live chain, as `iptables -S` prints it.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LiveRule {
    pub proto: Option<String>,
    pub dport: Option<String>,
    pub sport: Option<String>,
    pub source: Option<String>,
    pub iface: Option<String>,
    pub ctstate: Option<String>,
    pub target: String,
    /// The rule as printed, for what can't be put in words.
    pub line: String,
}

/// The `-A <CHAIN>` rules of `iptables -S <CHAIN>` output, in order.
pub fn parse_chain(out: &str) -> Vec<LiveRule> {
    let prefix = format!("-A {CHAIN} ");
    out.lines()
        .filter_map(|line| line.trim().strip_prefix(&prefix).map(|rest| (line, rest)))
        .map(|(line, rest)| {
            let mut rule = LiveRule {
                line: line.trim().to_string(),
                ..LiveRule::default()
            };
            let words: Vec<&str> = rest.split_whitespace().collect();
            let mut i = 0;
            while i < words.len() {
                let value = words.get(i + 1).map(|w| w.to_string());
                let slot = match words[i] {
                    "-p" => &mut rule.proto,
                    "--dport" => &mut rule.dport,
                    "--sport" => &mut rule.sport,
                    "-s" => &mut rule.source,
                    "-i" => &mut rule.iface,
                    "--ctstate" => &mut rule.ctstate,
                    "-j" => {
                        rule.target = value.unwrap_or_default();
                        i += 2;
                        continue;
                    }
                    // `-m <match>` and anything else this chain doesn't use.
                    _ => {
                        i += 2;
                        continue;
                    }
                };
                *slot = value;
                i += 2;
            }
            rule
        })
        .collect()
}

/// What is known about the host, to tell its rules apart.
#[derive(Debug, Default)]
pub struct Host {
    pub name: String,
    pub enabled: bool,
    pub ssh_port: u16,
    /// The `allow` entries of the settings.
    pub allow: Vec<String>,
    /// Whether the host runs a Kubernetes controller (its API is open).
    pub kube_api: bool,
    /// The other Kubernetes nodes' addresses, as the settings list them.
    pub kube_peers: Vec<String>,
    /// The WireGuard mesh's interface and listen port, when it is on.
    pub mesh: Option<(String, u16)>,
    /// Host names by address, to name a source.
    pub names: HashMap<String, String>,
}

/// One family's live rules: `None` when they could not be read (no
/// chain, no tool), with the reason.
pub struct Live {
    pub family: Family,
    pub rules: Result<Vec<LiveRule>, String>,
    /// Whether `INPUT` jumps to the chain, so the rules are in force.
    pub hooked: bool,
}

/// A line of the report: what gets in, from where, and why.
#[derive(Debug, Clone, PartialEq)]
struct Row {
    port: String,
    from: Vec<String>,
    why: String,
    /// The rule matched interfaces, which `from` lists.
    iface: bool,
    families: BTreeSet<&'static str>,
    /// The host a Kubernetes node rule lets in, by name, when known.
    peer: Option<String>,
}

enum Kind {
    /// Let in on every host, whatever the settings: loopback, replies...
    Always(&'static str),
    Row {
        port: String,
        from: String,
        why: String,
        /// Rows from interface rules join into one, listing the interfaces.
        iface: bool,
        peer: Option<String>,
    },
    Drop,
    Other,
}

fn family_name(family: Family) -> &'static str {
    match family {
        Family::V4 => "IPv4",
        Family::V6 => "IPv6",
    }
}

/// An address as a person would write it: no `/32` or `/128`, with the
/// host's name when it has one.
fn address(host: &Host, source: &str) -> String {
    let bare = source
        .strip_suffix("/32")
        .or_else(|| source.strip_suffix("/128"))
        .unwrap_or(source);
    match host.names.get(bare) {
        Some(name) => format!("{name} ({bare})"),
        None => bare.to_string(),
    }
}

/// `9000:9100` as `9000-9100`.
fn port(proto: &str, port: &str) -> String {
    format!("{proto} {}", port.replace(':', "-"))
}

fn classify(host: &Host, rule: &LiveRule) -> Kind {
    let r = rule;
    let bare = |r: &LiveRule| r.source.is_none() && r.iface.is_none() && r.ctstate.is_none();
    match r.target.as_str() {
        "ACCEPT" => {}
        "DROP" if bare(r) && r.proto.is_none() && r.dport.is_none() => return Kind::Drop,
        _ => return Kind::Other,
    }
    if r.iface.as_deref() == Some("lo") {
        return Kind::Always("loopback");
    }
    if r.ctstate.is_some() {
        return Kind::Always("replies to connections this host opened");
    }
    let proto = r.proto.as_deref().unwrap_or_default();
    if matches!(proto, "icmp" | "ipv6-icmp") && r.dport.is_none() && bare(r) {
        return Kind::Always("ping and other ICMP");
    }
    if proto == "udp"
        && matches!(
            (r.sport.as_deref(), r.dport.as_deref()),
            (Some("67"), Some("68")) | (Some("547"), Some("546"))
        )
    {
        return Kind::Always("DHCP replies");
    }
    let row = |port: String, from: String, why: &str, iface: bool| Kind::Row {
        port,
        from,
        why: why.to_string(),
        iface,
        peer: None,
    };
    if let Some(iface) = &r.iface {
        if r.proto.is_none() && r.source.is_none() {
            if kube::POD_INTERFACES.contains(&iface.as_str()) {
                return row("any".into(), iface.clone(), "Kubernetes pods", true);
            }
            if host.mesh.as_ref().is_some_and(|(name, _)| name == iface) {
                return row("any".into(), iface.clone(), "WireGuard mesh tunnel", true);
            }
        }
        return Kind::Other;
    }
    if let Some(source) = &r.source
        && r.proto.is_none()
        && r.dport.is_none()
    {
        let bare = source
            .strip_suffix("/32")
            .or_else(|| source.strip_suffix("/128"));
        if host
            .kube_peers
            .iter()
            .any(|p| p == source || Some(p.as_str()) == bare)
        {
            return Kind::Row {
                port: "any".into(),
                from: address(host, source),
                why: "Kubernetes node".into(),
                iface: false,
                peer: host.names.get(bare.unwrap_or(source)).cloned(),
            };
        }
        return Kind::Other;
    }
    let Some(dport) = &r.dport else {
        return Kind::Other;
    };
    let from = r
        .source
        .as_deref()
        .map_or_else(|| "anyone".to_string(), |s| address(host, s));
    // An allow entry wins over the guesses below: it says why itself.
    let allowed = host.allow.iter().any(|spec| {
        parse_allow(spec).is_ok_and(|a| {
            a.proto == proto
                && a.port == *dport
                && match (&a.source, &r.source) {
                    (None, None) => true,
                    (Some(want), Some(have)) => {
                        want == have
                            || format!("{want}/32") == *have
                            || format!("{want}/128") == *have
                    }
                    _ => false,
                }
        })
    });
    if allowed {
        return row(port(proto, dport), from, "allowed by you", false);
    }
    if r.source.is_none() {
        if proto == "tcp" && *dport == host.ssh_port.to_string() {
            return row(port(proto, dport), from, "SSH", false);
        }
        if proto == "tcp" && host.kube_api && *dport == kube::API_PORT.to_string() {
            return row(port(proto, dport), from, "Kubernetes API", false);
        }
        if proto == "udp"
            && host
                .mesh
                .as_ref()
                .is_some_and(|(_, p)| *dport == p.to_string())
        {
            return row(port(proto, dport), from, "WireGuard mesh", false);
        }
    }
    Kind::Other
}

/// Whether a host's firewall filters at all.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    /// On, and its rules are in force.
    On,
    /// Turned off in the settings.
    Off,
    /// On in the settings, but no rules are loaded.
    NotApplied,
    /// The host could not be asked, and why.
    Unreachable(String),
}

/// One host's live rules, summed up.
#[derive(Debug)]
pub struct View {
    pub name: String,
    pub state: State,
    rows: Vec<Row>,
    always: Vec<&'static str>,
    /// Rules usecode didn't write, as printed, with their family.
    other: Vec<(String, &'static str)>,
    problems: Vec<String>,
    has_allow: bool,
}

impl View {
    /// A host that could not be asked.
    pub fn unreachable(name: &str, why: String) -> View {
        View {
            name: name.to_string(),
            state: State::Unreachable(why),
            rows: Vec::new(),
            always: Vec::new(),
            other: Vec::new(),
            problems: Vec::new(),
            has_allow: false,
        }
    }

    /// Whether it has rules to show.
    fn filters(&self) -> bool {
        !self.rows.is_empty() || !self.always.is_empty() || !self.other.is_empty()
    }
}

/// Sum up a host's live rules: what gets in, what is always let in, what
/// usecode didn't write, and what is wrong.
pub fn view(host: &Host, live: &[Live]) -> View {
    let mut rows: Vec<Row> = Vec::new();
    let mut always: Vec<&'static str> = Vec::new();
    let mut other: Vec<(String, &'static str)> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    let mut drops = BTreeSet::new();

    for fam in live {
        let fname = family_name(fam.family);
        let rules = match &fam.rules {
            Ok(rules) => rules,
            Err(why) => {
                problems.push(format!("{fname} rules can't be read: {why}"));
                continue;
            }
        };
        if rules.is_empty() {
            continue;
        }
        if !fam.hooked {
            problems.push(format!(
                "the {fname} rules exist but INPUT doesn't jump to {CHAIN}, so they are not in force"
            ));
        }
        for rule in rules {
            match classify(host, rule) {
                Kind::Always(what) => {
                    if !always.contains(&what) {
                        always.push(what);
                    }
                }
                Kind::Drop => {
                    drops.insert(fname);
                }
                Kind::Other => other.push((rule.line.clone(), fname)),
                Kind::Row {
                    port,
                    from,
                    why,
                    iface,
                    peer,
                } => {
                    let found = rows.iter_mut().find(|row| {
                        row.port == port && row.why == why && (iface || row.from == [from.clone()])
                    });
                    match found {
                        Some(row) => {
                            if !row.from.contains(&from) {
                                row.from.push(from);
                            }
                            row.families.insert(fname);
                        }
                        None => rows.push(Row {
                            port,
                            from: vec![from],
                            why,
                            iface,
                            families: BTreeSet::from([fname]),
                            peer,
                        }),
                    }
                }
            }
        }
    }
    let applied: Vec<&str> = live
        .iter()
        .filter(|fam| fam.rules.as_ref().is_ok_and(|r| !r.is_empty()))
        .map(|fam| family_name(fam.family))
        .collect();
    for fname in &applied {
        if !drops.contains(fname) {
            problems.push(format!(
                "the {fname} rules don't end in a drop, so everything else gets in too"
            ));
        }
    }
    let state = if !host.enabled {
        if !applied.is_empty() {
            problems.insert(
                0,
                "the firewall is off, but its rules are still on the host".into(),
            );
        }
        State::Off
    } else if applied.is_empty() {
        State::NotApplied
    } else {
        State::On
    };
    // What anyone may reach first, then the rest.
    rows.sort_by_key(|row| row.from != ["anyone"]);
    View {
        name: host.name.clone(),
        state,
        rows,
        always,
        other,
        problems,
        has_allow: !host.allow.is_empty(),
    }
}

fn from_text(row: &Row) -> String {
    let joined = row.from.join(", ");
    if row.iface {
        let s = if row.from.len() > 1 { "s" } else { "" };
        format!("interface{s} {joined}")
    } else {
        joined
    }
}

fn row_line(row: &Row, width: usize) -> String {
    let only = if row.families.len() == 1 {
        format!(
            " ({} only)",
            row.families.first().copied().unwrap_or_default()
        )
    } else {
        String::new()
    };
    format!(
        "  {:<10} {:<width$}  {}{only}\n",
        row.port,
        from_text(row),
        row.why
    )
}

fn width<'a>(rows: impl Iterator<Item = &'a Row>) -> usize {
    rows.map(|row| from_text(row).len())
        .max()
        .unwrap_or(0)
        .max(4)
}

/// The report for one host: the state, what gets in, what is always let
/// in, and what is set by hand.
pub fn render(host: &Host, live: &[Live]) -> String {
    let v = view(host, live);
    let name = &v.name;
    let mut out = String::new();
    match &v.state {
        State::Off => {
            out.push_str(&format!(
                "{name}: firewall off. Every port is open to anyone (uc net firewall enable {name}).\n"
            ));
            if !v.filters() {
                return out;
            }
        }
        State::NotApplied => {
            out.push_str(&format!(
                "{name}: firewall on, but no rules are in force yet: every port is open.\n  \
                 Is the daemon running? (uc daemon reload {name})\n"
            ));
            for problem in &v.problems {
                out.push_str(&format!("  ! {problem}\n"));
            }
            return out;
        }
        State::Unreachable(why) => {
            out.push_str(&format!("{name}: can't be reached: {why}\n"));
            return out;
        }
        State::On => out.push_str(&format!(
            "{name}: firewall on. Inbound traffic is dropped unless a line below lets it in.\n"
        )),
    }

    out.push('\n');
    let width = width(v.rows.iter());
    out.push_str(&format!("  {:<10} {:<width$}  FOR\n", "PORT", "FROM"));
    for row in &v.rows {
        out.push_str(&row_line(row, width));
    }
    out.push('\n');
    if !v.always.is_empty() {
        out.push_str(&format!("  Always let in: {}.\n", v.always.join(", ")));
    }
    if !v.has_allow {
        out.push_str(&format!(
            "  Your own rules: none yet (e.g. uc net firewall allow {name} tcp:443).\n"
        ));
    }
    out.push_str(&other_rules(&v, "  "));
    for problem in &v.problems {
        out.push_str(&format!("  ! {problem}\n"));
    }
    out
}

fn other_rules(v: &View, indent: &str) -> String {
    let mut out = String::new();
    if !v.other.is_empty() {
        out.push_str(&format!(
            "{indent}Rules usecode didn't write (the chain is rebuilt on every reload):\n"
        ));
        for (line, fname) in &v.other {
            out.push_str(&format!("{indent}  {line}  [{fname}]\n"));
        }
    }
    out
}

fn names(hosts: &[&str]) -> String {
    hosts.join(", ")
}

/// The report for many hosts, with as little said twice as can be: the
/// rules every host has first, then the ones a group of hosts shares,
/// then each host's own. A Kubernetes node rule counts as on a host that
/// is that node itself, which never lets itself in by address, so the
/// rules that let the nodes in to each other are listed once.
pub fn render_cluster(views: &[View]) -> String {
    let mut out = String::new();
    let filtering: Vec<&View> = views.iter().filter(|v| v.filters()).collect();
    let all: BTreeSet<&str> = filtering.iter().map(|v| v.name.as_str()).collect();

    // The headline: where the firewall filters, and where it doesn't.
    let with = |state: fn(&State) -> bool| -> Vec<&str> {
        views
            .iter()
            .filter(|v| state(&v.state))
            .map(|v| v.name.as_str())
            .collect()
    };
    let on = with(|s| *s == State::On);
    let off = with(|s| *s == State::Off);
    let pending = with(|s| *s == State::NotApplied);
    if !on.is_empty() {
        out.push_str(&format!(
            "Firewall on at {}: inbound traffic is dropped unless a line below lets it in.\n",
            names(&on)
        ));
    }
    if !off.is_empty() {
        out.push_str(&format!(
            "Firewall off at {}: every port is open (uc net firewall enable HOST).\n",
            names(&off)
        ));
    }
    if !pending.is_empty() {
        out.push_str(&format!(
            "Firewall on but no rules in force yet at {}: every port is open \
             (is the daemon running? uc daemon reload HOST).\n",
            names(&pending)
        ));
    }
    for v in views {
        if let State::Unreachable(why) = &v.state {
            out.push_str(&format!("Can't reach {}: {why}\n", v.name));
        }
    }

    // Which hosts have each row. A node rule is on its own node too.
    let mut groups: Vec<(Row, BTreeSet<&str>)> = Vec::new();
    for v in &filtering {
        for row in &v.rows {
            match groups.iter_mut().find(|(r, _)| r == row) {
                Some((_, hosts)) => {
                    hosts.insert(v.name.as_str());
                }
                None => groups.push((row.clone(), BTreeSet::from([v.name.as_str()]))),
            }
        }
    }
    // The sets of hosts where a node rule was counted on its own node.
    let mut folded: Vec<BTreeSet<&str>> = Vec::new();
    for (row, hosts) in &mut groups {
        if let Some(peer) = &row.peer
            && let Some(peer) = all.get(peer.as_str())
            && hosts.insert(peer)
        {
            folded.push(hosts.clone());
        }
    }
    // The shared "always let in" list, and what differs per host.
    let mut always: Vec<&str> = filtering
        .first()
        .map(|v| v.always.clone())
        .unwrap_or_default();
    always.retain(|what| filtering.iter().all(|v| v.always.contains(what)));

    // Sets of hosts, the whole cluster first, then the larger groups.
    let mut sets: Vec<BTreeSet<&str>> = Vec::new();
    for (_, hosts) in &groups {
        if !sets.contains(hosts) {
            sets.push(hosts.clone());
        }
    }
    sets.sort_by(|a, b| {
        (*b == all)
            .cmp(&(*a == all))
            .then(b.len().cmp(&a.len()))
            .then(a.cmp(b))
    });
    let width = width(groups.iter().map(|(row, _)| row));

    let mut every_done = false;
    for set in &sets {
        let hosts: Vec<&str> = set.iter().copied().collect();
        out.push('\n');
        if *set == all && all.len() > 1 {
            out.push_str("On every host:\n");
        } else if hosts.len() == 1 {
            out.push_str(&format!("Only on {}:\n", hosts[0]));
        } else {
            out.push_str(&format!("On {}:\n", names(&hosts)));
        }
        let mut rows: Vec<&Row> = groups
            .iter()
            .filter(|(_, h)| h == set)
            .map(|(row, _)| row)
            .collect();
        // Open to anyone first, the nodes last and by name.
        rows.sort_by_key(|row| (row.from != ["anyone"], row.peer.clone()));
        for row in rows {
            out.push_str(&row_line(row, width));
        }
        if *set == all {
            every_done = true;
            if !always.is_empty() {
                out.push_str(&format!("  Always let in: {}.\n", always.join(", ")));
            }
        }
        if folded.contains(set) {
            out.push_str("  (Each node lets in the others; none lists itself.)\n");
        }
    }
    if !every_done && !always.is_empty() && !filtering.is_empty() {
        out.push_str(&format!(
            "\nOn every host:\n  Always let in: {}.\n",
            always.join(", ")
        ));
    }

    // What is particular to one host: other rules, differences, problems.
    let mut notes = String::new();
    for v in &filtering {
        let extra: Vec<&str> = v
            .always
            .iter()
            .copied()
            .filter(|what| !always.contains(what))
            .collect();
        let mut lines = String::new();
        if !extra.is_empty() {
            lines.push_str(&format!("    Also always let in: {}.\n", extra.join(", ")));
        }
        lines.push_str(&other_rules(v, "    "));
        for problem in &v.problems {
            lines.push_str(&format!("    ! {problem}\n"));
        }
        if !lines.is_empty() {
            notes.push_str(&format!("  {}:\n{lines}", v.name));
        }
    }
    for v in views.iter().filter(|v| v.state == State::NotApplied) {
        for problem in &v.problems {
            notes.push_str(&format!("  {}:\n    ! {problem}\n", v.name));
        }
    }
    if !notes.is_empty() {
        out.push_str(&format!("\nNotes:\n{notes}"));
    }
    if !filtering.is_empty() && filtering.iter().all(|v| !v.has_allow) {
        out.push_str(
            "\nYour own rules: none on any host yet (e.g. uc net firewall allow HOST tcp:443).\n",
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const V4: &str = "-N UC_FIREWALL
-A UC_FIREWALL -i lo -j ACCEPT
-A UC_FIREWALL -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
-A UC_FIREWALL -p icmp -j ACCEPT
-A UC_FIREWALL -p udp -m udp --sport 67 --dport 68 -j ACCEPT
-A UC_FIREWALL -p tcp -m tcp --dport 6443 -j ACCEPT
-A UC_FIREWALL -i cni0 -j ACCEPT
-A UC_FIREWALL -i flannel.1 -j ACCEPT
-A UC_FIREWALL -i flannel-wg -j ACCEPT
-A UC_FIREWALL -s 2.31.7.49/32 -j ACCEPT
-A UC_FIREWALL -s 37.27.150.180/32 -j ACCEPT
-A UC_FIREWALL -p tcp -m tcp --dport 22 -j ACCEPT
-A UC_FIREWALL -p tcp -m tcp --dport 443 -j ACCEPT
-A UC_FIREWALL -j DROP
";

    const V6: &str = "-N UC_FIREWALL
-A UC_FIREWALL -i lo -j ACCEPT
-A UC_FIREWALL -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
-A UC_FIREWALL -p ipv6-icmp -j ACCEPT
-A UC_FIREWALL -p udp -m udp --sport 547 --dport 546 -j ACCEPT
-A UC_FIREWALL -p tcp -m tcp --dport 6443 -j ACCEPT
-A UC_FIREWALL -i cni0 -j ACCEPT
-A UC_FIREWALL -i flannel.1 -j ACCEPT
-A UC_FIREWALL -i flannel-wg -j ACCEPT
-A UC_FIREWALL -p tcp -m tcp --dport 22 -j ACCEPT
-A UC_FIREWALL -p tcp -m tcp --dport 443 -j ACCEPT
-A UC_FIREWALL -j DROP
";

    fn host() -> Host {
        Host {
            name: "cp-1".into(),
            enabled: true,
            ssh_port: 22,
            allow: vec!["tcp:443".into()],
            kube_api: true,
            kube_peers: vec!["2.31.7.49".into(), "37.27.150.180".into()],
            mesh: None,
            names: HashMap::from([("2.31.7.49".to_string(), "cp-2".to_string())]),
        }
    }

    fn live(v4: &str, v6: &str) -> Vec<Live> {
        vec![
            Live {
                family: Family::V4,
                rules: Ok(parse_chain(v4)),
                hooked: true,
            },
            Live {
                family: Family::V6,
                rules: Ok(parse_chain(v6)),
                hooked: true,
            },
        ]
    }

    #[test]
    fn the_chain_parses_into_rules() {
        let rules = parse_chain(V4);
        assert_eq!(rules.len(), 13);
        assert_eq!(rules[0].iface.as_deref(), Some("lo"));
        assert_eq!(rules[3].sport.as_deref(), Some("67"));
        assert_eq!(rules[8].source.as_deref(), Some("2.31.7.49/32"));
        assert_eq!(rules[12].target, "DROP");
    }

    #[test]
    fn the_rules_read_as_what_gets_in() {
        let text = render(&host(), &live(V4, V6));
        assert_eq!(
            text,
            "cp-1: firewall on. Inbound traffic is dropped unless a line below lets it in.

  PORT       FROM                                    FOR
  tcp 6443   anyone                                  Kubernetes API
  tcp 22     anyone                                  SSH
  tcp 443    anyone                                  allowed by you
  any        interfaces cni0, flannel.1, flannel-wg  Kubernetes pods
  any        cp-2 (2.31.7.49)                        Kubernetes node (IPv4 only)
  any        37.27.150.180                           Kubernetes node (IPv4 only)

  Always let in: loopback, replies to connections this host opened, ping and other ICMP, DHCP replies.
"
        );
    }

    #[test]
    fn rules_it_does_not_know_are_shown_as_they_are() {
        let v4 = V4.replace(
            "-A UC_FIREWALL -j DROP",
            "-A UC_FIREWALL -p tcp -m tcp --dport 8080 -j ACCEPT",
        );
        let text = render(&host(), &live(&v4, V6));
        assert!(text.contains("-A UC_FIREWALL -p tcp -m tcp --dport 8080 -j ACCEPT  [IPv4]"));
        assert!(text.contains("! the IPv4 rules don't end in a drop"));
    }

    #[test]
    fn no_rules_means_open() {
        let text = render(&host(), &live("", ""));
        assert!(text.starts_with("cp-1: firewall on, but no rules are in force yet"));
        let off = Host {
            enabled: false,
            ..host()
        };
        assert!(render(&off, &live("", "")).starts_with("cp-1: firewall off."));
    }

    /// A node of a three-node cluster: two controllers and a worker.
    fn node(name: &str, api: bool, ip_of_others: &[&str]) -> View {
        let mut v4 = String::from("-N UC_FIREWALL\n-A UC_FIREWALL -i lo -j ACCEPT\n");
        if api {
            v4.push_str("-A UC_FIREWALL -p tcp -m tcp --dport 6443 -j ACCEPT\n");
        }
        v4.push_str("-A UC_FIREWALL -i cni0 -j ACCEPT\n");
        for ip in ip_of_others {
            v4.push_str(&format!("-A UC_FIREWALL -s {ip}/32 -j ACCEPT\n"));
        }
        v4.push_str("-A UC_FIREWALL -p tcp -m tcp --dport 22 -j ACCEPT\n-A UC_FIREWALL -j DROP\n");
        let v6 = "-N UC_FIREWALL\n-A UC_FIREWALL -i lo -j ACCEPT\n\
                  -A UC_FIREWALL -i cni0 -j ACCEPT\n\
                  -A UC_FIREWALL -p tcp -m tcp --dport 22 -j ACCEPT\n-A UC_FIREWALL -j DROP\n";
        let host = Host {
            name: name.into(),
            enabled: true,
            ssh_port: 22,
            kube_api: api,
            kube_peers: ip_of_others.iter().map(|ip| ip.to_string()).collect(),
            names: HashMap::from([
                ("10.0.0.1".to_string(), "cp-1".to_string()),
                ("10.0.0.2".to_string(), "cp-2".to_string()),
                ("10.0.0.3".to_string(), "w-1".to_string()),
            ]),
            ..Host::default()
        };
        view(&host, &live(&v4, v6))
    }

    #[test]
    fn a_cluster_lists_what_every_host_shares_once() {
        let views = vec![
            node("cp-1", true, &["10.0.0.2", "10.0.0.3"]),
            node("cp-2", true, &["10.0.0.1", "10.0.0.3"]),
            node("w-1", false, &["10.0.0.1", "10.0.0.2"]),
            View::unreachable("pi", "ssh timed out".into()),
        ];
        let text = render_cluster(&views);
        assert_eq!(
            text,
            "Firewall on at cp-1, cp-2, w-1: inbound traffic is dropped unless a line below lets it in.
Can't reach pi: ssh timed out

On every host:
  tcp 22     anyone           SSH
  any        interface cni0   Kubernetes pods
  any        cp-1 (10.0.0.1)  Kubernetes node (IPv4 only)
  any        cp-2 (10.0.0.2)  Kubernetes node (IPv4 only)
  any        w-1 (10.0.0.3)   Kubernetes node (IPv4 only)
  Always let in: loopback.
  (Each node lets in the others; none lists itself.)

On cp-1, cp-2:
  tcp 6443   anyone           Kubernetes API (IPv4 only)

Your own rules: none on any host yet (e.g. uc net firewall allow HOST tcp:443).
"
        );
    }
}
