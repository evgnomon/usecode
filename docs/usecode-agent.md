<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# usecode agent

Messenger for chatting with AI agents.

- `lib/api` — Rust (axum) backend with server-rendered Jinja2 + HTMX UI and OTP auth API.
- `lib/app` — SCSS source assets compiled into API static CSS.
- `lib/bot` — MCP server (Rust) that operates usecode agent (wraps `lib/api` as tools for AI agents).

## Run locally

```sh
cd lib/api
USECODE_AGENT_NODE_NAME=api-1 cargo run   # http://localhost:8000
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
logout). It talks to the hosted API at `https://usecode.dev/api` by default; set
`USECODE_MCP_API_BASE_URLS` to point it at your own `lib/api` instead.

```sh
cd lib/bot && make install && uc-agent-mcp   # starts an MCP server over stdio
```

Add it to Claude Code:

```sh
claude mcp add usecode -- uc-agent-mcp
```

See `lib/bot/README.md` for configuration and the full tool list.

## Container registry

`uc image push` and `uc image pull` (from `lib/uc`) push/pull container images to the registry. The registry host is only
reachable through the `shadow` bastion, so both commands open a local SSH tunnel before logging in.

```sh
# push a local image
uc image push myimage:latest

# pull an image, optionally stripping the registry host from the resulting tag
uc image pull myimage:latest
uc image pull --strip-host myimage:latest
```

Both commands read the registry password from `deploy/vault.yaml` of the current git
checkout (decrypted by uc itself, no ansible needed; the vault password is found the way
`ansible-vault` would find it) unless `REGISTRY_PASSWORD` is set (or `--vault-file` points
elsewhere), and require an SSH host entry (or DNS-resolvable name) for the bastion. Run `uc image push -h` for the
other settings; each has a matching environment variable.
