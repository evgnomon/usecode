#!/usr/bin/env python3
"""Validate and normalize firewall role input into iptables-ready rule data.

Reads a JSON object from stdin:
  {"bastion_hosts": [...], "allow_rules": [...], "egress_allow_rules": [...]}

allow_rules entries look like "tcp:8080:*" or "udp:8000:192.168.0.1/24"
(proto and port may be joined with ":" or "/", e.g. "udp/8000:10.0.0.0/8").
"*" (or "any") as the source means "allow from anywhere".

egress_allow_rules entries use the same "<tcp|udp>[:/]<port|port-range|*>:<dest>"
shape (port "*" means any port), plus the special literal "*/*", which means
"allow to any destination, any proto, any port" (i.e. unrestricted egress).

Writes normalized JSON to stdout:
  {"bastion_hosts": ["a.b.c.d/32", ...],
   "allow_rules": [{"proto": "tcp", "port": "8080", "source": "0.0.0.0/0"}, ...],
   "egress_allow_rules": [{"proto": "tcp", "port": null, "dest": "10.0.0.0/24"}, ...]}

On invalid input, writes {"error": "..."} to stderr and exits 1.
"""

import ipaddress
import json
import re
import sys

RULE_RE = re.compile(
    r"^(?P<proto>tcp|udp)[:/](?P<port>\d{1,5}(?:-\d{1,5})?):(?P<source>.+)$",
    re.IGNORECASE,
)

EGRESS_RULE_RE = re.compile(
    r"^(?P<proto>tcp|udp)[:/](?P<port>\d{1,5}(?:-\d{1,5})?|\*):(?P<dest>.+)$",
    re.IGNORECASE,
)


def normalize_source(source):
    source = source.strip()
    if source.lower() in ("*", "any", "0.0.0.0/0"):
        return "0.0.0.0/0"
    if "/" not in source:
        source = f"{source}/32"
    return str(ipaddress.ip_network(source, strict=False))


def normalize_port(port):
    if "-" in port:
        lo, hi = port.split("-", 1)
        lo, hi = int(lo), int(hi)
        if not (0 < lo <= hi <= 65535):
            raise ValueError(f"invalid port range: {port}")
        return f"{lo}:{hi}"
    p = int(port)
    if not 0 < p <= 65535:
        raise ValueError(f"invalid port: {port}")
    return str(p)


def parse_allow_rule(raw):
    match = RULE_RE.match(raw.strip())
    if not match:
        raise ValueError(f"malformed allow rule: {raw!r}")
    return {
        "proto": match.group("proto").lower(),
        "port": normalize_port(match.group("port")),
        "source": normalize_source(match.group("source")),
    }


def parse_egress_rule(raw):
    raw = raw.strip()
    if raw == "*/*":
        return [
            {"proto": "tcp", "port": None, "dest": "0.0.0.0/0"},
            {"proto": "udp", "port": None, "dest": "0.0.0.0/0"},
        ]
    match = EGRESS_RULE_RE.match(raw)
    if not match:
        raise ValueError(f"malformed egress allow rule: {raw!r}")
    port = match.group("port")
    return [
        {
            "proto": match.group("proto").lower(),
            "port": None if port == "*" else normalize_port(port),
            "dest": normalize_source(match.group("dest")),
        }
    ]


def main():
    payload = json.load(sys.stdin)
    bastion_hosts = payload.get("bastion_hosts") or []
    allow_rules = payload.get("allow_rules") or []
    egress_allow_rules = payload.get("egress_allow_rules") or []

    normalized = {
        "bastion_hosts": [normalize_source(h) for h in bastion_hosts],
        "allow_rules": [parse_allow_rule(r) for r in allow_rules],
        "egress_allow_rules": [
            rule for raw in egress_allow_rules for rule in parse_egress_rule(raw)
        ],
    }
    json.dump(normalized, sys.stdout)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, TypeError, json.JSONDecodeError) as exc:
        json.dump({"error": str(exc)}, sys.stderr)
        sys.exit(1)
