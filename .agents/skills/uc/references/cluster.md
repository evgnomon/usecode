# uc daemon and uc kube

Every host runs one service, `usecode.service` (`usecoded run`), and features
are modules of it: the WireGuard mesh, k3s, the firewall. uc only puts the
daemon on a host and hands it files; the daemon sets things up itself and
retries until it can.

HOST below is an inventory name or any ssh destination (`[USER@]HOST` or an
alias from `~/.ssh/config`, like the ones `uc vm` writes).

## uc daemon

```bash
uc daemon install web1                  # web1 is an ssh alias already
uc daemon install web1 root@203.0.113.5 # name, and where ssh reaches it
uc daemon install --all                 # reinstall every inventory host (roll out a new version)
uc daemon reload web1 web2              # converge every module now, and say how each did
uc daemon reload --all
uc daemon register web1                 # have it call home to usecode.dev (needs UC_USECODE_API_KEY)
```

`install` takes `usecoded` from the GitHub release of this uc, checked
against SHA256SUMS; run inside a usecode checkout, it builds it from the
checkout instead. Running it again reinstalls. The mesh stays off until
`uc net mesh add` and `apply` (see `net.md`).

## uc kube

k3s on your hosts, run by the daemon. The daemon has to be there first.

```bash
uc kube enable cp-1 --control           # a one-node cluster, added to kubeconfig
uc kube enable cp-1 cp-2 cp-3 --control # HA control plane (embedded etcd), founded by the first;
                                        # list the whole control plane every time you grow it
uc kube enable w-1 w-2 --to cp-1        # workers
uc kube enable cp-1 --control --version v1.31.4+k3s1   # a given k3s release
uc kube connect cp-1 [--name prod] [--server ADDR]     # add to kubeconfig and switch kubectl
uc kube status cp-1                     # settings, k3s and the nodes
uc kube replace old-1 new-1 [--via cp-1] [--no-drain]  # swap a host, same role, keeps quorum
uc kube remove w-2 [--via cp-1] [--no-drain]           # out of the cluster for good
uc kube disable w-2                     # stop k3s, keep its data
uc kube disable w-2 --purge             # uninstall k3s and delete its data
```

`remove`, `replace` and `disable --purge` change the cluster for good:
`ask: true`. Use `--via CONTROLLER` when the host can't be reached any more.

Day to day:

```bash
uc kube sync -C                         # what the cluster would get from the user config
uc kube sync                            # configure the current context (asks; -y to skip)
uc kube sync --context prod -t hcloud_csi
uc kube sync -t forgejo -e forgejo_host=git.example.com
uc kube logs                            # last 100 lines of the last hour, all containers
uc kube logs -n obs -e --since 2h       # one namespace, errors only
uc kube logs -p backend -c migrate --since 2d
uc kube logs -l app.kubernetes.io/name=web
uc kube logs timeout -healthz           # words to look for (LogsQL filters work too)
uc kube logs --json --tail 500          # for reading in a script
uc kube logs pods                       # how much each pod logged (also namespaces, nodes, fields)
uc kube usage                           # CPU, memory, disk, network per node, namespace, top pods
uc kube usage -n shop -t 20 --no-color
```

`uc kube sync` reads its settings from the user config: the top level for
every cluster, `kube.<context>` for one (e.g. `forgejo_host`,
`registry_host`, `rustfs_host`), and `-e` wins over both. Like `uc sync`, it
has `-t`, `-s`, `-l`, `-C`, `--graph`.

Don't use `-f`/`--follow` or `usage -w` without `background: true`: they
don't end.

Every cluster command takes `--context NAME` or `UC_KUBE_CONTEXT`. For
anything uc has no command for, plain `kubectl` works on the same contexts.
