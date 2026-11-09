# uc net

Mesh networks, VPNs, firewalls and DNS lookups.

## uc net mesh (WireGuard)

The usual way: from the control node, in a usecode checkout, put hosts in the
mesh and hand them their config; the daemon on each sets it up.

```bash
uc net mesh add web1 [-endpoint HOST:PORT] [-ansible-host HOST]
                                   # next free tunnel address, a keypair, into the inventory
uc net mesh apply                  # every member gets its key and config
uc net mesh apply web1 web2        # only these
```

The host needs the daemon first (`uc daemon install`, see `cluster.md`).

By hand, on a host itself (as root):

```bash
uc net mesh export -out me.desc [-address CIDR] [-endpoint HOST:PORT]
uc net mesh import other.desc                     # peer with another host
uc net mesh forward web tcp 443                   # "I run this here"
uc net mesh forward web tcp 8443:peer1:443        # forward 8443 to peer1:443
uc net mesh unforward web
uc net mesh up | down | reload | status | validate   # [-config PATH]
uc net mesh pubkey
```

Config is `/etc/uc/config.toml` (0600, root); keys are under
`/etc/uc/wireguard`. Options take one dash: `-endpoint`, `-config`.

## uc net firewall

Default-deny inbound, IPv4 and IPv6, everything but SSH closed unless allowed.
Changes go to `/etc/uc/firewall.toml` on the host and the daemon reloads.

```bash
uc net firewall status web1
uc net firewall allow web1 tcp:443 tcp:80
uc net firewall allow web1 udp:8000-8100:10.0.0.0/8
uc net firewall unallow web1 tcp:80          # matched as written
uc net firewall ssh-port web1 2222           # keep this port open for SSH
uc net firewall enable web1                  # turn default-deny on
uc net firewall disable web1                 # open the host
```

Before `enable` on a remote host, check with `status` that the SSH port is
right, or you lock yourself out: `ask: true`.

## uc net ipsec

A full-mesh strongSwan VPN with certificates, run on each host:

```bash
sudo -n uc net ipsec --local-ip 203.0.113.1 --local-id @alpha \
  --virtual-ip 192.168.88.10/32 \
  --peer beta 203.0.113.2 @beta --peer gamma 203.0.113.3 @gamma \
  --cert-file alphaCert.pem --key-file alphaKey.pem --ca-file caCert.pem [--install]
```

Certificates can come from `uc secret cert` (see `secret.md`).

## uc net dig

`dig` that prints only the A records: `uc net dig example.com`. Other `dig`
arguments pass through.
