# uc deb, uc image and uc db

## uc deb

```bash
uc deb build                             # package . into ./dist/<name>_<version>_<arch>.deb
uc deb build -n myapp -v 1.2.0 --depends libc6,curl -s ./pkg
uc deb publish                           # add ./dist/*.deb to the apt repository and push it
```

`build` reads `.deb.json` or `.deb.jsonc` in the source directory (`name`,
`version`, `description`, `maintainer`, `depends`, `section`, `priority`,
`homepage`, `license`, `root`, `tmpDir`); options win over it. `root` is a
directory copied to the package root (`./pkg-root/usr/bin/myapp` installs
`/usr/bin/myapp`).

`uc deb publish` takes no arguments (`-h` prints what it does). It rsyncs the repository from `shadow` with `--delete`, adds the
packages with reprepro (trixie), rsyncs it back and syncs it to the archive.
It needs the user's SSH key (often a security key to touch). Only run it when
the user asked to publish: `ask: true`.

## uc image

Container images through the bastion to the private registry, with podman:

```bash
uc image push myapp:1.2.0                # tunnel through the bastion and push
uc image pull myapp:1.2.0 [-s]           # -s also tags it without the registry host
```

The bastion, registry host, ports and user default to the user's own setup
and each has a variable (`BASTION_HOST`, `REGISTRY_HOST`, `REGISTRY_PORT`,
`CONTAINER_CLI`, ...); the registry password comes from `REGISTRY_PASSWORD`,
else the ansible-vault file `deploy/vault.yaml` of the checkout you run in.

`uc image run barge` runs the barge workflow container on the current
directory, interactively: it needs a terminal, so give it to the user.

## uc db

```bash
uc db pg kickstart [-n pg1] [-p 5432] [--force]   # PostgreSQL in podman
uc db pg db ...                          # databases; also tab, col, idx, row, schema
uc db pg schema
uc db pg dump                            # DDL only, no data
uc db pg query file.sql [-d mydb] [-f table]
echo 'select 1' | uc db pg query -d mydb
uc db migrate ./migrations --sqlite ./app.db [--dry-run]
uc db migrate ./migrations --postgres --database mydb --user admin --password secret
uc db migrate ./migrations --sqlite ./app.db --direction down
uc db resources sync | dump | configmap | schema | pods   # Kubernetes-style YAMLs in PostgreSQL
```

`kickstart --force` removes the existing container; `migrate --direction
down` rolls back. Both: `ask: true`. Run `--print-only` or `--dry-run` first.
