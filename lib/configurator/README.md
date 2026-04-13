# HGL/Blueprint

Print your workstation wherever you are. The same thing, the same way, every time.

## Install

### Linux (bare metal)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/evgnomon/blueprint/refs/heads/main/install.sh | sh
```

Provide user configs:

```bash
git clone ssh://git@github.com:YOURUSER/.blueprint.git ~/.config/blueprint
```

### Windows / macOS (Dev Container)

On Windows or macOS, run Blueprint inside a [Dev Container](https://containers.dev/). Open the repo in VS Code or any Dev Containers-compatible editor and reopen in container — the `.devcontainer/devcontainer.json` is already configured. Then run the install script inside the container:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/evgnomon/blueprint/refs/heads/main/install.sh | DEV_CONTAINER=1 sh
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

Run `rotsec` in your repo. Set `repo_secrets` using `rchain`:

```yaml
repo_secrets:
  - owner_name: evgnomon
    repo_name: blueprint
    secret_name: VAULT_PASS
    secret_value: yourpass
  - owner_name: evgnomon
    repo_name: blueprint
    secret_name: VAULT_FILE
    secret_file: ~/.config/blueprint/secrets/evgnomon_blueprint_github.yaml
```

## License

HGL, verified:

```bash
shasum -a 512 -c SHA512SUMS
```

