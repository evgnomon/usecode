# uc secret, uc secret cert and uc profile

## uc secret

Secret stores are ansible-vault files. A store has a name; with `-r`/`--repo`
it is the current repository's store, `<org>_<repo>[_NAME]` (where `uc vm`
and others find tokens like `hetzner.prod`).

```bash
uc secret gen                            # a random 32-char password
uc secret gen -l 20 --no-symbols
uc secret get -r                         # the repository's secrets as JSON
uc secret get -r -f hetzner.prod         # one field, like jq -r .hetzner.prod
uc secret get mystore -f api.token
uc secret ensure -r                      # make the store if missing, print its file
uc secret edit -r                        # opens $EDITOR: leave it to the user
uc secret rotate NAME                    # re-encrypt under a new password
```

Never print a secret's value back to the user unless they asked for it; use
it in the command (`HCLOUD_TOKEN=$(uc secret get -r -f hetzner.prod) ...`).

Files:

```bash
uc secret encrypt notes.txt              # -> notes.txt.asc, removes notes.txt once checked
uc secret decrypt notes.txt.asc          # -> notes.txt, keeps the .asc
uc secret decrypt -c notes.txt.asc       # to stdout
```

Both ask for a password on the terminal: give them to the user to run.

Backups of `~/.ssh`, `~/.gnupg` and `~/.kube`, with their modes:

```bash
uc secret backup [FILE]                  # <hostname>-secrets-<date>.tar.gz.asc, asks a password
uc secret backup --no-password --no-gpg out.tar.gz
uc secret restore FILE [--no-kube] [-f]  # keeps existing files unless -f
```

`uc secret server` / `uc secret serve`: the SSH-key authenticated secret
manager (`upsert`, `read`, `serve`); see its `-h`.

## uc secret cert

```bash
sudo -n uc secret cert trust registry.example.com:443   # trust the CA an endpoint serves
sudo -n uc secret cert trust --remove registry.example.com:443
uc secret cert trust --list [example.com]
```

`trust` installs into the system store, `/etc/containers/certs.d/<host:port>`
(podman) and `~/.pki/nssdb` (Chrome/Brave, restart it after). The endpoint has
to be live.

A local CA with server and client certificates, in `~/.x509/`:

```bash
uc secret cert init [--days 3650]               # the CA, once (--force recreates it: don't)
uc secret cert server web -d example.com -d www.example.com -i 192.0.2.10 [--p12]
uc secret cert client ada --email ada@example.com [--no-p12]
uc secret cert list [-v]
uc secret cert show web
uc secret cert verify web
uc secret cert nginx-config web --server-name example.com
uc secret cert delete web --force
```

`uc secret cert p12 KEYNAME` and `uc secret cert p12 fetch` bundle the zygote keys and
certificates as PKCS#12; `fetch` ssh-es to `controller.zygote`.

## uc profile

Switches this machine's profile: `~/.config/usecode/profiles/NAME.yaml` (or a
directory of that name), laid over `config.yaml`. Profiles nest
(`main/contract/client-1`).

```bash
uc profile                               # the profiles, the active one marked
uc profile laptop-wsl                    # switch, creating it if needed
uc profile main/contract/client-1 -y     # nested; -y: don't ask before making a GPG key
uc profile --off                         # back to the shared stores
```

A new profile may generate a GPG key and secret stores of its own: tell the
user before running it.
