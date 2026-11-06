# uc sync, uc new and uc terminal

## uc sync

Configures this machine: packages, repositories, dotfiles, toolchains, run as
a graph of tasks in parallel. What it does comes from the user config
(`~/.config/usecode/config.yaml`) and the built-in roles. The profile
(`workstation`, `vm`, `wsl`, `dev_container`) is detected; `-p` picks one.

```bash
uc sync -C                               # what would change; always first
uc sync                                  # everything for the detected profile
uc sync -t dotfiles,git                  # only these tags
uc sync -t dot/tmux                      # link only the tmux dotfiles
uc sync -t docker -d                     # docker and everything it needs
uc sync -s apt,extrepo                   # skip these
uc sync -t upgrade                       # also the `never` upgrade tasks
uc sync -l -t 'git/clone:*'              # list the selected tasks
uc sync --list-tags                      # every tag
uc sync -e blueprint_user=alice          # a variable for the templates
uc sync -x -v                            # stop at the first failure, show every command
uc sync --roles lib/uc/roles             # roles from a checkout instead of built in
```

Tasks that need root run through `sudo -n`; when sudo wants a password they
fail, so tell the user to run `sudo -v` first (or run it themselves). A full
run takes a while: give it a long `timeout`.

`uc kube sync` is the same for the cluster kubectl points at (see
`cluster.md`).

## uc new

```bash
uc new script hello                      # ~/.local/bin/hello, sh, executable
uc new script tool -s py -p ./bin        # sh, bash, py, python or zig
uc new unit backup /usr/local/bin/backup           # writes ./backup.service
uc new unit web /usr/bin/myapp serve -d 'My app' -o ./web.service
sudo -n uc new unit web /usr/bin/myapp serve --enable   # install to /etc/systemd/system and enable
```

## uc terminal

Browser-style windows, tabs and splits in the terminal (shell alias `t`).

```bash
uc terminal window list | new work | open work | close work | rename work play
uc terminal tab list | new logs --in work | open logs | move logs --to play | close logs
uc terminal split right | down | list | close
```

`open` and the splits act on the user's terminal; from the agent, `list` is
safe, the rest only when the user asks.
