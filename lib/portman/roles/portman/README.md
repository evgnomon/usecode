portman
=======

Applies the mesh topology to one host: builds the `portman` binary on the
control node and copies it over (no Rust toolchain needed on the target),
installs `wireguard-tools`/`iproute2`/`iptables`, installs the host's
WireGuard credentials from the vault, renders `/etc/portman/config.toml`
from the topology, and enables the systemd service.

The role decides nothing. Which hosts exist, what address each holds and
what key it uses all come from the inventory (`deploy/inventory`), which
is why two hosts can't end up on the same address: the address is
allocated once, centrally, by `portman add`, and this role only applies
what it finds. It refuses to run against a host the topology doesn't
describe rather than inventing values for it.

Requirements
------------

- Rust toolchain on the **control node** (the binary is cross-compiled
  there, not on the target), plus the target it builds for:
  `rustup target add x86_64-unknown-linux-musl` for amd64,
  `aarch64-unknown-linux-musl` for arm64, and a linker for that target
  when it isn't the control node's own architecture. Build artifacts are
  cached per architecture under `~/.cache/portman/build`. Set
  `portman_rust_target` to a `-gnu` target instead if you would rather
  link against the target's glibc.
- The vault password, since each host's private key comes out of
  `group_vars/portman/secrets.yml`.
- Debian-family targets.

What comes from where
---------------------

Per host, written by `portman add` into `host_vars/<host>.yml`:

- `portman_address` — the host's tunnel address, e.g. `10.10.0.3`, bare
  (the prefix comes from `portman_network`).
- `portman_public_key` — the public half of the keypair minted for it.
- `portman_endpoint` — `host:port` others should dial, if it has one.

Per host, added by hand afterwards:

- `portman_services` — a list of `{name, protocol?, local_port,
  remote_bind?, peer?}`. A bare `local_port` declares "I run this here";
  `remote_bind` plus `peer` declares "public traffic arriving here goes
  to that mesh member", and the member's address is resolved from the
  topology (use `client_address` instead to point outside the mesh).
- `ansible_host`, `ansible_user`, `portman_arch`, and any other
  Ansible var.

Mesh-wide, in `group_vars/portman/main.yml`: `portman_network`,
`portman_address_start`, `portman_interface`, `portman_listen_port`,
`portman_persistent_keepalive`, `portman_mtu`.

Secret, in the vaulted `group_vars/portman/secrets.yml`:
`portman_private_keys`, one mapping from inventory hostname to WireGuard
private key for the whole mesh, so a play resolves the running host's key
with `portman_private_keys[inventory_hostname]`.

Everything else — install paths, packages, the build cache — is in
`defaults/main.yml`.

Peers are not configured anywhere: the role derives them, giving every
host a `[[peer]]` for every other member of the `portman` group, with
`endpoint` and `persistent_keepalive` set only towards members that
publish an endpoint. That is what keeps both sides of every link in
agreement without anyone maintaining two copies of it.

Example Playbook
-----------------

    - hosts: portman
      become: true
      any_errors_fatal: true
      roles:
        - role: portman

See `deploy/playbooks/portman.yml`, which is exactly this.

License
-------

MIT
