<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# uc net mesh

`uc net mesh` makes a program on one machine reachable through an IP address and
port on another machine, over a WireGuard mesh.

There's no client/server "mode" to pick. Every host runs the same commands;
what a host actually does - hold a public endpoint, forward traffic to a
peer, or just run its own program - falls out of its config, not a flag you
set up front. That means you can bootstrap any host first, in any order, and
grow the mesh to more hosts later without touching the ones already running.

There are two ways to run it, and they answer the same question
differently. **Managed** (below) keeps the whole mesh in one topology file
and hands out addresses from there. **By hand** (further down) has you
name every address yourself, which is fine for two machines and is where
addresses start to collide once there are more.

## Install

```sh
cargo build -p uc-daemon --release     # the binary lands in ../../target/release/uc-net-mesh
```

That is all the control node needs - no Ansible run required to install
anything.

## The usecode daemon

Every host runs one service, `usecode.service`, which runs one binary:
`usecoded run`. What it does is the sum of its modules, and the mesh is
the first of them - more (think cluster setup, log collection) slot in
next to it as the daemon grows, without a new service or a new binary.

Each module knows what it needs and gets the host there by itself. The
mesh module waits for a config and a key in `/etc/uc`; once they show
up it installs WireGuard (and iptables, if you forward ports), and brings
the tunnel up. If something isn't there yet it leaves a note in the
journal and tries again a minute later - it never takes the service
down. It converges again straight away on `systemctl reload usecode`,
and whenever its config changes.

The daemon also listens on a small control socket,
`/run/usecode/usecoded.sock`, so your own tools can nudge it too - even
from another machine over ssh:

```sh
uc daemon reload edge          # from your machine: a host in the inventory, or any ssh alias
ssh root@edge usecoded reload  # the same thing by hand
# mesh: ok
```

It answers once every module has had its turn, one line each, and exits
non-zero if any of them hit an error - handy in scripts. The socket is
root-only, same as `systemctl reload`. If you'd rather not go through
`usecoded`, it's one line in and the answer back:
`echo reload | socat - UNIX-CONNECT:/run/usecode/usecoded.sock`.

### The firewall

The daemon also closes the host down. One of its modules owns an inbound
`iptables` chain (`UC_FIREWALL`) and, by default, drops everything except
SSH - no settings file needed. It applies to IPv4 and IPv6, and keeps the
loopback interface, established traffic, the DHCP client, and the
WireGuard listen port and tunnel interface open, so turning it on doesn't
break the mesh or the host's own network.

Open a port, or move SSH off 22, from wherever you run `uc`:

```sh
uc net firewall status edge                 # what's open, and the live rules
uc net firewall allow edge tcp:443          # from anywhere
uc net firewall allow edge udp:8000-8100:10.0.0.0/8
uc net firewall allow edge tcp:443:2001:db8::/32   # IPv6 source
uc net firewall ssh-port edge 2657          # sshd isn't on 22
```

Each of those writes `/etc/uc/firewall.toml` on the host and reloads the
daemon there, which reapplies the chain. The module never touches a rule
it doesn't own, so anything else on the host keeps its own firewall
rules. Because the daemon is the rules' owner now, it also turns off an
enabled `nftables.service` (whose config usually `flush ruleset`s
everything) so nothing wipes them out from under it. Stop the daemon and
the chain goes away, so a host you can't reach otherwise doesn't stay
locked.

## Quick install

Got a box with systemd and ssh access? One command puts the daemon on
it, binary and systemd service:

```sh
uc daemon install edge root@203.0.113.10
```

Run it from anywhere inside a checkout of this repo. It checks that the
host is reachable and runs systemd, adds it to the inventory, builds a
static `uc-net-mesh` for the host's architecture, copies it over and runs
`usecoded setup` there, which installs the binary and the unit. No
vault, no keys, no addresses. Run it again for the same name and it just
reinstalls - the service is restarted only when something changed.
`uc daemon install --all` does that for every host, which is how a new
version of the daemon rolls out.

Building for the host needs the musl target on your machine, once:

```sh
rustup target add x86_64-unknown-linux-musl     # aarch64-unknown-linux-musl for arm hosts
```

Already have the box as an ssh alias (servers made with usecode.dev get
one in `~/.ssh/config.d/`)? Then the name is all it needs:

```sh
uc daemon install worker-2
```

The mesh starts out off. When you want the host in it, add it to the
topology and hand every member the result:

```sh
uc net mesh add edge -endpoint vpn.example.com   # -endpoint only for a public host
uc net mesh apply
```

Automatic package installs use apt-get, so they happen on Debian-family
hosts; anywhere else, install `wireguard-tools` (and `iptables` for
forwards) yourself and the daemon picks them up on its next start.

The rest of this page is what that does under the hood.

## Managed: one topology, addresses handed out from it

The mesh lives in `deploy/inventory` - a multi-file Ansible inventory
that is the single place recording which hosts exist and what address
each one holds:

```text
deploy/inventory/hosts.yml                       who is in the mesh
deploy/inventory/group_vars/usecode/main.yml     the inputs you set: network, port, MTU
deploy/inventory/group_vars/usecode/secrets.yml  every host's private key (ansible-vault)
deploy/inventory/host_vars/<host>.yml            one host's address, public key, endpoint, services
```

An address can only be picked safely by something that can see every
other host, so nothing picks one on the host itself. `uc net mesh add` does
it centrally:

```sh
# a host with a public IP others dial
uc net mesh add edge -endpoint vpn.example.com -ansible-host 203.0.113.10

# a host behind NAT, which only dials out
uc net mesh add laptop -ansible-host 198.51.100.4
```

Each `add` reads the whole topology, takes the lowest free address in
`usecode_network`, mints that host's WireGuard keypair, and records it:
the address and public key into `host_vars/<host>.yml`, the host into the
`usecode` group, and the private key into the vaulted `secrets.yml` under
the host's name. Nothing is touched on the host itself yet, and the same
address is never handed out twice - it can't be, since there is only one
place that hands them out.

Declare what should be reachable by editing the host's `host_vars` file -
name the mesh member, not its address:

```yaml
# host_vars/edge.yml - public port 80 goes to laptop's 8080
usecode_services:
  - name: web
    protocol: tcp
    remote_bind: "0.0.0.0:80"
    peer: laptop
    local_port: 8080

# host_vars/laptop.yml - "I run web on 8080"
usecode_services:
  - name: web
    protocol: tcp
    local_port: 8080
```

Then hand every member the topology:

```sh
uc net mesh apply
```

That works out each member's `config.toml` from the whole topology on
your machine, takes its private key out of the vault, and drops both on
the host with `usecoded join`, which reloads the daemon - and the
daemon's mesh module sets the mesh up from there. A host that's down doesn't stop the
rest; run `apply` again for it later. Peers are never written
down: each host gets a `[[peer]]` for every other member automatically,
with an `endpoint` only towards the ones that publish one, so the two
sides of a link cannot drift apart.

To see what the mesh is actually doing afterwards, there is a read-only
playbook (run it from this directory, where `ansible.cfg` is):

```sh
ansible-playbook deploy/playbooks/status.yml
```

It reports, per host, the installed version, the service and config
state, the tunnel interface, any DNAT rules, and - for every host the
topology says should be a peer - how long ago it handshaked and whether
it answers a ping over the tunnel. It changes nothing, and a host that is
down is reported as down instead of ending the run.

Growing the mesh is `uc net mesh add phone` and another
`uc net mesh apply`; every existing host picks the new member up as a
peer. Nothing else has to be edited.

The vault password comes from `deploy/vault-pass.sh`, which `ansible.cfg`
names as the `vault_password_file`; being executable, it is run and its
stdout used, so the password stays in whatever `getsecret` reads and
never lands on disk here. `uc net mesh add` and `uc daemon install` run
`ansible-vault` from this directory, so they resolve the password the
same way. The vault is only opened once a host has the mesh on. Swap the body of
that script for your own secret store, or comment the setting out and
uncomment `ask_vault_pass` to be prompted instead.

## By hand: expose one port through a public host

Two machines: `laptop` runs a program on `127.0.0.1:8080`; `edge` has a
public IP and should serve it on port `80`. Here you pick the addresses,
so keeping track of which are taken is on you - that is the part the
managed flow above takes over.

**1. Generate each host's keypair and exchange descriptors.** Both machines
need the `uc-net-mesh` binary plus `wireguard-tools`, `iproute2` and `iptables`
already installed - `uc-net-mesh` does not install them. Order doesn't matter -
run these in either order, on either machine:

```sh
# on edge (has a public IP others can dial)
sudo uc net mesh export -address 10.10.0.1/24 -endpoint vpn.example.com:51820 -out edge.peer.toml

# on laptop (behind NAT, dials out - no -endpoint)
sudo uc net mesh export -address 10.10.0.2/24 -out laptop.peer.toml
```

Copy `edge.peer.toml` to `laptop`, and `laptop.peer.toml` to `edge` (scp,
chat, USB stick - it's not secret, no private key is ever in it).

**2. Import each other as peers:**

```sh
# on laptop
sudo uc net mesh import edge.peer.toml

# on edge
sudo uc net mesh import laptop.peer.toml
```

**3. Declare the service.** On `laptop`, say what's running locally; on
`edge`, say where public traffic should go:

```sh
# on laptop: "I run web on my port 8080"
sudo uc net mesh forward web tcp 8080

# on edge: "public port 80 forwards to laptop's port 8080"
sudo uc net mesh forward web tcp 80:laptop:8080
```

**4. Bring it up, on both:**

```sh
sudo uc net mesh up
```

`edge` sees a forward rule in its own config and turns on IP forwarding and
DNAT automatically - nothing else told it to. Now open `http://vpn.example.com/`
from another machine. Make sure `edge`'s firewall allows UDP `51820` and TCP
`80` in from the internet.

## Growing the mesh

Adding a third host (say `phone`, also served through `edge`) doesn't touch
`laptop` at all:

```sh
# on phone
sudo uc net mesh export -address 10.10.0.3/24 -out phone.peer.toml
# copy phone.peer.toml to edge, edge.peer.toml to phone

# on edge
sudo uc net mesh import phone.peer.toml
sudo uc net mesh forward api tcp 443:phone:9000

# on phone
sudo uc net mesh import edge.peer.toml
sudo uc net mesh forward api tcp 9000
sudo uc net mesh up

# on edge, to pick up the new peer/service
sudo uc net mesh reload
```

A host can hold public endpoints for some peers while being a plain leaf of
another - there's nothing that stops one config from having both kinds of
`[[service]]` entries.

## Command reference

Fleet (on the control node, inside a checkout - `add` touches only the topology,
`apply` delivers it to the members):

```text
uc net mesh add NAME [-endpoint HOST[:PORT]] [-address IP] [-ansible-host HOST]
                   [-ansible-user USER] [-inventory DIR] [-vault-password-file FILE]
                                     put a host into the mesh: allocate its address,
                                     mint its keypair, record it in the inventory
uc net mesh apply [NAME...]         hand every member (or those named) its key and
                                     config; its daemon sets the mesh up from there
```

`-address` pins a host to a specific address instead of the next free one,
and is refused if it is taken or outside `usecode_network`. `-endpoint`
without a port gets `usecode_listen_port`.

Setup (on the host, mutate its config):

```text
uc net mesh export  [-out FILE] [-address CIDR] [-endpoint HOST:PORT]
                                     write this host's descriptor
uc net mesh import  DESCRIPTOR_FILE add the host behind a descriptor as a peer
uc net mesh forward NAME PROTO PORT
uc net mesh forward NAME PROTO [BIND:]PORT:PEER:PEER_PORT
                                     declare a service, or a forward rule to a peer
uc net mesh unforward NAME          remove a service/forward declaration
```

Apply (act on the config already on disk):

```text
sudo uc net mesh up     bring up the tunnel, and DNAT rules for any forward rules
sudo uc net mesh down   tear down the tunnel and any DNAT rules
sudo uc net mesh reload reapply the config to a running tunnel
sudo uc net mesh status show the tunnel and forwarding state
sudo uc net mesh validate check the config file without applying it
```

Low-level (rarely needed directly - `export` calls these for you):

```text
sudo uc net mesh pubkey          print this host's WireGuard public key
sudo uc net mesh genkey [-force] (re)generate this host's WireGuard keypair
```

The `forward` port mapping reads left to right: everything before the last
two colons is where traffic arrives; `PEER:PEER_PORT` is where it goes. One
segment (`8080`) means "this is what I run"; four segments
(`0.0.0.0:443:phone:9000`) mean "this public port goes to that peer's port".

## Useful systemd commands

```text
sudo systemctl start usecode
sudo systemctl stop usecode
sudo systemctl reload usecode     # every module converges again now
sudo usecoded reload              # the same, and prints how each module did
sudo journalctl -u usecode        # what each module did, or is waiting for
```

The configuration is root-only at `/etc/uc/config.toml`. `uc-net-mesh` refuses
to use a less protected file because it may contain a WireGuard preshared
key. `export`/`import`/`forward`/`unforward` all rewrite the file in place
(via a validated encode), so hand-written comments don't survive past the
first command that touches it - `configs/*.example.toml` in this repo are
kept as annotated references instead.

## Troubleshooting

- **No connection:** confirm both descriptors were imported on the correct host (`uc net mesh validate` lists peer/service counts).
- **No handshake:** confirm the dialing side can reach the endpoint host's address:port over UDP.
- **Handshake but no web page:** confirm the local program is listening on the port named in `forward`, the edge host's firewall allows the public port, and `uc net mesh status` shows the DNAT rule.
- **Two hosts on the same address:** `uc net mesh apply` refuses to start and names both hosts. Fix the offending `host_vars` file; `uc net mesh add` won't allocate on top of a topology that already clashes either.
- **Configuration error:** run `sudo uc net mesh validate` and follow the message - it reports every problem in the config at once, not just the first one.

## For developers

- `src/main.rs` contains the CLI; `src/flags.rs` is the flag parser behind it.
- `src/inventory/` reads and extends the mesh topology (address allocation, host_vars, the vault).
  `src/inventory/yamledit.rs` adds a host to `hosts.yml` as text, so the file's comments survive.
- `src/config.rs` loads/validates/mutates `config.toml` and the peer descriptor format.
- `src/wg.rs` manages the WireGuard interface.
- `src/iptables.rs` manages DNAT/forwarding rules for hosts with forward-rule services.
- `src/keys.rs` manages this host's persistent WireGuard keypair.
- `src/remote.rs` stages files and runs commands on a host over one ssh connection (or locally).
- `cargo test` covers the parts that decide things: address allocation, config validation,
  rule building, forward-spec parsing and the `hosts.yml` edit.
- `src/inventory/render.rs` derives one host's `config.toml` from the topology.
- `src/bin/usecoded.rs` is the daemon binary: `run`, `setup`, `join`, `reload`.
- `src/daemon/` is the daemon: the loop and the `Module` trait in `mod.rs`, what every module can use
  (packages, root-owned files) in `host.rs`, the control socket (`usecoded reload`) in `control.rs`,
  and one file per module - `mesh.rs` today. A new feature is
  a new module there, listed in `modules()`.
- `src/app.rs` brings the tunnel and forwards up and down; the mesh module and `uc net mesh up` both use it.
- `src/install.rs` is `uc daemon install`: build `usecoded`, copy it, run `usecoded setup`, per host.
- `src/reload.rs` is `uc daemon reload`: run `usecoded reload` on each host over ssh.
- `src/bin/uc-daemon-ctl.rs` is the control node's `uc daemon` binary: `install`, `reload`.
- `src/setup.rs` is `usecoded setup` (install the binary and unit) and `usecoded join` (hand each bundle
  section to its module, then reload).
- `src/bundle.rs` is what the control node delivers to a host, one section per module.
- `src/apply.rs` is `uc net mesh apply`: a bundle to every member, then `join` there.
- `init/systemd/usecode.service` is the unit `setup` installs (it is compiled into the binary).
- `deploy/inventory` is the topology itself; `uc net mesh apply` hands it to every member.
- `deploy/playbooks/status.yml` reports the running state of every member back; it only reads.
