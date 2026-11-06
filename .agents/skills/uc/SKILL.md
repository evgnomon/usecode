---
name: uc
description: Using the `uc` command line tool of usecode to run machines and the things on them. Use for virtual machines (`uc vm`, local KVM/QEMU or on Hetzner, DigitalOcean, OVHcloud, UpCloud with --provider), workspaces (`uc ws`), port forwards (`uc forward`), the usecode daemon and k3s on hosts (`uc daemon`, `uc kube`), the WireGuard mesh, firewall and IPsec (`uc net`), DNS records (`uc dns`), S3 buckets (`uc fs`), mail (`uc mail`), secrets, certificates and profiles (`uc secret`, `uc secret cert`, `uc profile`), Debian packages, container images and databases (`uc deb`, `uc image`, `uc db`), configuring this machine (`uc sync`), scaffolding (`uc new`), terminal windows (`uc terminal`) and the agent's settings (`uc agent`). Triggers include "make a VM", "spin up a server on Hetzner", "ssh into the vm", "snapshot it", "start my workspace", "add a DNS record", "copy this to the bucket", "set up k3s on these hosts", "open port 443", "read my mail", "sync my machine", "publish the deb".
---

# uc

`uc` is the one command of usecode: with no arguments it is the coding agent,
with a command it manages machines and what runs on them. This file is the
map; the details of each command are in `references/` next to it. Read the
one for the command at hand before running it.

- `references/vm.md`: `uc vm`, local VMs (sudo, ssh, virtiofs mounts,
  snapshots, forks) and cloud VMs with `--provider` (options, quirks,
  credentials).
- `references/ws.md`: `uc ws` workspaces on the cluster, and `uc forward`.
- `references/cluster.md`: `uc daemon` (install, reload, register) and
  `uc kube` (enable, connect, replace, remove, sync, logs, usage).
- `references/net.md`: `uc net mesh`, `firewall`, `ipsec`, `dig`.
- `references/dns.md`: `uc dns` zones and records.
- `references/fs.md`: `uc fs` buckets and objects.
- `references/mail.md`: `uc mail` reading, sending, and the mail server.
- `references/secret.md`: `uc secret`, `uc secret cert`, `uc profile`.
- `references/packages.md`: `uc deb`, `uc image`, `uc db`.
- `references/machine.md`: `uc sync`, `uc new`, `uc terminal`.
- `references/agent.md`: `uc agent`.

The installed help is the source of truth when it and a reference disagree:
`uc -h`, `uc <command> -h`, `uc <command> <sub> -h`.

## Things that hold everywhere

- Flags: the command line wins over `UC_<FLAG>` in the environment, which
  wins over `uc:` in `~/.config/usecode/config.yaml`.
- Commands that talk to the cluster (`ws`, `forward`, `kube`, `dns`, `fs`,
  `mail` server commands) take `--context NAME` or `UC_KUBE_CONTEXT`;
  without it, the current kubeconfig context.
- HOST arguments (`daemon`, `kube`, `net firewall`) are an inventory name or
  any ssh destination, including the aliases `uc vm` and `uc ws` write.
- Credentials come from the environment, else from the secrets of the
  repository you run in (`uc secret get -r`), so `cd` to the right one.
- Local VMs, `uc secret cert trust` and `uc sync`'s root tasks need sudo: use
  `sudo -n`, and if it wants a password, give the user the command.

## Careful

- `-h` or `--help` anywhere on a command line only prints help and never
  runs the command, at every level (`uc vm remove -h`, `uc kube enable -h`,
  `uc deb publish -h`). If one ever does something else, the uc on PATH is
  older than this skill: stop and tell the user.
- Things that keep running need `background: true`: `uc forward`,
  `uc kube logs -f`, `uc kube usage -w`, `uc agent serve`, `api`, `mcp`.
- Things that open an editor, a picker or ask on the terminal can't run from
  the agent: `uc secret edit`, `uc secret encrypt`/`decrypt`/`backup` with a
  password, `uc mail view` without `-n` or ids, `uc mail send` without a
  body and `-y`, `uc agent login`, `uc image run`. Pass what makes them
  non-interactive, or give the command to the user.
- Look before you change, where there is a way: `uc sync -C`,
  `uc kube sync -C`, `uc dns sync -C`, `uc mail configure -C`,
  `uc db migrate --dry-run`.
- `ask: true` for anything that costs money, deletes or can lock you out:
  cloud `vm create`/`remove`, `vm remove`, `ws down --purge`, `kube remove`,
  `replace`, `disable --purge`, `dns zone rm`/`rm`/`sync --prune`, `fs rm`,
  `fs rb`, `mail account rm`, `mail send`, `net firewall enable`,
  `deb publish`, `db pg kickstart --force`.
- Never echo a secret's value back unless the user asked for it.
