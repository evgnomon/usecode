<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# uc

The top level `usecode` command. `uc` itself holds no subcommand logic: like
`git`, it looks up `uc-<name>` and hands the process over to it, so a new
subcommand is added by dropping a `uc-<name>` executable next to `uc` or
anywhere on `PATH`.

```sh
uc help                # list the uc-* commands found on PATH
uc encrypt secrets.txt # runs uc-encrypt
```

This crate ships the dispatcher, the two file encryption subcommands, which
replace the former `boom` shell script, and `uc configure`, the machine
configurator, `uc push`/`uc pull`, which move container images through
the registry behind the bastion, `uc ghcr`, which builds, pushes and deletes
GitHub Container Registry images, and `uc secret`, which generates secrets and
manages the ansible-vault secret stores.
The last two replace the former `lib/pylib` Python package (`bp` and the
`gh_image` Ansible module).

It also ships the command groups — `uc image`, `uc repo`, `uc cert`,
`uc deb`, `uc db`, `uc net`, `uc vm`, `uc new`, `uc cloud`, `uc agent`,
`uc data`, `uc media`, `uc pick` and `uc sys` — which gather every tool in
`lib/` under one command; see [Command groups](#command-groups).

## Encrypting files

```sh
uc encrypt secrets.txt        # -> secrets.txt.asc, secrets.txt removed
uc decrypt secrets.txt.asc    # -> secrets.txt, the .asc kept
uc decrypt -c secrets.txt.asc # print the plaintext instead of writing it
```

Both commands prompt for the password on the terminal and never echo it.

* `-f` overwrites an existing output file; without it an existing file is an
  error.
* `uc encrypt` decrypts its own output and compares it against the input before
  it removes the original, so a failed encryption never costs the plaintext.
* `uc decrypt` accepts only `.asc` files and leaves the encrypted copy in place.
* Output files are written through a temporary sibling and renamed, with mode
  `0600`, so an interrupted run cannot leave a half-written file behind.

### File format

The `.asc` file is exactly what `openssl enc -aes-256-cbc -pbkdf2 -salt -a`
produces — an 8-byte salt behind a `Salted__` magic, AES-256-CBC with PKCS#7
padding, base64 wrapped at 64 columns — so `openssl` remains a usable fallback:

```sh
openssl enc -d -aes-256-cbc -pbkdf2 -a -in secrets.txt.asc
```

`uc decrypt` also opens files written with the pre-OpenSSL-3.0 MD5 key
derivation, and says so when it does; re-encrypting upgrades them.

## Configuring the machine

`uc configure` configures the local machine — Apt sources and packages,
extrepo repositories, dotfiles, git checkouts, toolchains, desktop settings —
without Python or Ansible. Independent tasks run in parallel:

```sh
uc configure                    # everything, for the detected profile
uc configure -C                 # dry run: report what would change
uc configure -t dotfiles,git    # only these roles
uc configure -t zls -d          # zls plus everything it runs after
uc configure -l -t 'git/*'      # list tasks, their tags and dependencies
uc configure --graph | dot -Tsvg > graph.svg
```

It keeps the architecture of the Ansible playbook it replaced:

* **modules** (`src/configure/modules`) are the reusable steps, after Ansible's
  modules: `copy`, `template`, `file`, `inflate`, `command`/`shell`, `apt`,
  `git`, `make`, `systemd`, `group`, `uri`. Each is idempotent, reports
  `ok` or `changed`, and honours `--check`.
* **roles** (`src/configure/roles`) are the configuration functions, one
  per Ansible role, adding tasks built from those modules to the plan.
* the files and templates live in `roles/<role>/{files,templates}` and are
  read from the checkout. Templates render with Ansible's Jinja settings and
  see the same variables, including `ansible_facts`.

### Scheduling

Every task has an id, `<role>/<step>`, and names the tasks it runs
**after**. A task starts as soon as each of those has finished, or was left
out by the tags or its condition, with at most `-j/--jobs` tasks running at
once (default: the number of CPUs, or `UC_CONFIGURE_JOBS`). Clones,
downloads and repository setup therefore overlap, while tasks touching dpkg
queue on a shared lock.

A failed task blocks only the tasks after it; the others carry on and the
recap counts them as `blocked`. `-x/--fail-fast` stops starting new tasks
after the first failure instead. A handler becomes a task that runs after
the tasks that notify it and acts only when one of them changed something
(`sysctl/apply`, `udev_hwdb/rebuild`).

### Tags

Tags follow Ansible: `-t` runs tasks carrying any of the tags, `-s` leaves
them out, `always` tasks run unless skipped by name, and `never` tasks
(`apt/upgrade`, `git/fetch:*`) run only when asked for by an explicit tag or
their id. A task's role and id also work as tags, and tags may contain `*`.
`-d/--with-deps` also pulls in what the selected tasks run after.

### Options

| Flag | Meaning |
| --- | --- |
| `-t, --tags` / `-s, --skip-tags` | select tasks, comma separated |
| `-d, --with-deps` | also run the selected tasks' dependencies |
| `-j, --jobs N` | tasks running at the same time |
| `-p, --profile` | `workstation`, `vm`, `wsl` or `dev_container`; also `INSTALL_PROFILE`, else `dev_container` with `DEV_CONTAINER` set, `wsl` on a Microsoft kernel, or `workstation` |
| `-e KEY=VALUE` | set a template variable, like Ansible's `-e` |
| `-C, --check` | dry run |
| `-x, --fail-fast` | stop after the first failure |
| `-l, --list`, `--list-tags`, `--graph` | inspect the plan and exit |
| `-v, --verbose` | stream every command's output |
| `--roles DIR`, `--config FILE` | the role files (`lib/uc/roles`) and the user config, found automatically |

Tasks that need root run through `sudo -n`; when the selection has any,
`uc configure` asks for the password once, before the run, and keeps the
credentials fresh until it ends.

## Moving container images

`uc push` and `uc pull` replace `deploy/push.sh` and `deploy/pull.sh`. The
registry deployed by `deploy/playbooks/registry.yaml` is only reachable through
the bastion, so both open an SSH tunnel to it, log the container CLI in, move
the images and close the tunnel again:

```sh
uc push myimage:latest             # tag as localhost:5000/myimage:latest and push
uc pull myimage:latest             # pull localhost:5000/myimage:latest
uc pull --strip-host myimage:latest # ... and also tag it as myimage:latest
```

The password comes from `REGISTRY_PASSWORD`, or else from
`vault_container_registry_password` in `deploy/playbooks/vault.yaml` of the
current git checkout (`--vault-file` to use another), decrypted with
`ansible-vault`. The registry and bastion addresses, ports and users, and the
container CLI, are options with the same environment variables the scripts
read (`BASTION_HOST`, `REGISTRY_HOST`, `LOCAL_PORT`, `CONTAINER_CLI`, ...);
see `uc push -h`.

## GitHub Container Registry

```sh
uc ghcr build -o evgnomon -i ark -t feature/x --push  # ghcr.io/evgnomon/ark:feature-x
uc ghcr delete -o evgnomon -i ark -t feature/x        # remove that version
```

The token comes from `GHCR_TOKEN` (or `--token`). `build` logs the container
CLI (`CONTAINER_CLI`, default `docker`) in to `ghcr.io`, builds with `--pull`
and pushes with `--push`; `-f` and `-C` pick the Dockerfile and context.
`delete` finds the version carrying the tag (or digest) through the GitHub
packages API, for user and organization owners alike, and deletes it; it
talks to the API through `curl`, so the crate stays free of a TLS stack.
Slashes in tags become dashes, so a branch name can be passed as is. The
`z_container` Ansible role runs both.

## Secrets

```sh
uc secret gen                    # 32 characters: letters, digits and symbols
uc secret gen -l 16 --no-symbols
uc secret get                    # the default store as JSON
uc secret get -r -f github_pat   # one field of this repository's store
uc secret edit -r                # edit this repository's store in vi
uc secret ensure -r              # create it if missing, print its path
uc secret rotate NAME            # re-encrypt under a new vault password
uc secret rotate -r --playbook   # rotate this repository's secrets themselves
uc secret encrypt notes.txt      # same as uc encrypt / uc decrypt
uc secret server read acme prod db  # the SSH key authenticated secret server
uc secret serve                  # same as uc secret server serve
```

`gen` draws characters uniformly from the OS random source; the dotfiles
alias `mkpass` runs it.

The other subcommands work on the ansible-vault secret stores in
`~/src/github.com/$USER/config/secrets`. A store `NAME` is `NAME.yaml`,
encrypted with ansible-vault, and `NAME.vault.asc`, its vault password as kept
by the `vault` command; without a name it is `secrets.yaml` and `vault.asc`.
`-r` picks the current repository's store, `<org>_<repo>` from the working
directory, and a name given with it is appended (`-r github` is
`<org>_<repo>_github`). Vault passwords reach ansible-vault only through a
pipe, and stores are always edited with `vi`, the hardened `lib/vi`.

`get` converts the YAML to JSON itself (merge keys included), so `yj` is no
longer needed, and `-f a.b` prints one field the way `jq -r .a.b` would.
`rotate` moves the secret file to `NAME.yaml.bak` until the new password is
in place, so an interrupted rotation can simply be run again. With
`--playbook` it leaves the password alone and rotates the secrets themselves:
it runs `rotate.yaml` in `~/src/github.com/$USER/blueprint` with the store
as extra vars, passing anything after `--` to ansible-playbook.

`encrypt` and `decrypt` are `uc encrypt` and `uc decrypt`. `server` runs
`uc-secret-server` (was `secd`), the SSH key authenticated secret server, with
its `upsert`, `read` and `serve` commands; `serve` on its own is a shortcut
for `server serve`.

`uc secret` replaces eight tools, and `make install` links their names to
`uc-secret`, which behaves as the tool it was called as:

| Old command | Same as |
|---|---|
| `getsecret [NAME]` | `uc secret get [NAME]` |
| `keychain [NAME]` | `uc secret edit [NAME]` |
| `rchain` | `uc secret edit -r` |
| `ghchain` | `uc secret edit -r github` |
| `ensure_secret` | `uc secret ensure -r` |
| `ensure_vault` | `uc secret ensure -r --print vault` |
| `rotate_keychain_pass [NAME]` | `uc secret rotate [NAME]` |
| `rotsec [ARGS]` | `uc secret rotate -r --playbook -- [ARGS]` |

## Command groups

Every other tool in `lib/` is its own executable, named after the command
that runs it: `uc db pg` runs `uc-db-pg`, `uc cert p12 fetch` runs
`uc-cert-p12-fetch`. So you can call a tool through `uc` or straight by its
`uc-*` name, whichever reads better in a script. A group is a `uc-<group>`
executable that hands the rest of the command line to the tool, the way `uc`
hands it to the group; `uc <group>` lists its commands and flags the ones
whose tool is not installed. `uc help` lists just the groups and top level
commands, not every `uc-*` tool.

The old tool names still work for now: `make install` and `make link` put
them next to the new ones as symlinks. They go away in a later release, so
switching scripts over to the `uc` commands is a good idea.

| Command | Executable | Was |
|---|---|---|
| `uc image push`, `pull` | `uc-push`, `uc-pull` | |
| `uc image ghcr build`, `delete` | `uc-ghcr` | |
| `uc image run barge`, `yacht` | `uc-image-run-barge`, `uc-image-run-yacht` | `barge`, `yacht` |
| `uc repo fqn` | `uc-repo-fqn` | `repofqn` |
| `uc repo version` | `uc-repo-version` | `ucversion` |
| `uc repo open` | `uc-repo-open` | `gotorepo` |
| `uc repo status [DIR]` | `uc-repo-status` | `git_repos` |
| `uc repo git-config` | `uc-repo-git-config` | `set_git_conf` |
| `uc repo extract` | `uc-repo-extract` | `extract-tool` |
| `uc repo dist` | `uc-repo-dist` | `ansidist` |
| `uc repo headers check`, `fix` | `uc-repo-headers` | `hgl` |
| `uc repo authors` | built in | `scripts/authors.sh` (`make authors`) |
| `uc cert init`, `server`, `client`, ... | `uc-cert-gen`: any command not below | `certgen` |
| `uc cert trust HOST:PORT` | `uc-cert-trust` | `trust_ca` |
| `uc cert p12 KEYNAME` | `uc-cert-p12-bundle` | `mkp12` |
| `uc cert p12 fetch` | `uc-cert-p12-fetch` | `zcdump` |
| `uc deb build`, `publish` | `uc-deb-build`, `uc-deb-publish` | `mkdeb`, `pubdeb` |
| `uc db pg`, `mongo`, `migrate` | `uc-db-pg`, `uc-db-mongo`, `uc-db-migrate` | `pg`, `mgo`, `sqlize` |
| `uc db resources sync`, `dump` | `uc-db-resources-yaml sync`, `dump` | `ysys` |
| `uc db resources configmap` | `uc-db-resources-configmap` | `confmap` |
| `uc db resources schema` | `uc-db-resources-schema` | `k8s_ddl` |
| `uc db resources pods` | `uc-db-resources-pods` | `mkpod` |
| `uc net mesh` | `uc-net-mesh` | `uc-daemon` |
| `uc net ipsec`, `dig` | `uc-net-ipsec`, `uc-net-dig` | `ipmesh`, `diga` |
| `uc vm` | `uc-vm-local`, arguments and all; with `--provider`, built in (see below) | `vm` |
| `uc nats`, `uc work` | `uc-nats`, `uc-work` themselves | `natsup`, `workd` |
| `uc new role`, `workflow`, `script`, `unit` | `uc-new-role`, `uc-new-workflow`, `uc-new-script`, `uc-new-unit` | `mkarole`, `catalyze`, `shole`, `mkunit` |
| `uc cloud do`, `hcloud` | `uc-cloud-do`, `uc-cloud-hcloud` | `wdoctl`, `whcloud` |
| `uc cloud play [ARGS]` | `uc-cloud-play-run`: the repository playbook | `y` |
| `uc cloud play host`, `ssh` | `uc-cloud-play-host`, `uc-cloud-play-ssh` | `plat`, `annabelle` |
| `uc secret server` | `uc-secret-server` | `secd` |
| `uc agent api`, `mcp` | `uc-agent-api`, `uc-agent-mcp` | `usecode-agent-api`, `usecode-mcp` |
| `uc data pdf`, `tidy`, `jsonc` | `uc-data-pdf`, `uc-data-tidy`, `uc-data-jsonc` | `csv2pdf`, `tidycsv`, `jsonc` |
| `uc media backup`, `compress`, `yt` | `uc-media-backup`, `uc-media-compress`, `uc-media-yt` | `backup_archive`, `imgpress`, `ytdump` |
| `uc pick file`, `url` | `uc-pick-file`, `uc-pick-url` | `ff`, `fzurls` |
| `uc sys yubikey`, `argv` | `uc-sys-yubikey`, `uc-sys-argv` | `ykattach`, `num_argv` |
| `uc sys docker`, `vi` | `uc-sys-docker`, `uc-sys-vi` | `docker`, `vi` |

`docker` and `vi` do their job by standing in for the real ones on `PATH`, so
those two names stay installed as symlinks for good. `x` keeps its name too:
it is the shortcut for `uc configure`.

### Cloud VMs

`uc vm` takes the same commands and size options to Hetzner Cloud,
DigitalOcean, OVHcloud Public Cloud or UpCloud when given `--provider`:

```sh
sudo uc vm create uc3 --memory 4GiB --vcpus 4 --disk-size 60G  # local KVM/QEMU
uc vm create uc3 --memory 4GiB --vcpus 4 --disk-size 60G --provider hetzner
uc vm list --provider digitalocean
uc vm create uc4 --vcpus 2 --memory 8GiB --provider ovh --location GRA11
uc vm create uc5 --vcpus 2 --memory 4GiB --provider upcloud --location de-fra1
uc vm remove uc3 --provider hetzner --force
```

`create` picks the cheapest server type, across the provider's locations, with
at least the requested vCPUs, memory and disk; `--location`, `--arch` and
`--image` narrow it down. On UpCloud, `create` defaults to the `dk-cph1`
(Copenhagen) zone and the smallest plan unless `--location`, `--vcpus`,
`--memory` or `--disk-size` say otherwise. The server boots with the local VMs' cloud-init
user-data from `/etc/vm/config.yaml`, and gets an ssh_config entry
(`/etc/ssh/ssh_config.d`, or `~/.ssh/config.d` without root), so `ssh uc3`
reaches the same user either way. `list`, `info`, `inspect`, `ip`, `start`,
`stop`, `restart` and `remove` work on cloud VMs too; `snapshot`, `fork`,
`mount` and `config` stay local. The providers are driven through their REST
APIs (over `curl`, like `uc ghcr`), with the token from `HCLOUD_TOKEN` or
`DIGITALOCEAN_ACCESS_TOKEN`, else `hetzner.prod` or `doctl.prod` in the
current repository's secrets, as `uc cloud` finds it. OVHcloud signs requests
with application keys instead: `OVH_APPLICATION_KEY`, `OVH_APPLICATION_SECRET`,
`OVH_CONSUMER_KEY`, the Public Cloud project id in `OVH_CLOUD_PROJECT_SERVICE`
and optionally `OVH_ENDPOINT` (`ovh-eu`, the default, `ovh-ca` or `ovh-us`),
else `application_key`, `application_secret`, `consumer_key`, `project` and
`endpoint` under `ovh.prod` in the secrets. Its flavors are priced from the
public catalog's hourly rate over 730 hours, and it has no graceful stop, so
`stop` and `stop --force` do the same. UpCloud takes an API token from
`UPCLOUD_TOKEN` or `upcloud.prod`, else its API user from `UPCLOUD_USERNAME`
and `UPCLOUD_PASSWORD` or `username` and `password` under `upcloud.prod`. Its
plans are priced per zone from the hourly price list, capped at 672 hours a
month; servers clone a template (`Debian GNU/Linux 13` by default, matched by
title prefix or UUID), log in with the SSH keys of the cloud-init user-data,
and are stopped before `remove --force` removes them with their disks. Cloud
VMs need no sudo.

A group's commands are tables in `src/groups.rs`; a command either runs a
tool, with arguments of its own put in front of the user's, opens a nested
group, or calls into this crate. A tool is found next to `uc` first, then on
`PATH`, and replaces the group's process, so its exit status, signals and
terminal are its own.

## Build

```sh
make build    # cargo build --profile release --target x86_64-unknown-linux-musl
make check    # fmt --check, clippy -D warnings, tests
make install  # install uc and its uc-* subcommands to /usr/local/bin
```

`make install` honours `DESTDIR` (with a trailing slash) and `PREFIX`, e.g.
`make install PREFIX=$HOME/.local`.

The default target is `x86_64-unknown-linux-musl`, added with `rustup` on first
use. The crate has no C dependencies, so the result is a static-pie executable
with no shared libraries at all — it runs on any Linux of the same
architecture, including a `scratch` container, and needs no `openssl` on the
host. `TARGET=` picks another triple, e.g. a dynamically linked glibc build:

```sh
make build TARGET=x86_64-unknown-linux-gnu
```
