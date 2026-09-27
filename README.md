<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# usecode

Hi! This is the toolbox I use every day to set up Linux machines, spin up
servers and run AI agents. I got tired of doing the same setup over and over,
so I put all of it in one place and gave it one command: `uc`. If you work on
Debian or Ubuntu, there's a good chance some of it saves you a weekend too.

## What you get

- **A ready-to-code machine in one go.** `uc configure` installs and configures
  compilers, language servers, CLI tools, editors and dotfiles. It knows whether
  it's on a desktop, a VM, WSL or a container and only installs what makes sense
  there.
- **One command for the everyday DevOps chores.** `uc` gathers dozens of small
  tools under friendly groups: `uc vm`, `uc cert`, `uc db`, `uc image`,
  `uc repo`, `uc secret` and more. Run `uc help` to see what's there.
- **Cloud servers without the console clicking.** `uc vm create` spins up a
  machine locally (KVM/QEMU) or on Hetzner, DigitalOcean, OVHcloud or UpCloud,
  picks the cheapest server that fits the size you asked for, and adds an ssh
  entry so `ssh <name>` just works.
- **Secrets that stay secret.** `uc encrypt` / `uc decrypt` for files, and
  `uc secret` for generating secrets and managing vault stores.
- **An AI agent stack you can run yourself.** A small web app and API for
  chatting with AI agents, plus an MCP server (`usecode-mcp`) so Claude Code or
  any MCP client can drive it. See [docs/usecode-agent.md](docs/usecode-agent.md).

## Quick start

On a Debian or Ubuntu machine (a VM or a spare box is perfect if you just want
to look around), run:

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/evgnomon/usecode/refs/heads/master/play.sh)
```

Grab a coffee, the first run takes a while. When it's done, open a new shell and
say hello:

```bash
uc help
```

On Windows or macOS? No need to switch: a Debian/Ubuntu VM (or WSL on Windows)
works great, locally or in the cloud.

### What the kickstart does

[`play.sh`](play.sh) is short and worth a read before you run it. In order, it:

1. updates apt and installs `git`, `make` and `curl`,
2. clones this repo to `~/src/github.com/evgnomon/usecode` (or updates it),
3. installs the base system packages (`lib/configurator`, needs `sudo`),
4. installs Rust, builds `uc` and runs `uc configure` for your machine,
5. builds everything in `lib/` and installs the tools to `/usr/local/bin`.

It's safe to run again; finished steps are skipped and your checkout is just
fast-forwarded. A few environment variables let you steer it:

| Variable | What it does | Default |
| --- | --- | --- |
| `USECODE_DIR` | Where the checkout lives | `~/src/github.com/evgnomon/usecode` |
| `USECODE_BRANCH` | Which branch to build | `master` |
| `INSTALL_PROFILE` | `workstation`, `dev_container`, `wsl` or `vm` | detected |

For example, to set up a VM without the desktop hardware bits:

```bash
INSTALL_PROFILE=vm bash <(curl -fsSL https://raw.githubusercontent.com/evgnomon/usecode/refs/heads/master/play.sh)
```

Want to see what `uc configure` would change before it does? After the first
run, `lib/configurator/play.sh -C` does a dry run. More on profiles in
[lib/configurator/README.md](lib/configurator/README.md).

### Already have a checkout?

```bash
cd ~/src/github.com/evgnomon/usecode
make submodules
make
sudo make install
```

## Try it in a container first

If you'd rather not touch your machine yet, a container is a nice way to kick
the tires. With Podman or Docker installed:

```bash
bash scripts/containerize.sh build   # lightweight dev image
bash scripts/containerize.sh run
```

To build an image that runs the full kickstart inside it:

```bash
bash scripts/containerize.sh build Dockerfile
```

Podman is used when both are installed; set `CONTAINER_RUNTIME=docker` to pick
Docker, or `USECODE_IMAGE` for a different tag.

### VS Code Dev Container

Open the repo in VS Code and pick **Dev Containers: Reopen in Container**. The
default container is kept light: dev and CLI tools, no fonts, GUI apps or
hardware-token packages. If you need Podman inside it, use
`.devcontainer/devcontainer.nested.json`; it runs privileged, so keep it for
workspaces you trust. After changing the container config, run
**Dev Containers: Rebuild Container Without Cache**.

## Run the agent stack locally

The web app, API and friends run from `deploy/compose.yml` with Podman:

```bash
make up       # start, building images only the first time
make reload   # rebuild and restart after code changes
make logs     # follow the logs
make down     # stop (your data is kept)
```

Then open <http://localhost:8430>. To let Claude Code talk to it:

```bash
claude mcp add usecode -- usecode-mcp
```

## Finding your way around

| Path | What's in it |
| --- | --- |
| `lib/uc` | The `uc` command and `uc configure`; start with its [README](lib/uc/README.md) |
| `lib/` | Every tool and library, each with its own Makefile |
| `lib/configurator` | The bootstrap that the kickstart calls |
| `deploy/` | Compose file, container images and Ansible playbooks |
| `docs/` | Longer write-ups, like the agent architecture |
| `scripts/` | Small build helpers used by the Makefile |

Handy `make` targets at the root: `build`, `install`, `link`, `clean`,
`submodules`, `fmt`, `headers-check` and `authors`.

## Contributing

Ideas, bug reports and patches are very welcome. Please have a look at
[CONTRIBUTING.md](CONTRIBUTING.md) (commits need a `Signed-off-by` line) and the
[Code of Conduct](CODE_OF_CONDUCT.md). usecode is licensed under the
[HGL General License](COPYING).

## No Warranty

The following disclaimers must keep being prominently displayed in the documentation and any other materials for the Covered Work or a Derivative Work:

EXCEPT WHEN OTHERWISE STATED IN WRITING, THE COPYRIGHT HOLDERS AND/OR OTHER PARTIES PROVIDE THE COVERED WORK “AS IS” WITHOUT IMPLIED WARRANTIES OF FITNESS FOR A PARTICULAR PURPOSE, NON-INFRINGEMENT, MERCHANTABILITY AND TITLE, AND ANY OTHER KIND OF EXPRESSED OR IMPLIED WARRANTIES.

IN NO EVENT SHALL ANY COPYRIGHT HOLDER, OR ANY OTHER PARTY WHO MAY MODIFY AND/OR REDISTRIBUTE THE PROGRAM AS PERMITTED ABOVE BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, LOSS OF GOODWILL, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THE COVERED WORK OR AS A RESULT OF OUR LICENSE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

Nevertheless, you retain the option to extend offers of support, warranties, indemnities, or other liability obligations and/or rights in alignment with this License. Such offers may be provided in exchange for a fee, at your discretion. This provision allows you to engage in commercial transactions by offering additional services or assurances while remaining compliant with the terms of this License.
