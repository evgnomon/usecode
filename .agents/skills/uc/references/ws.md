# uc ws (workspaces) and uc forward

## uc ws

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
uc ws up dev --user ada                    # another login name than $USER
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
config; `-e KEY=VALUE` sets one for this run.

`up` writes `~/.ssh/config.d/uc-ws-<name>.conf`, so `ssh <name>` (or
`ssh uc-ws-<name>`) works, as long as `~/.ssh/config` has
`Include config.d/*`; `uc ws settings` says what to add if it hasn't. From
the agent, run things there with `ssh dev 'command'` or
`uc ws ssh dev -- command`.

`down --purge` deletes everything in the home: always `ask: true`.

## uc forward

The cluster's Services on localhost, all of a namespace at once. It keeps
running, so start it with `background: true` and stop it with job_kill.

```bash
uc forward                     # the namespaces with something to forward
uc forward obs                 # metrics, logs and alerts: vmui and friends
uc forward obs rustfs          # more than one namespace
uc forward obs/vmsingle        # one Service
uc forward rustfs/rustfs:9001  # one port of it, the RustFS console
uc forward myapp               # your own apps, the same way
uc forward ws                  # what listens in your workspaces, as it comes and goes
uc forward ws/dev:3000         # one port of workspace dev, listening yet or not
```

Read its output for the local addresses it picked and give them to the user.
