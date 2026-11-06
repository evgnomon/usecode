# uc dns

Zones and records on your own name servers (PowerDNS on the cluster, one per
worker). Every command takes `--context NAME` or `UC_KUBE_CONTEXT`.

```bash
uc dns servers                               # the instances and where port 53 answers
uc dns zones                                 # the zones, and those not every server has
uc dns zone add example.com                  # ns1, ns2, ... in the zone, one per worker
uc dns zone add example.com --ns ns1.other.net --ns ns2.other.net
uc dns zone rm example.com                   # the zone and all its records, everywhere
uc dns list example.com                      # the records, marking disagreements
uc dns add example.com @ A 192.0.2.10        # add, keeping the records it has
uc dns add example.com www CNAME @
uc dns add example.com @ MX '10 mail'
uc dns add example.com @ A 192.0.2.10 192.0.2.11 --ttl 300
uc dns set example.com @ TXT 'v=spf1 mx -all' --ttl 300   # replace that type's records
uc dns rm example.com www CNAME              # all of a name's records of a type
uc dns rm example.com @ A 192.0.2.11         # just these
uc dns sync --from pdns-1 -C                 # what the others are missing
uc dns sync --from pdns-1 -y [--prune] [example.com]
```

- A name without a dot is in the zone (`mail` is `mail.example.com`), one
  with dots is a full name, `@` is the zone itself.
- TXT contents are quoted for you; pass them plain.
- `add` keeps what is there, `set` replaces the records of that type.
- TTL defaults to the one there, or 3600.
- If a name server was down during a change, `uc dns sync --from <one that
  has it>` catches it up. Run with `-C` first; `--prune` also deletes zones
  the source doesn't have.

`zone rm`, `rm` and `sync --prune` delete: `ask: true`. Check with
`uc net dig` or `dig @<server>` that a change answers.
