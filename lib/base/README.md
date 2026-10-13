<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# base

A comfy Debian trixie dev box in a container, the one `uc workspace` (or
`uc ws`) runs for you on your cluster. You SSH in as yourself, the same
login name and uid you have on your laptop, sudo needs no password, and the usual suspects are already there: git, build-essential,
python3, curl, jq, ripgrep, tmux, vim and friends. VS Code's Remote - SSH
works out of the box.

```sh
uc ws up          # your workspace, as you, with the keys in ~/.ssh/*.pub
uc ws ssh         # and you're in
uc ws settings    # ssh config and VS Code, step by step
```

`/home` is the workspace's own volume, so your clones, dotfiles and
`~/.cargo` stay when you stop it, start it again or move to a newer image.
The SSH host keys live there too, so ssh never asks you to trust it twice.

## Build and push

```sh
make image                        # localhost/usecode-base:<commit>
make push                         # to your cluster's registry, as <commit> and latest
make push REGISTRY=registry.example.com
```

`uc ws up` uses `<registry_host>/usecode-base:latest` when your config has a
`registry_host`, so after a push `uc ws up` again picks up the new image.
Another image of your own? `uc ws up --image ...` or `workspace_image` in
your config; anything with sshd on port 22 that starts the way
`entrypoint.sh` does will do.

## What's inside

* `Containerfile`: the packages; no users.
* `entrypoint.sh`: makes the user from `USECODE_USER` and `USECODE_UID`
  (`uc ws up` sets them to you, `--user` to someone else) with sudo, fills
  its home from `/etc/skel` on a new volume, makes
  the host keys once, puts the keys from `/etc/usecode/authorized_keys`
  where sshd reads them, and runs sshd.
* `sshd_config`: keys only, no root; the entrypoint lets in only that user.
