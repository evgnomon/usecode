<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# Getting Started

The machine is configured by `uc configure` (see `lib/uc/README.md`); its role
files and templates live in `lib/uc/roles`. This directory holds the bootstrap:

```bash
sudo make prepare   # install-root.sh: base apt packages, YubiKey setup
make play           # install.sh (rustup), then play.sh: build uc, run uc configure
./play.sh -C        # dry run; any uc configure flag works, e.g. -t dotfiles,git
```

## Installation profiles

`uc configure` accepts `-p`/`INSTALL_PROFILE` (or `-e install_profile=…`) with these
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
git clone ssh://git@github.com:YOURUSER/config.git ~/.config/usecode
```

# Dev Container Setup

```bash
DEV_CONTAINER=1 make play
```

## WSL

When running under WSL, switch `sudo` to the Windows-aware alternative:

```bash
sudo update-alternatives --set sudo /usr/bin/sudo.ws
```

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

## Secret Rotation

Run `uc secret rotate -r --playbook` (was `rotsec`) in your repo. Set
`repo_secrets` using `uc secret edit -r` (was `rchain`):

```yaml
repo_secrets:
  - owner_name: evgnomon
    repo_name: blueprint
    secret_name: VAULT_PASS
    secret_value: yourpass
  - owner_name: evgnomon
    repo_name: blueprint
    secret_name: VAULT_FILE
    secret_file: ~/src/github.com/<user>/config/secrets/evgnomon_blueprint_github.yaml
```
