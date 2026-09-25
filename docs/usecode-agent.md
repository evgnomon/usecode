<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# usecode agent

Messenger for chatting with AI agents.

- `lib/api` — FastAPI backend with server-rendered Jinja2 + HTMX UI and OTP auth API.
- `lib/app` — SCSS source assets compiled into API static CSS.
- `lib/bot` — MCP server (Rust) that operates usecode agent (wraps `lib/api` as tools for AI agents).

## Run locally

```sh
cd lib/api
uv sync
uv run usecode-agent-api   # http://localhost:8000
```

### Rebuild UI styles (SCSS)

```sh
cd lib/app
npm install
npm run build:css
```

See `lib/app/README.md`, `lib/api/README.md`, and `lib/bot/README.md` for details.

## MCP bot

`lib/bot` is an MCP server that lets an AI agent operate usecode agent (OTP login, session lookup,
logout) against a running `lib/api` instance.

```sh
cd lib/bot && make install && usecode-mcp   # starts an MCP server over stdio
```

Add it to Claude Code:

```sh
claude mcp add usecode -- usecode-mcp
```

See `lib/bot/README.md` for configuration and the full tool list.

## Container registry

`uc push` and `uc pull` (from `lib/uc`) push/pull container images to the registry deployed by
`deploy/playbooks/registry.yaml` (`lib/roles/container-registry`). The registry host is only
reachable through the `shadow` bastion, so both commands open a local SSH tunnel before logging in.

```sh
# push a local image
uc push myimage:latest

# pull an image, optionally stripping the registry host from the resulting tag
uc pull myimage:latest
uc pull --strip-host myimage:latest
```

Both commands read the registry password from `deploy/playbooks/vault.yaml` of the current git
checkout via `ansible-vault` unless `REGISTRY_PASSWORD` is set (or `--vault-file` points
elsewhere), and require an SSH host entry (or DNS-resolvable name) for the bastion, matching
`deploy/playbooks/group_vars/main.yml`'s `firewall_bastion_hosts`. Run `uc push -h` for the
other settings; each has a matching environment variable.
