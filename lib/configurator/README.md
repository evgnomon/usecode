<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# Getting Started

The machine is configured by `uc sync` (see `lib/uc/README.md`); its role
files and templates live in `lib/uc/roles`. On a fresh machine all
you need are the `uc` and `uc-sync` binaries:

```bash
uc sync -t usecode -d   # clones this checkout, builds and installs it
uc sync                 # everything your config's sync list asks for
uc sync -C              # dry run; e.g. -t dotfiles,git for just those roles
```

`uc sync` clones the checkout when it's missing (installing git first if
needed). Everything else is opt-in through the `sync` list of your config:
`usecode` builds the checkout and brings along what that needs (the build
packages, rustup, mise and its tools), and there's `yubikey` (udev and
polkit rules, usbip), `wsl_sudo` (sudo.ws under WSL), `vim` (vim-plug) and
the rest of the roles.

## Installation profiles

`uc sync` accepts `-p`/`INSTALL_PROFILE` (or `-e install_profile=…`) with these
values:

| Profile | Intended environment | Desktop, fonts, hardware |
| --- | --- | --- |
| `workstation` | Physical Linux desktop | Enabled |
| `dev_container` | VS Code/devcontainer or generic container | Disabled |
| `wsl` | WSL2 | Disabled |
| `vm` | Explicit VM installation | Desktop packages enabled; host-only hardware remains disabled |

The profile is selected explicitly when provided, otherwise `DEV_CONTAINER` and WSL
detection (a Microsoft kernel) are used. Dev containers omit JetBrains IDEs, Nerd Fonts, GUI applications,
QEMU/image tooling, hardware-token packages, and GNOME configuration while retaining
compilers, language servers, CLI tools, and development libraries.

# Provide user configs:

```bash
git clone ssh://git@github.com:YOURUSER/config.git ~/src/github.com/YOURUSER/config
```

`config.yaml` in there is read by `uc sync` and `uc kube sync`;
`include: [more.yaml, conf.d]` pulls in other files (a directory means its
`*.yaml`), and the including file's own keys win. `uc sync` links
`~/.config/usecode` to this repository, so the mesh inventory and the
cluster files it writes are kept there too. Already have a
`~/.config/usecode` directory? Move what's in it into the repository first.

# Dev Container Setup

```bash
DEV_CONTAINER=1 uc sync -t usecode -d
```

## WSL

Add `wsl_sudo` to your `sync` list and `uc sync` switches `sudo` to the
Windows-aware `sudo.ws`.

## Vim Plugins

After installation, run in Vim:

```
:PlugInstall
:CocInstall coc-snippets coc-prettier coc-eslint coc-tsserver coc-toml coc-rust-analyzer coc-pyright coc-go @yaegassy/coc-ruff
```

## YubiKey

### Passwordless sudo with U2F

```bash
sudo mkdir -p /etc/Yubico
pamu2fcfg | sudo tee /etc/Yubico/u2f_keys
```

Add a spare key:

```bash
pamu2fcfg -n | sudo tee -a /etc/Yubico/u2f_keys
```

### Require touch for GPG operations

```bash
ykman openpgp keys set-touch dec on
ykman openpgp keys set-touch aut on
ykman openpgp keys set-touch sig on
```

### USB passthrough (Dev Container on Windows)

Share the YubiKey with the container via WSL2 using an admin PowerShell:

```powershell
usbipd bind -i <device_id>
usbipd attach --wsl -i <device_id>
```

Find the device ID with `usbipd list`. If you get `Loading vhci_hcd failed`, run `sudo modprobe vhci_hcd` inside the container.

## Git Signing

Add pubkeys to `allowed_signers` for git verification:

```bash
echo "$(git config --get user.email) namespaces=\"git\" $(cat ~/.ssh/yourkey.pub)" >> ~/.ssh/allowed_signers
```
