# portman

`portman` makes a program on one machine reachable through an IP address and
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
go build -o portman ./cmd/portman
```

That is all the control node needs. `portman` itself installs nothing:
targets get WireGuard, iptables, the binary and the systemd unit from the
playbook, which is the only way portman is installed on a host.

## Managed: one topology, addresses handed out from it

The mesh lives in `deploy/inventory` - a multi-file Ansible inventory
that is the single place recording which hosts exist and what address
each one holds:

```text
deploy/inventory/hosts.yml                       who is in the mesh
deploy/inventory/group_vars/portman/main.yml     the inputs you set: network, port, MTU
deploy/inventory/group_vars/portman/secrets.yml  every host's private key (ansible-vault)
deploy/inventory/host_vars/<host>.yml            one host's address, public key, endpoint, services
```

An address can only be picked safely by something that can see every
other host, so nothing picks one on the host itself. `portman add` does
it centrally:

```sh
# a host with a public IP others dial
portman add edge -endpoint vpn.example.com -ansible-host 203.0.113.10

# a host behind NAT, which only dials out
portman add laptop -ansible-host 198.51.100.4
```

Each `add` reads the whole topology, takes the lowest free address in
`portman_network`, mints that host's WireGuard keypair, and records it:
the address and public key into `host_vars/<host>.yml`, the host into the
`portman` group, and the private key into the vaulted `secrets.yml` under
the host's name. Nothing is touched on the host itself yet, and the same
address is never handed out twice - it can't be, since there is only one
place that hands them out.

Declare what should be reachable by editing the host's `host_vars` file -
name the mesh member, not its address:

```yaml
# host_vars/edge.yml - public port 80 goes to laptop's 8080
portman_services:
  - name: web
    protocol: tcp
    remote_bind: "0.0.0.0:80"
    peer: laptop
    local_port: 8080

# host_vars/laptop.yml - "I run web on 8080"
portman_services:
  - name: web
    protocol: tcp
    local_port: 8080
```

Then converge every host:

```sh
ansible-playbook deploy/playbooks/portman.yml
```

Run it from the repo root: `ansible.cfg` there supplies the inventory and
the roles path, so there are no flags to remember.

That installs the binary, the dependencies, the host's credentials from
the vault, a `config.toml` derived from the whole topology, and the
systemd service - on every member, in one run. Peers are never written
down: each host gets a `[[peer]]` for every other member automatically,
with an `endpoint` only towards the ones that publish one, so the two
sides of a link cannot drift apart.

To see what the mesh is actually doing afterwards, there is a read-only
companion playbook:

```sh
ansible-playbook deploy/playbooks/status.yml
```

It reports, per host, the installed version, the service and config
state, the tunnel interface, any DNAT rules, and - for every host the
topology says should be a peer - how long ago it handshaked and whether
it answers a ping over the tunnel. It changes nothing, and a host that is
down is reported as down instead of ending the run.

Growing the mesh is `portman add phone` and another
`ansible-playbook deploy/playbooks/portman.yml`; every existing host picks the
new member up as a peer. Nothing else has to be edited.

The vault password comes from `deploy/vault-pass.sh`, which `ansible.cfg`
names as the `vault_password_file`; being executable, it is run and its
stdout used, so the password stays in whatever `getsecret` reads and
never lands on disk here. `portman add` runs `ansible-vault` from the
repo root too, so it resolves the password the same way. Swap the body of
that script for your own secret store, or comment the setting out and
uncomment `ask_vault_pass` to be prompted instead.

## By hand: expose one port through a public host

Two machines: `laptop` runs a program on `127.0.0.1:8080`; `edge` has a
public IP and should serve it on port `80`. Here you pick the addresses,
so keeping track of which are taken is on you - that is the part the
managed flow above takes over.

**1. Generate each host's keypair and exchange descriptors.** Both machines
need the `portman` binary plus `wireguard-tools`, `iproute2` and `iptables`
already installed - `portman` does not install them. Order doesn't matter -
run these in either order, on either machine:

```sh
# on edge (has a public IP others can dial)
sudo portman export -address 10.10.0.1/24 -endpoint vpn.example.com:51820 -out edge.peer.toml

# on laptop (behind NAT, dials out - no -endpoint)
sudo portman export -address 10.10.0.2/24 -out laptop.peer.toml
```

Copy `edge.peer.toml` to `laptop`, and `laptop.peer.toml` to `edge` (scp,
chat, USB stick - it's not secret, no private key is ever in it).

**2. Import each other as peers:**

```sh
# on laptop
sudo portman import edge.peer.toml

# on edge
sudo portman import laptop.peer.toml
```

**3. Declare the service.** On `laptop`, say what's running locally; on
`edge`, say where public traffic should go:

```sh
# on laptop: "I run web on my port 8080"
sudo portman forward web tcp 8080

# on edge: "public port 80 forwards to laptop's port 8080"
sudo portman forward web tcp 80:laptop:8080
```

**4. Bring it up, on both:**

```sh
sudo portman up
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
sudo portman export -address 10.10.0.3/24 -out phone.peer.toml
# copy phone.peer.toml to edge, edge.peer.toml to phone

# on edge
sudo portman import phone.peer.toml
sudo portman forward api tcp 443:phone:9000

# on phone
sudo portman import edge.peer.toml
sudo portman forward api tcp 9000
sudo portman up

# on edge, to pick up the new peer/service
sudo portman reload
```

A host can hold public endpoints for some peers while being a plain leaf of
another - there's nothing that stops one config from having both kinds of
`[[service]]` entries.

## Command reference

Fleet (on the control node, inside a checkout - touches the topology, not
any host):

```text
portman add NAME [-endpoint HOST[:PORT]] [-address IP] [-ansible-host HOST]
                 [-ansible-user USER] [-inventory DIR] [-vault-password-file FILE]
                                   put a host into the mesh: allocate its address,
                                   mint its keypair, record it in the inventory
```

`-address` pins a host to a specific address instead of the next free one,
and is refused if it is taken or outside `portman_network`. `-endpoint`
without a port gets `portman_listen_port`.

Setup (on the host, mutate its config):

```text
portman export  [-out FILE] [-address CIDR] [-endpoint HOST:PORT]
                                   write this host's descriptor
portman import  DESCRIPTOR_FILE   add the host behind a descriptor as a peer
portman forward NAME PROTO PORT
portman forward NAME PROTO [BIND:]PORT:PEER:PEER_PORT
                                   declare a service, or a forward rule to a peer
portman unforward NAME            remove a service/forward declaration
```

Apply (act on the config already on disk):

```text
sudo portman up       bring up the tunnel, and DNAT rules for any forward rules
sudo portman down     tear down the tunnel and any DNAT rules
sudo portman reload   reapply the config to a running tunnel
sudo portman status   show the tunnel and forwarding state
sudo portman validate check the config file without applying it
```

Low-level (rarely needed directly - `export` calls these for you):

```text
sudo portman pubkey            print this host's WireGuard public key
sudo portman genkey [-force]   (re)generate this host's WireGuard keypair
```

The `forward` port mapping reads left to right: everything before the last
two colons is where traffic arrives; `PEER:PEER_PORT` is where it goes. One
segment (`8080`) means "this is what I run"; four segments
(`0.0.0.0:443:phone:9000`) mean "this public port goes to that peer's port".

## Useful systemd commands

```text
sudo systemctl start portman
sudo systemctl stop portman
sudo systemctl reload portman
sudo journalctl -u portman
```

The configuration is root-only at `/etc/portman/config.toml`. portman refuses
to use a less protected file because it may contain a WireGuard preshared
key. `export`/`import`/`forward`/`unforward` all rewrite the file in place
(via a validated encode), so hand-written comments don't survive past the
first command that touches it - `configs/*.example.toml` in this repo are
kept as annotated references instead.

## Troubleshooting

- **No connection:** confirm both descriptors were imported on the correct host (`portman validate` lists peer/service counts).
- **No handshake:** confirm the dialing side can reach the endpoint host's address:port over UDP.
- **Handshake but no web page:** confirm the local program is listening on the port named in `forward`, the edge host's firewall allows the public port, and `portman status` shows the DNAT rule.
- **Two hosts on the same address:** the playbook stops on "Assert every mesh address is unique" and prints host → address for the whole group. Fix the offending `host_vars` file; `portman add` won't allocate on top of a topology that already clashes either.
- **Configuration error:** run `sudo portman validate` and follow the message - it reports every problem in the config at once, not just the first one.

## For developers

- `cmd/portman` contains the CLI.
- `internal/inventory` reads and extends the mesh topology (address allocation, host_vars, the vault).
- `internal/config` loads/validates/mutates `config.toml` and the peer descriptor format.
- `internal/wg` manages the WireGuard interface.
- `internal/iptables` manages DNAT/forwarding rules for hosts with forward-rule services.
- `internal/keys` manages this host's persistent WireGuard keypair.
- `init/systemd/portman.service` is the unit the playbook installs.
- `roles/portman` applies the topology to one host.
- `deploy/inventory` is the topology itself; `deploy/playbooks/portman.yml` applies it to all of them.
- `deploy/playbooks/status.yml` reports the running state of every member back; it only reads.
