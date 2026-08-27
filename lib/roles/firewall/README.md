firewall
========

Default-deny inbound iptables firewall. All TCP/UDP ports are blocked from
the outside except:

- traffic from `firewall_bastion_hosts` (unrestricted), and
- explicit exceptions listed in `firewall_allow_rules`.

Everything the role manages lives inside a single dedicated chain
(`firewall_chain`, default `USECODE_AGENT-FW`) that `INPUT` jumps to. Rules
added elsewhere by other roles/tasks (other chains, nat/mangle tables,
Docker's own chains, etc.) are left untouched.

Outbound (egress) traffic is unrestricted by default. Set
`firewall_egress_allow_rules` to restrict which destinations this host is
allowed to *initiate* TCP/UDP connections to — see below.

Applying vs. persisting, and why you should reboot after a change
--------------------------------------------------------------------

Two separate things happen when rules change:

- **Live apply** (immediate, this play run): `iptables-restore --noflush`
  on the rendered fragment — appended to the chain, never flushed. The
  fragment ends with an unconditional `-j DROP`, which is a terminating
  target: it drops the packet and never evaluates anything after it, in
  that chain or the caller. So on a rerun, the *previous* run's `-j DROP`
  is still there ahead of the newly-appended block, meaning any new
  allow-rule is inert, not doubly-applied — it doesn't take effect until
  reboot. Nothing already-allowed becomes less restrictive, and nothing
  fails open; this deliberately trades "briefly fail-open like a
  flush-then-restore would" for "briefly fail-closed on the delta."
- **Boot load** (authoritative): `netfilter-persistent.service` loads
  `/etc/iptables/rules.v4` `Before=network-pre.target` — i.e. before
  networking even comes up, so before any service could possibly be
  reached from outside. This is the one clean, complete application of
  the current rule set.

**Reboot the host after changing `firewall_bastion_hosts` or
`firewall_allow_rules`** — that's what actually cuts the new rules over.

Requirements
------------

- Debian/Ubuntu target (uses `apt` + `iptables-persistent`/
  `netfilter-persistent`).
- `python3` available on the control node (used to validate/normalize
  rule input before templating — see below).

Role Variables
---------------

- `firewall_bastion_hosts` — list of IPs/CIDRs with unrestricted inbound
  access, e.g. `["203.0.113.10", "10.10.0.0/24"]`. Bare IPs are treated
  as `/32`.
- `firewall_allow_rules` — list of exception strings, format
  `"<tcp|udp>:<port|port-range>:<source>"` (proto/port may also be
  separated with `/`). `source` may be `*` (any host) or a bare IP/CIDR.
  Examples:
  - `"tcp:8080:*"` — allow TCP/8080 from anywhere.
  - `"udp:8000:192.168.0.1/24"` — allow UDP/8000 from that subnet.
  - `"tcp:9000-9100:10.2.0.0/16"` — allow a TCP port range from a subnet.
- `firewall_chain` — name of the dedicated chain this role owns (default
  `USECODE_AGENT-FW`).
- `firewall_default_action` — terminal action for unmatched traffic
  (default `DROP`).
- `firewall_allow_loopback` / `firewall_allow_icmp` — booleans, default
  `true`.
- `firewall_rules_dir` / `firewall_rules_file` — where the rendered chain
  fragment is written on the target (default
  `/etc/iptables/<chain-lowercased>.v4`).
- `firewall_egress_allow_rules` — list of exception strings restricting
  OUTBOUND tcp/udp connections *initiated by this host*. Same format as
  `firewall_allow_rules`: `"<tcp|udp>:<port|port-range|*>:<dest>"`
  (proto/port may also be separated with `/`; port `*` means any port).
  `dest` may be `*` (any host) or a bare IP/CIDR. The special literal
  `"*/*"` means "any proto/port/dest" (explicitly unrestricted). Examples:
  - `"tcp:443:10.0.0.0/24"` — this host may only initiate TCP/443 to that
    subnet.
  - `"udp/53:*"` — this host may initiate UDP/53 to anywhere.
  - `"*/*"` — no egress restriction.
  Leave empty (the default) for unrestricted egress — no `OUTPUT` chain is
  touched at all. When non-empty, only listed tcp/udp destinations (plus
  established/related and loopback traffic) are allowed out; **non-tcp/udp
  traffic (e.g. icmp/ping) is never restricted by this list**, even when
  it's non-empty. Managed in its own chain, `firewall_egress_chain`
  (default `<firewall_chain>-OUT`), that `OUTPUT` jumps to — same
  apply/persist/reboot semantics as the inbound chain, see below.

How rules are generated and applied
------------------------------------

1. `files/generate_rules.py` runs on the control node (`delegate_to:
   localhost`) and turns `firewall_bastion_hosts`/`firewall_allow_rules`/
   `firewall_egress_allow_rules` into validated, normalized JSON (protocol
   lowercased, `*` expanded to `0.0.0.0/0`, bare IPs turned into CIDRs,
   ports/ranges checked). This keeps the parsing/validation logic out of
   `tasks/main.yml` and fails the play with a clear error on malformed
   input instead of producing a broken ruleset.
2. `templates/firewall-chain.v4.j2` renders that data into an
   `iptables-restore` fragment for just `firewall_chain`.
3. On change, a handler appends it to that chain via
   `iptables-restore --noflush` (see above for why append-only, and why
   a reboot is what actually applies the change).
4. The `INPUT -> firewall_chain` jump is managed separately via the
   `ansible.builtin.iptables` module (idempotent by itself).
5. When `firewall_egress_allow_rules` is non-empty, steps 2-4 repeat for
   egress: `templates/firewall-egress-chain.v4.j2` renders
   `firewall_egress_chain`, a handler appends it the same append-only way,
   and `OUTPUT -> firewall_egress_chain` is managed the same way as the
   inbound jump. When the list is empty, that jump is removed instead (so
   toggling it back to unrestricted takes effect on the next apply, same
   as any other rule change — see reboot note above).
6. `netfilter-persistent save` persists the live ruleset (this role's
   chain(s) plus anything else currently active) to
   `/etc/iptables/rules.v4` so it's reloaded before networking comes up on
   every boot.

Letting other roles open a port
---------------------------------

Other roles should not call `iptables` directly for inbound host ports —
add an entry to `firewall_allow_rules` instead (e.g. via `group_vars`
with list-merge `hash_behaviour`, or by including this role after
setting `firewall_allow_rules` in the play) so the exception stays
visible in one place and survives this role's idempotent rebuild.

Also run `firewall` before any role that starts a network-facing
service (e.g. `container-registry`), with that service's port already
declared in `firewall_allow_rules` — otherwise the service is exposed,
unfiltered, for however long the play takes to reach this role.

Example Playbook
-----------------

    - hosts: main
      roles:
        - role: firewall
          firewall_bastion_hosts:
            - "203.0.113.10"
          firewall_allow_rules:
            - "tcp:5000:*"          # container-registry
            - "tcp:8080:*"
            - "udp:8000:192.168.0.1/24"
          firewall_egress_allow_rules:
            - "tcp:443:10.0.0.0/24" # only reach the internal registry mirror over HTTPS
            - "udp:53:*"            # DNS from anywhere
        - role: container-registry

License
-------

MIT
