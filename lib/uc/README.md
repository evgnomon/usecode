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
in place, so an interrupted rotation can simply be run again.

`uc secret` replaces seven tools, and `make install` links their names to
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
