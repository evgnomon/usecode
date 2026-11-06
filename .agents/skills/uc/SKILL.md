---
name: uc
description: Using the `uc` command line tool of usecode to run machines and the things on them. Use when the user wants to create, start, stop, snapshot, fork, mount into or remove a virtual machine (`uc vm`, local KVM/QEMU or Hetzner, DigitalOcean, OVHcloud, UpCloud), or to work with their hosts and cluster through uc - the daemon, k3s (`uc kube`), the WireGuard mesh (`uc net`), DNS records (`uc dns`), S3 buckets (`uc fs`), workspaces (`uc ws`), port forwards (`uc forward`), secrets, certificates, mail, Debian packages, local databases or configuring this machine (`uc sync`). Triggers include "make a VM", "spin up a server on Hetzner", "ssh into the vm", "snapshot it", "add a DNS record", "copy this to the bucket", "set up k3s on these hosts", "sync my machine".
---

# uc

`uc` is the one command of usecode: with no arguments it is the coding agent,
with a command it manages machines and what runs on them. Each command is a
group with its own help, and the help is the source of truth for the
installed version, so read it before using a command you haven't run yet:

```bash
uc -h                 # the commands
uc vm -h              # one command's options and examples
uc kube enable -h     # a subcommand's
```

## The commands

- `vm`: virtual machines, locally with KVM/QEMU or on a cloud provider. See below.
- `daemon`: `uc daemon install HOST` puts `usecoded` on a host; `reload` makes it converge now.
- `kube`: k3s on your hosts, run by the daemon. `uc kube enable HOST --control`,
  `uc kube enable HOST --to CONTROLLER`, `connect`, `replace OLD NEW`, `remove HOST`.
- `net`: `mesh` (WireGuard mesh, `uc net mesh apply`), `ipsec`, `firewall`, `dig`.
- `dns`: records on your own name servers: `uc dns add example.com www CNAME @`,
  `set`, `rm`, `list`, `zones`, `sync --from <server>`.
- `fs`: S3 buckets on the cluster: `uc fs ls`, `mb`, `cp [-r] SRC bucket:path`, `cat`, `rm`, `rb`.
- `forward`: cluster services on localhost: `uc forward obs`, `uc forward ws/dev:3000`.
- `workspace` / `ws`: a dev container on the cluster: `uc ws up`, `ls`, `ssh`, `stop`, `start`, `down [--purge]`.
- `secret`: `gen`, `get`, `ensure`, `encrypt`/`decrypt`, `backup`/`restore` of ~/.ssh, ~/.gnupg, ~/.kube.
- `cert`: local CA, server and client certificates, `trust` a TLS endpoint's CA.
- `mail`: `pull`, `view`, `send`, and the mail server's `domain`, `account`, `configure`.
- `db`: `pg` (PostgreSQL instances), `migrate`, `resources`.
- `deb`: `build` packages into ./dist, `publish` them.
- `image`: `push`/`pull` container images through the bastion, `run` workflow containers.
- `sync`: configure this machine. `uc sync -C` first to see what would change,
  `-t TAGS` to pick, `-l` to list tasks.
- `profile`, `new` (script/unit scaffolds), `terminal` (windows, tabs, splits), `agent`.

Commands that talk to the cluster take `--context NAME` (or `UC_KUBE_CONTEXT`).
Every flag can also be `UC_<FLAG>` in the environment or set under `uc:` in
`~/.config/usecode/config.yaml`; the command line wins, then the environment.

## uc vm

### Local VMs (KVM/QEMU through libvirt)

Local VMs go through `qemu:///system` and write `/etc/ssh/ssh_config.d`, so
they run under sudo. The agent can't type a password: use `sudo -n uc vm ...`,
and if sudo wants a password, give the user the command to run.

```bash
sudo -n uc vm create dev                                   # 2 vCPUs, 1GiB, 10G disk
sudo -n uc vm create dev --memory 4GiB --vcpus 4 --disk-size 40G
sudo -n uc vm create dev --mount ~/src/myproj               # share a host dir, tag = basename
sudo -n uc vm create dev --mount ~/src/myproj:src           # ... with tag src
sudo -n uc vm create dev --no-start                         # define only
sudo -n uc vm list -a                                       # all, not only running
sudo -n uc vm inspect dev                                   # RAM, disk, IP
sudo -n uc vm ip dev
sudo -n uc vm stop dev [--force]  /  start  /  restart
sudo -n uc vm config dev --memory 8GiB                      # resize memory
sudo -n uc vm remove dev [--force]                          # --force if it runs
```

`create` waits for the IP (unless `--no-wait-ip`) and writes an ssh_config
entry, so once it returns, `ssh dev` works. The user is `vm` (or `username:`
in `/etc/vm/config.yaml`) with passwordless sudo, logging in with
`~/.ssh/id_ed25519`. Run things on the VM with `ssh dev 'command'`; pass
`-o BatchMode=yes -o StrictHostKeyChecking=accept-new` so nothing prompts. A
fresh VM may take a few seconds after the IP shows until sshd answers, and
cloud-init may still be running: `ssh dev cloud-init status --wait`.

Shared directories are virtiofs. `--mount` at create, or later with
`uc vm mount dev ~/src/myproj [--tag src]`, then inside the VM:

```bash
ssh dev 'sudo mkdir -p /mnt/src && sudo mount -t virtiofs src /mnt/src'
```

Snapshots and forks, for trying something risky and going back:

```bash
sudo -n uc vm snapshot create dev clean
sudo -n uc vm snapshot list dev
sudo -n uc vm snapshot restore dev clean
sudo -n uc vm snapshot remove dev clean
sudo -n uc vm fork dev dev2 [--memory 2GiB --vcpus 4]       # a new VM from dev's disk
```

Base images live in `/usr/share/vm/images` (`--image <path>` for another);
settings in `/etc/vm/config.yaml`.

### Cloud VMs: `--provider`

`--provider <local|hetzner|digitalocean|ovh|upcloud>` says where the VM runs;
`local` is the default. It is a command line option only (no `UC_` variable),
and it goes on every command for that VM, not only on `create`: without it,
`uc vm list` or `uc vm remove web1` look at the local machines. No sudo needed
for cloud VMs.

`create` takes the same size options as locally (`--vcpus`, `--memory`,
`--disk-size`) and picks the cheapest server type with at least those, across
the provider's locations. It boots the server with the local VMs' cloud-init
user-data from `/etc/vm/config.yaml` (same `vm` user, same SSH key) and writes
an ssh_config entry (`/etc/ssh/ssh_config.d`, or `~/.ssh/config.d` without
root), so `ssh NAME` works the same as for a local VM.

Cloud options for `create`:

- `--location <code>` (or `--region`): Hetzner location (`fsn1`, `nbg1`,
  `hel1`, ...), DigitalOcean region (`fra1`, `ams3`, ...), OVHcloud region
  (`GRA11`, ...) or UpCloud zone (`de-fra1`, ...). Default: the cheapest one;
  on UpCloud `dk-cph1`.
- `--arch <x86|arm>`: default `x86`.
- `--image <name>`: the provider's image. Defaults: `debian-13` (Hetzner),
  `debian-13-x64` (DigitalOcean), `Debian 13` (OVHcloud), the
  `Debian GNU/Linux 13` template (UpCloud, matched by title prefix or UUID).

```bash
uc vm create web1 --memory 4GiB --vcpus 2 --disk-size 40G --provider hetzner
uc vm create arm1 --vcpus 4 --memory 8GiB --provider hetzner --arch arm --location fsn1
uc vm create web2 --vcpus 2 --memory 4GiB --provider digitalocean --location fra1
uc vm create web3 --vcpus 2 --memory 8GiB --provider ovh --location GRA11
uc vm create web4 --provider upcloud --location de-fra1   # smallest plan
uc vm list --provider hetzner
uc vm inspect web1 --provider hetzner
uc vm ip web1 --provider hetzner
uc vm stop web1 --provider hetzner      # start, restart the same way
uc vm remove web1 --provider hetzner --force
```

What works where: `list`, `info`, `inspect`, `ip`, `start`, `stop`, `restart`
and `remove` work on every provider; `snapshot`, `fork`, `mount` and `config`
are local only.

Provider quirks:

- OVHcloud has no graceful stop: `stop` and `stop --force` do the same.
  Prices come from the catalog's hourly rate times 730 hours.
- UpCloud picks the smallest plan unless `--vcpus`, `--memory`,
  `--disk-size` or `--location` are given, logs in with the SSH keys in the
  cloud-init user-data (it fails if there are none), and `remove --force`
  stops the server first and removes its disks with it.

Credentials, from the environment first, else from the secrets of the
repository you run in (so `cd` to the one holding them):

- Hetzner: `HCLOUD_TOKEN`, else `hetzner.prod`.
- DigitalOcean: `DIGITALOCEAN_ACCESS_TOKEN`, else `doctl.prod`.
- OVHcloud: `OVH_APPLICATION_KEY`, `OVH_APPLICATION_SECRET`,
  `OVH_CONSUMER_KEY`, `OVH_CLOUD_PROJECT_SERVICE` and optionally
  `OVH_ENDPOINT` (`ovh-eu`, `ovh-ca`, `ovh-us`), else `application_key`,
  `application_secret`, `consumer_key`, `project`, `endpoint` under `ovh.prod`.
- UpCloud: `UPCLOUD_TOKEN`, else `upcloud.prod`; or `UPCLOUD_USERNAME` and
  `UPCLOUD_PASSWORD`, else `username` and `password` under `upcloud.prod`.

Cloud servers cost money: set `ask: true` on `create` and `remove` with a
provider, say the size, location and price you picked, and remind the user of
servers left running.

### From VM to cluster

A VM is a host like any other, so the rest of uc builds on it:

```bash
uc daemon install web1                  # the usecode daemon (web1 is already an ssh alias)
uc net mesh add web1                    # put it in the WireGuard mesh (control node, in a checkout)
uc net mesh apply web1                  # hand it its config; the daemon sets the mesh up
uc kube enable web1 --control           # a one-node k3s cluster, added to kubeconfig
uc kube enable web2 --to web1           # a worker
```

Check `uc daemon -h`, `uc net mesh -h` and `uc kube -h` for the current
arguments before running these.

## uc ws (workspaces)

`uc ws` (short for `uc workspace`) is a dev container of your own on the
cluster: Debian trixie (the `lib/base` image), you as the user with the same
login name and uid as on this machine, sudo, your `~/.ssh/*.pub` keys, and a
home on a volume of its own that survives stop, start and new images. Each is
a Deployment `ws-<name>` in the `workspace` namespace. Nothing is exposed:
SSH goes through `kubectl port-forward`, so it only needs kubectl to reach
the cluster. Every command takes `--context NAME` (or `UC_KUBE_CONTEXT`) for
another cluster than the current one.

```bash
uc ws up                                   # workspace "dev", as you, with your keys
uc ws up ada --cpu 4 --memory 8Gi --size 50Gi
uc ws up dev --image registry.example.com/me/base:latest
uc ws up dev --key ~/.ssh/other.pub        # let in another key (repeatable)
uc ws ls                                   # the workspaces and whether they run
uc ws ssh                                  # a shell in dev
uc ws ssh ada -- make test                 # a command, after --
uc ws ssh -- -L 3000:localhost:3000        # with a port brought home
uc ws settings                             # how to reach it with ssh and VS Code
uc ws settings --ssh-config                # only the ~/.ssh/config entry
uc ws stop                                 # stop it; /home stays
uc ws start
uc ws down dev                             # remove it, keep the home volume
uc ws down dev --purge                     # remove it with its home
```

The name defaults to `dev` everywhere except `down`, which wants it spelled
out. `up` makes the workspace, or updates it (image, keys, resources) and
starts it again; `--size` and `--class` only count when the home volume is
first made. Defaults for `--image`, `--size` and `--class` come from
`workspace_image`, `workspace_size` and `workspace_storage_class` in the user
config.

`up` writes `~/.ssh/config.d/uc-ws-<name>.conf`, so `ssh <name>` (or
`ssh uc-ws-<name>`) works, as long as `~/.ssh/config` has
`Include config.d/*`; `uc ws settings` says what to add if it hasn't. From
the agent, run things there with `ssh dev 'command'` or
`uc ws ssh dev -- command`.

Ports: `uc forward ws/dev` brings every port that listens in the workspace to
localhost as it opens; `uc forward ws/dev:3000` just one; `uc forward ws` all
workspaces. Start these with `background: true`, they keep running.

`down --purge` deletes everything in the home: always `ask: true`.

## Good habits

- Read `uc <command> -h` before the first use; don't guess flags.
- Prefer `-C`/`--check` where a command has it (`uc sync -C`, `uc dns sync -C`) before changing things.
- Anything that removes VMs, hosts from a cluster, DNS records, buckets or
  data, or that creates paid cloud resources, gets `ask: true`.
- Tell the user the VM's name, IP and `ssh NAME` once it is up.
