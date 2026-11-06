# uc vm

Virtual machines, locally with KVM/QEMU or on Hetzner, DigitalOcean, OVHcloud
and UpCloud. `uc vm -h` has every option.

## Local VMs (KVM/QEMU through libvirt)

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

## Cloud VMs: `--provider`

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

## From VM to cluster

A VM is a host like any other, so the rest of uc builds on it (see
`cluster.md` and `net.md`):

```bash
uc daemon install web1                  # the usecode daemon (web1 is already an ssh alias)
uc net mesh add web1                    # put it in the WireGuard mesh (control node, in a checkout)
uc net mesh apply web1                  # hand it its config; the daemon sets the mesh up
uc kube enable web1 --control           # a one-node k3s cluster, added to kubeconfig
uc kube enable web2 --to web1           # a worker
```

Tell the user the VM's name, IP and `ssh NAME` once it is up.
