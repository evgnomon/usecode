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
uc                     # talk to the coding agent
uc help                # list the uc-* commands found on PATH
uc secret gen          # runs uc-secret gen
```

This crate ships the dispatcher, the two file encryption subcommands, which
replace the former `boom` shell script, and `uc sync`, the machine
configurator, `uc image push`/`uc image pull`, which move container images through
the registry behind the bastion, and `uc secret`, which generates secrets,
manages the ansible-vault secret stores and, with `uc secret cert`, your
certificates and trust.
The last two replace the former `lib/pylib` Python package.

It also ships the command groups — `uc image`, `uc secret cert`,
`uc deb`, `uc db`, `uc net`, `uc daemon`, `uc vm`, `uc new`
and `uc agent` — which gather
every tool in `lib/` under one command; see [Command groups](#command-groups).

## Talking to uc

Run `uc` on its own and you get a prompt: it's a small coding agent. Tell it
what you want, and it looks around, runs commands and edits files in the
current directory for you, straight away. If you'd rather okay each command
or change first, start it with `uc --ask`; answer `a` there and it stops
asking for the rest of the session. Deleting files always asks, whatever
mode you're in. Slow builds and servers don't freeze it either: a command
still going after its timeout keeps running as a job the agent checks on
and stops by itself. It runs DeepSeek V4.1 Flash with high reasoning effort
unless you pick something else. You see the commands it runs and its
answers, not the commands' raw output; `uc --output` shows that too.
Each command shows up as a few words on what it does, like `· list the rust
files`, so the screen reads like a story instead of a wall of flags; anything
it asks you to okay still shows in full, and `uc --commands` shows every
command line as it is.
Type a command like `ls -al` or `git status` and it just runs on your
terminal, like in a shell (`cd` and your bash aliases and functions too), with
the same confirmations; a line that reads like a sentence goes to the model,
and `!` in front makes anything a command. Rather talk to the model every
time? `uc --no-shell` (or `UC_NO_SHELL=1`) sends every line to it.
Up and Down bring back what you typed before, in uc or in bash, even in an earlier session,
and Ctrl-R finds it in a colourful fuzzy picker, just like fzf in bash. Changed your mind while it
works? Esc or Ctrl-C stops it, and whatever it was running, and you're back at
the prompt; Ctrl-C at the prompt leaves uc.
No OpenRouter key yet? No problem: `uc` starts anyway with the AI off, and
every line you type just runs in bash. Set the key whenever you like and
the model is there next time.

The providers and their keys live in `~/.config/usecode/agent.yaml`, not in
the environment:

```yaml
# ~/.config/usecode/agent.yaml
providers:
  openrouter:
    api_key: sk-or-...                        # https://openrouter.ai/keys
  groq:                                       # any OpenAI-compatible API
    base_url: https://api.groq.com/openai/v1
    api_key: gsk_...
    models: [llama-3.3-70b-versatile]         # offered first; optional
  local:
    base_url: http://localhost:11434/v1       # no key needed
```

Each entry is one provider, named by its key. Anything that speaks OpenAI's
chat completions is one entry away: give it a `base_url` (and an `api_key`
unless it takes none), and `uc` sends to `<base_url>/chat/completions` and
lists `<base_url>/models`. `kind` says how to talk to it when the name
doesn't: `openai` (the default), `openrouter`, `claude` or `copilot`, so a
second OpenRouter key is `work: {kind: openrouter, api_key: ...}`. The rest
is optional: `models` puts some first, `default_model` is the one to start
on, `list_models: false` offers only `models`, and `headers` adds HTTP
headers. The file holds keys: keep it out of a public repository.

```sh
uc                                            # Ctrl-D or uc agent exit to leave
uc --ask                                      # confirm each command and change
uc --output                                   # also show what the commands print
uc --commands                                 # show command lines, not their titles
uc --no-shell                                 # every line goes to the model
UC_MODEL=openai/gpt-5 uc                      # another model, just this once
```

Which provider and model a session starts on, and how hard it thinks, sit
beside `providers:` in the same file; `UC_PROVIDER`, `UC_MODEL` and
`UC_EFFORT` override them for one run:

```yaml
provider: openrouter                          # else the first with a key
model: z-ai/glm-5.3-flash                     # uc agent model sets these two
effort: medium                                # low, medium or high reasoning
```

Have a Claude Pro or Max subscription instead? `uc agent login claude` opens
Claude's login, takes the code the page shows, and saves a year-long,
inference-only token as the `claude` provider's `api_key` in `agent.yaml`
(`--print` shows the token instead, `--name work` saves it under another
name). Got GitHub Copilot? `uc agent login copilot` shows a short code to
type on github.com and saves the token as the `copilot` provider's; `uc`
then lists the chat models your Copilot plan offers.

Set as many of these as you like: `uc agent model` shows the models of every provider
you have a key for, all in one list, and you can switch between them in the
middle of a conversation. When a model is on more than one, say Opus on both
OpenRouter and your Claude subscription, each gets its own line with the
provider beside it, and `uc agent model opus claude` (or the number) picks the one you
want. `uc` starts on the first provider in the file that has a key;
`provider: claude` (or any provider's name) picks another. The
cost line is OpenRouter's. `uc agent usage` shows how much of
the subscription's 5-hour and weekly windows you have used, next to the
OpenRouter spend when both keys are set.

One command everywhere: `uc agent model`. Typed at a session's prompt it
switches that session — `uc agent model` picks one in the picker, `uc agent
model opus claude` goes straight to it — and saves the choice as `provider:`
and `model:` in `agent.yaml`, so the sessions you start after
follow it. The other `uc` processes running on the machine keep their model
until you say so in them. Typed in a shell instead, with no model named it
lists the models, marking the one a new `uc` starts on, and naming one saves
it.

Every flag can be set three ways, so you can make your favourite the
default: on the command line, as a `UC_` environment variable, or under
`uc:` in your `config.yaml`. The command line wins, then the environment,
then the config, and `0`, `false`, `no` or `off` turns a flag back off:

```sh
uc --no-shell                                 # just this once
export UC_NO_SHELL=1                          # this shell and what it starts
```

```yaml
# ~/.config/usecode/config.yaml
uc:
  no_shell: true                              # every time
```

It runs on GLM 5.3 Flash, which is quick, cheap and good at everyday
work, and stays on one model for the whole session: each line is a single
request, so replies start fast. Want another? Type `uc agent model` and pick one in
the picker (Esc keeps what you have), or go straight to one with `uc agent model opus`. A
name that fits several, like `uc agent model claude`, opens the picker filtered by it. The
new model gets the whole conversation and carries on from there, even on
another provider. `model:` in `agent.yaml` sets the one to start on.

The lists of models are kept in `~/.cache/usecode/models.json`, so starting
`uc` does not wait on the providers. A new model not showing up? `uc agent model-update`
fetches the lists again.

The model can do the same itself with the `chat` tool: `chat model
<name>` switches, `chat model` lists, and `chat clear` clears the screen and
the context; `chat exit` leaves uc. It only does this when you ask.

Wondering how much of the model's context the conversation has taken? Type
`uc agent context` and it says how many messages there are, about how many tokens
they make (an estimate until the model has counted them), what the last
turn took in tokens — how many went in, how many were cached, how many
came out — how full the model's context window is, and what the session
has spent so far, at the model's price per million tokens.
`uc agent compact` cuts old tool output and sums up the earlier
conversation, and `uc agent exit` (or `uc agent quit`) leaves uc.

Rather talk than type? Hold Space at an empty prompt and say it: it
listens while you hold, and when you let go what you said is written out and
sent, just as if you had typed it. A quick tap still types a space, and any
other key while it listens cancels. It records from PulseAudio or PipeWire,
whichever you run, and OpenRouter turns your voice into text with the same key.
It transcribes with MAI-Transcribe 2, the best of the engines at a short
command and a tenth of GPT-4o Transcribe's price, and sends the names of your
repositories and commands along as words to expect, so `kubectl`, `fzf` and
`usecode` come out right; word by word rather than as a block, so a word the
engine is unsure of, or a recording that was only a knock, is dropped instead
of sent:

```yaml
# ~/.config/usecode/agent.yaml
voice:
  model: openai/gpt-transcribe                # MAI-Transcribe 2 unless you pick another
  language: de                                # the language your locale is in, if unset
  terms: [idempotent, nexus, syllabub]        # words to expect, on top of the ones it finds
  # terms: false                              # send none of them
```

It can look things up on the web too, so it isn't stuck with what the model
knew when it was trained. It searches DuckDuckGo, no key needed, or
Parallel or Exa if you have a key, which tend to find better pages for a
model to read (Parallel first when both are there; `uc --exa` or
`uc --duckduckgo` picks one); and it reads pages; with a Cloudflare account it reads them
rendered in a real browser, so pages built by JavaScript come through as
clean markdown instead of an empty shell:

```yaml
# ~/.config/usecode/agent.yaml, all optional
search:
  parallel: {api_key: ...}                    # https://parallel.ai
  exa: {api_key: ...}                         # https://exa.ai
fetch:
  cloudflare:                                 # render pages first
    account_id: ...
    api_token: ...                            # with Browser Rendering: Edit
```

`uc -h` (or `uc help`) lists the commands.

## Encrypting files

```sh
uc secret encrypt secrets.txt        # -> secrets.txt.asc, secrets.txt removed
uc secret decrypt secrets.txt.asc    # -> secrets.txt, the .asc kept
uc secret decrypt -c secrets.txt.asc # print the plaintext instead of writing it
```

Both commands prompt for the password on the terminal and never echo it.

* `-f` overwrites an existing output file; without it an existing file is an
  error.
* `uc secret encrypt` decrypts its own output and compares it against the input before
  it removes the original, so a failed encryption never costs the plaintext.
* `uc secret decrypt` accepts only `.asc` files and leaves the encrypted copy in place.
* Output files are written through a temporary sibling and renamed, with mode
  `0600`, so an interrupted run cannot leave a half-written file behind.

### File format

The `.asc` file is exactly what `openssl enc -aes-256-cbc -pbkdf2 -salt -a`
produces — an 8-byte salt behind a `Salted__` magic, AES-256-CBC with PKCS#7
padding, base64 wrapped at 64 columns — so `openssl` remains a usable fallback:

```sh
openssl enc -d -aes-256-cbc -pbkdf2 -a -in secrets.txt.asc
```

`uc secret decrypt` also opens files written with the pre-OpenSSL-3.0 MD5 key
derivation, and says so when it does; re-encrypting upgrades them.

## Configuring the machine

`uc sync` configures the local machine — Apt sources and packages,
extrepo repositories, dotfiles, git checkouts, toolchains, desktop settings —
without Python or Ansible. Independent tasks run in parallel. All it needs
is uc itself: install the .deb or drop in the binary and run `uc sync`, no
checkout required.

Out of the box it only touches what uc itself needs: `~/.local/bin` and the
`~/.config/usecode` link. Everything else
is yours to switch on, by role, task id or glob, in the `sync` list of your
config (`all` turns on every role):

```yaml
sync: [apt, extrepo, dotfiles, git, mise, alacritty]  # alacritty brings its font
```

```sh
uc sync                    # what uc needs plus your sync list, for the detected profile
uc sync -C                 # dry run: report what would change
uc sync -t dotfiles,git    # only these roles, listed in sync or not
uc sync -t docker -d       # docker plus everything it runs after
uc sync -l -t 'git/*'      # list tasks, their tags and dependencies
uc sync --graph | dot -Tsvg > graph.svg
```

It keeps the architecture of the Ansible playbook it replaced:

* **modules** (`src/configure/modules`) are the reusable steps, after Ansible's
  modules: `copy`, `template`, `file`, `inflate`, `command`/`shell`, `apt`,
  `git`, `make`, `systemd`, `group`, `uri`. Each is idempotent, reports
  `ok` or `changed`, and honours `--check`.
* **roles** (`src/configure/roles`) are the configuration functions, one
  per Ansible role, adding tasks built from those modules to the plan.
* the files and templates live in `roles/<role>/{files,templates}` and are
  built into uc, then copied into place, so your home gets real files rather
  than links into a checkout. Templates render with Ansible's Jinja settings
  and see the same variables, including `ansible_facts`. Working on them?
  `--roles lib/uc/roles` (or `UC_ROLES`) reads them straight from a
  checkout, no rebuild needed.

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
| `--roles DIR`, `--config FILE` | read the role files from `DIR` instead of the built-in ones (also `UC_ROLES`, or `roles:` under `uc:`), and the user config |

Tasks that need root run through `sudo -n`; when the selection has any,
`uc sync` asks for the password once, before the run, and keeps the
credentials fresh until it ends.

## Moving container images

`uc image push` and `uc image pull` replace `deploy/push.sh` and `deploy/pull.sh`. The
registry is only reachable through the bastion, so both open an SSH tunnel to it, log the container CLI in, move
the images and close the tunnel again:

```sh
uc image push myimage:latest             # tag as localhost:5000/myimage:latest and push
uc image pull myimage:latest             # pull localhost:5000/myimage:latest
uc image pull --strip-host myimage:latest # ... and also tag it as myimage:latest
```

The password comes from `REGISTRY_PASSWORD`, or else from
`vault_container_registry_password` in `deploy/vault.yaml` of the
current git checkout (`--vault-file` to use another). uc decrypts it
itself, no ansible needed, finding the vault password just like
`ansible-vault` would (`ANSIBLE_VAULT_PASSWORD_FILE`, `ansible.cfg`, or a
prompt). The registry and bastion addresses, ports and users, and the
container CLI, are options with the same environment variables the scripts
read (`BASTION_HOST`, `REGISTRY_HOST`, `LOCAL_PORT`, `CONTAINER_CLI`, ...);
see `uc image push -h`.

## Secrets

```sh
uc secret gen                    # 32 characters: letters, digits and symbols
uc secret gen -l 16 --no-symbols
uc secret get                    # the default store as JSON
uc secret get -r -f github_pat   # one field of this repository's store
uc secret edit -r                # edit this repository's store in vi
uc secret ensure -r              # create it if missing, print its path
uc secret rotate NAME            # re-encrypt under a new vault password
uc profile laptop-wsl            # stores of this machine's own, see below
uc secret encrypt notes.txt      # see Encrypting files
uc secret server read acme prod db  # the SSH key authenticated secret server
uc secret serve                  # same as uc secret server serve
uc secret backup                 # your SSH, GPG and kube keys -> <host>-secrets-<date>.tar.gz.asc
uc secret restore FILE           # put them back in your home, same modes
```

`gen` draws characters uniformly from the OS random source; the dotfiles
alias `mkpass` runs it.

The other subcommands work on the ansible-vault secret stores in
`~/src/github.com/$USER/config/secrets`. A store `NAME` is `NAME.yaml`,
encrypted with ansible-vault, and `NAME.vault.asc`, its vault password
encrypted with gpg to the config's `gpg.key`; without a name it is
`secrets.yaml` and `vault.asc`.
uc reads and writes the stores itself, so ansible doesn't need to be
installed, and they still open with `ansible-vault` if you ever want to.
`-r` picks the current repository's store, `<org>_<repo>` from the working
directory, and a name given with it is appended (`-r github` is
`<org>_<repo>_github`). Vault passwords stay in memory, and stores are
always edited with `vi`, the hardened `lib/vi`, on a private temporary copy
kept in `/dev/shm` when there is one.

### Profiles

A profile is a set of config you switch on and off, like a machine's own
settings, a theme or a client's work, and profiles nest:

```sh
uc profile cochroack-wsl         # switch to it, creating it if needed
uc profile cochroack-wsl/dark    # dark, inside cochroack-wsl
uc profile main/contract/client-1
uc profile                       # list profiles, * marks the active one
uc profile --off                 # back to the main config
```

Profiles live in `~/.config/usecode/profiles/`, and the active one is
linked from `profile.yaml`, laid over `config.yaml` so it can override any
key. A profile is a file, `NAME.yaml`, or a directory, `NAME/`, shaped just
like the config root: its own `config.yaml`, a `profile.yaml` link over it
once something nests in it, and `profiles/`:

```
~/.config/usecode/
  .gitignore                    # profile.yaml, profiles/**/profile.yaml
  config.yaml
  profile.yaml -> profiles/cochroack-wsl
  profiles/
    cochroack-wsl/
      config.yaml               # cochroack-wsl's own settings
      profile.yaml -> profiles/dark.yaml
      profiles/
        dark.yaml
        light.yaml
    main.yaml
```

Switching creates whatever of the path is missing, turns a file profile
that gets a nested one into a directory (`main.yaml` becomes
`main/config.yaml`), and points each level's link at the next. Includes are
read from where a file really lives, so a profile can include files of its
own.

What gets checked in is up to you. The links say which profile this machine
uses, so ignore them:

```
profile.yaml
profiles/**/profile.yaml
```

#### A machine without your GPG key

The secret stores are encrypted to `gpg.key`. If, after a switch, this
machine doesn't have that key (say, a new laptop while your YubiKey is at
home), `uc profile` offers to generate one for the default git profile's
`fullname` and `useremail`, and gives the outermost new profile secret
stores of its own:

```yaml
# profiles/cochroack-wsl/config.yaml
profile: cochroack-wsl
gpg:
  key: 0123456789ABCDEF0123456789ABCDEF01234567
```

With that, every store lives in `secrets/cochroack-wsl/` and its passwords
are encrypted to the new key; `dark` and `light` share it. The shared
stores are left alone, and on a machine with the key nothing changes.

`get` converts the YAML to JSON itself (merge keys included), so `yj` is no
longer needed, and `-f a.b` prints one field the way `jq -r .a.b` would.
`rotate` moves the secret file to `NAME.yaml.bak` until the new password is
in place, so an interrupted rotation can simply be run again.

`encrypt` and `decrypt` run `uc-secret-encrypt` and `uc-secret-decrypt`; see
[Encrypting files](#encrypting-files). `server` runs
`uc-secret-server` (was `secd`), the SSH key authenticated secret server, with
its `upsert`, `read` and `serve` commands; `serve` on its own is a shortcut
for `server serve`.

`uc secret` replaces eight tools, and `make install` links their names to
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

### Backing up SSH, GPG and kube keys

Moving to a new machine, or about to wipe one? `uc secret backup` puts your
`~/.ssh`, `~/.gnupg` and `~/.kube` into one password encrypted
`.tar.gz.asc`, and `uc secret restore` puts them back into your home, with
the same modes:

```sh
uc secret backup                              # <hostname>-secrets-<YYYYMMDD>.tar.gz.asc
uc secret backup keys.tar.gz.asc --no-gpg --no-kube   # only the SSH keys
uc secret restore keys.tar.gz.asc             # asks for the password; existing files are kept
uc secret restore -f keys.tar.gz.asc          # ... or replaced
```

No root needed: it's always just you (under `sudo`, the user who ran it).
Got a different username or home on the new machine? No problem, the keys
land in your home there, owned by you, and absolute symlinks into the old
home are pointed at the new one.

Backups made by an older or a newer uc restore just fine: the archive
carries a small `uc-backup.json` saying what's in it, older uc's skip it,
and anything a newer uc added that this one doesn't know yet is put back
as it is. From an older backup that took every user, you get yours back
(the one with your name, or the only one). Agent sockets and lock files
are left out, and so are kubectl's caches. Your kubeconfigs are also all
`uc fs` needs: it keeps no keys of its own on disk and reads them from the
cluster each time (or from `UC_FS_*` in your environment). `--no-ssh`,
`--no-gpg` and `--no-kube` work on both sides, and `--root DIR`
backs up or restores a system mounted at `DIR`.

Backups are encrypted the same way as `uc secret encrypt` (AES-256 with
PBKDF2, OpenSSL compatible), so `uc secret decrypt keys.tar.gz.asc` gives you
the plain `.tar.gz` if you ever want to look inside. `--no-password` writes a
plain `.tar.gz` instead, and `restore` takes either.

## Command groups

Every other tool in `lib/` is its own executable, named after the command
that runs it: `uc db pg` runs `uc-db-pg`, `uc secret cert p12 fetch` runs
`uc-secret-cert-p12-fetch`. So you can call a tool through `uc` or straight by its
`uc-*` name, whichever reads better in a script. A group is a `uc-<group>`
executable that hands the rest of the command line to the tool, the way `uc`
hands it to the group; `uc <group>` lists its commands and flags the ones
whose tool is not installed. `uc help` lists just the groups and top level
commands, not every `uc-*` tool.

The old tool names still work for now: `make install` and `make link` put
them next to the new ones as symlinks. They go away in a later release, so
switching scripts over to the `uc` commands is a good idea.

| Command | Executable | Was |
|---|---|---|
| `uc image push`, `pull` | `uc-image-push`, `uc-image-pull` | |
| `uc image run barge` | `uc-image-run-barge` | `barge` |
| `uc secret cert init`, `server`, `client`, ... | `uc-secret-cert-gen`: any command not below | `certgen` |
| `uc secret cert trust HOST:PORT` | `uc-secret-cert-trust` | `trust_ca` |
| `uc secret cert p12 KEYNAME` | `uc-secret-cert-p12-bundle` | `mkp12` |
| `uc secret cert p12 fetch` | `uc-secret-cert-p12-fetch` | `zcdump` |
| `uc deb build`, `publish` | `uc-deb-build`, `uc-deb-publish` | `mkdeb`, `pubdeb` |
| `uc db pg`, `mongo`, `migrate` | `uc-db-pg`, `uc-db-mongo`, `uc-db-migrate` | `pg`, `mgo`, `sqlize` |
| `uc db resources sync`, `dump` | `uc-db-resources-yaml sync`, `dump` | `ysys` |
| `uc db resources configmap` | `uc-db-resources-configmap` | `confmap` |
| `uc db resources schema` | `uc-db-resources-schema` | `k8s_ddl` |
| `uc db resources pods` | `uc-db-resources-pods` | `mkpod` |
| `uc net mesh` | `uc-net-mesh` | |
| `uc net ipsec`, `dig` | `uc-net-ipsec`, `uc-net-dig` | `ipmesh`, `diga` |
| `uc daemon install`, `reload` | `uc-daemon-ctl install`, `reload` | |
| `uc vm` | `uc-vm-local`, arguments and all; with `--provider`, built in (see below) | `vm` |
| `uc new script`, `unit` | `uc-new-script`, `uc-new-unit` | `shole`, `mkunit` |
| `uc secret server` | `uc-secret-server` | `secd` |
| `uc agent api`, `mcp` | `uc-agent-api`, `uc-agent-mcp` | `usecode-agent-api`, `usecode-mcp` |
| `uc agent usage` | built in (OpenRouter and Claude usage) | |
| `uc terminal` | `uc-terminal-tm`, arguments and all | |

For `uc terminal`, the dotfiles give you the shell alias `t`.

### Cloud VMs

`uc vm` takes the same commands and size options to Hetzner Cloud,
DigitalOcean, OVHcloud Public Cloud or UpCloud when given `--provider`:

```sh
sudo uc vm create uc3 --memory 4GiB --vcpus 4 --disk-size 60G  # local KVM/QEMU
uc vm create uc3 --memory 4GiB --vcpus 4 --disk-size 60G --provider hetzner
uc vm list --provider digitalocean
uc vm create uc4 --vcpus 2 --memory 8GiB --provider ovh --location GRA11
uc vm create uc5 --vcpus 2 --memory 4GiB --provider upcloud --location de-fra1
uc vm remove uc3 --provider hetzner --force
```

`create` picks the cheapest server type, across the provider's locations, with
at least the requested vCPUs, memory and disk; `--location`, `--arch` and
`--image` narrow it down. On UpCloud, `create` defaults to the `dk-cph1`
(Copenhagen) zone and the smallest plan unless `--location`, `--vcpus`,
`--memory` or `--disk-size` say otherwise. The server boots with the local VMs' cloud-init
user-data from `/etc/vm/config.yaml`, and gets an ssh_config entry
(`/etc/ssh/ssh_config.d`, or `~/.ssh/config.d` without root), so `ssh uc3`
reaches the same user either way. `list`, `info`, `inspect`, `ip`, `start`,
`stop`, `restart` and `remove` work on cloud VMs too; `snapshot`, `fork`,
`mount` and `config` stay local. The providers are driven through their REST
APIs (over `curl`), with the token from `HCLOUD_TOKEN` or
`DIGITALOCEAN_ACCESS_TOKEN`, else `hetzner.prod` or `doctl.prod` in the
current repository's secrets. OVHcloud signs requests
with application keys instead: `OVH_APPLICATION_KEY`, `OVH_APPLICATION_SECRET`,
`OVH_CONSUMER_KEY`, the Public Cloud project id in `OVH_CLOUD_PROJECT_SERVICE`
and optionally `OVH_ENDPOINT` (`ovh-eu`, the default, `ovh-ca` or `ovh-us`),
else `application_key`, `application_secret`, `consumer_key`, `project` and
`endpoint` under `ovh.prod` in the secrets. Its flavors are priced from the
public catalog's hourly rate over 730 hours, and it has no graceful stop, so
`stop` and `stop --force` do the same. UpCloud takes an API token from
`UPCLOUD_TOKEN` or `upcloud.prod`, else its API user from `UPCLOUD_USERNAME`
and `UPCLOUD_PASSWORD` or `username` and `password` under `upcloud.prod`. Its
plans are priced per zone from the hourly price list, capped at 672 hours a
month; servers clone a template (`Debian GNU/Linux 13` by default, matched by
title prefix or UUID), log in with the SSH keys of the cloud-init user-data,
and are stopped before `remove --force` removes them with their disks. Cloud
VMs need no sudo.

A group's commands are tables in `src/groups.rs`; a command either runs a
tool, with arguments of its own put in front of the user's, opens a nested
group, or calls into this crate. A tool is found next to `uc` first, then on
`PATH`, and replaces the group's process, so its exit status, signals and
terminal are its own.

## Build

```sh
make build    # cargo build --profile fast (RELEASE=1 for release) --target $(uname -m)-unknown-linux-musl
make check    # fmt --check, clippy -D warnings, tests
make install  # install uc and its uc-* subcommands to /usr/local/bin
```

`make install` honours `DESTDIR` (with a trailing slash) and `PREFIX`, e.g.
`make install PREFIX=$HOME/.local`.

The default target is the musl one for your machine's arch
(`x86_64-unknown-linux-musl` or `aarch64-unknown-linux-musl`), added with
`rustup` on first use. The crate has no C dependencies, so the result is a static-pie executable
with no shared libraries at all — it runs on any Linux of the same
architecture, including a `scratch` container, and needs no `openssl` on the
host. `TARGET=` picks another triple, e.g. a dynamically linked glibc build:

```sh
make build TARGET=x86_64-unknown-linux-gnu
```

Got a Mac with Apple Silicon or a Windows on ARM laptop? uc runs there too,
inside a Lima VM or WSL2, which are aarch64 Linux. An x86_64 machine builds
that arm binary for you, linked with the `rust-lld` that comes with Rust, so
there's no cross compiler to install:

```sh
make build TARGET=aarch64-unknown-linux-musl   # target/aarch64-unknown-linux-musl/fast/
```
