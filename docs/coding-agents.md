<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# Use usecode from your coding agent

Your coding agent is already good at writing code. Give it usecode and it can
also spin up servers, run the AI model container and look after its own
workspace, all from the same chat. You just ask in plain words ("make me a
small Ubuntu box", "start the model") and it picks the right tools.

It works through `uc-agent-mcp`, a small MCP server that talks to
[usecode.dev](https://usecode.dev) out of the box. Set it up once and every
session gets the usecode tools.

## What you need

1. **An API key.** Sign in at [usecode.dev](https://usecode.dev) and grab one.
2. **The `uc-agent-mcp` binary on your `PATH`.** From a checkout of this repo:

   ```bash
   cd lib/bot && make install    # installs /usr/local/bin/uc-agent-mcp
   ```

   If you ran the [kickstart](../README.txt), it's already there.

Then pick your agent below. In every example, swap `<your API key>` for the key
you got from usecode.dev.

## Claude Code

One line:

```bash
claude mcp add usecode -e USECODE_MCP_API_KEY=<your API key> -- uc-agent-mcp
```

Check it with `claude mcp list`, or run `/mcp` inside a session. To remove it:
`claude mcp remove usecode`.

## Claude Desktop

Open **Settings → Developer → Edit Config** and add usecode to
`claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "usecode": {
      "command": "uc-agent-mcp",
      "env": {
        "USECODE_MCP_API_KEY": "<your API key>"
      }
    }
  }
}
```

Restart Claude Desktop and the usecode tools show up in the tools menu.

## Cursor

Add this to `~/.cursor/mcp.json` (for every project) or `.cursor/mcp.json` (for
just one):

```json
{
  "mcpServers": {
    "usecode": {
      "command": "uc-agent-mcp",
      "env": {
        "USECODE_MCP_API_KEY": "<your API key>"
      }
    }
  }
}
```

It shows up under **Settings → MCP**, where you can switch it on and off.

## VS Code (GitHub Copilot)

Copilot Chat's agent mode reads MCP servers from `.vscode/mcp.json` in your
workspace:

```json
{
  "servers": {
    "usecode": {
      "type": "stdio",
      "command": "uc-agent-mcp",
      "env": {
        "USECODE_MCP_API_KEY": "<your API key>"
      }
    }
  }
}
```

Click the **Start** codelens above the entry, or run **MCP: List Servers** from
the Command Palette. Since this file often gets committed, you may prefer to
keep the key out of it and set `USECODE_MCP_API_KEY` in your shell instead.

## GitHub Copilot CLI

```bash
copilot mcp add usecode --env USECODE_MCP_API_KEY=<your API key> -- uc-agent-mcp
```

Check it with `copilot mcp list`. To remove it: `copilot mcp remove usecode`.

## OpenAI Codex CLI

```bash
codex mcp add usecode --env USECODE_MCP_API_KEY=<your API key> -- uc-agent-mcp
```

Or add it to `~/.codex/config.toml` by hand:

```toml
[mcp_servers.usecode]
command = "uc-agent-mcp"
env = { USECODE_MCP_API_KEY = "<your API key>" }
```

## Gemini CLI

Add usecode to `~/.gemini/settings.json`:

```json
{
  "mcpServers": {
    "usecode": {
      "command": "uc-agent-mcp",
      "env": {
        "USECODE_MCP_API_KEY": "<your API key>"
      }
    }
  }
}
```

Run `/mcp` in a session to see it.

## Zed

Add this to your Zed `settings.json`:

```json
{
  "context_servers": {
    "usecode": {
      "command": "uc-agent-mcp",
      "args": [],
      "env": {
        "USECODE_MCP_API_KEY": "<your API key>"
      }
    }
  }
}
```

## opencode

Add usecode to `~/.config/opencode/opencode.json` (for every project) or
`opencode.json` at the root of a project:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "usecode": {
      "type": "local",
      "command": ["uc-agent-mcp"],
      "enabled": true,
      "environment": {
        "USECODE_MCP_API_KEY": "<your API key>"
      }
    }
  }
}
```

It pairs nicely with a local model: the agent can start usecode's model
container and then use it.

## Crush

Add usecode to `~/.config/crush/crush.json` (for every project) or
`crush.json` at the root of a project:

```json
{
  "$schema": "https://charm.land/crush.json",
  "mcp": {
    "usecode": {
      "type": "stdio",
      "command": "uc-agent-mcp",
      "env": {
        "USECODE_MCP_API_KEY": "<your API key>"
      }
    }
  }
}
```

## Windsurf

Add usecode to `~/.codeium/windsurf/mcp_config.json`:

```json
{
  "mcpServers": {
    "usecode": {
      "command": "uc-agent-mcp",
      "env": {
        "USECODE_MCP_API_KEY": "<your API key>"
      }
    }
  }
}
```

Refresh the MCP servers list in Cascade and the tools show up.

## Cline and Roo Code

Both are VS Code extensions with their own MCP settings. In Cline, open the
**MCP Servers** panel and click **Configure MCP Servers**. In Roo Code, open
**MCP Servers** and edit the global settings, or add `.roo/mcp.json` to a
project. Then add:

```json
{
  "mcpServers": {
    "usecode": {
      "command": "uc-agent-mcp",
      "env": {
        "USECODE_MCP_API_KEY": "<your API key>"
      }
    }
  }
}
```

## Grok Build

Add usecode to `~/.grok/config.toml` (for every project) or
`.grok/config.toml` in a project:

```toml
[mcp_servers.usecode]
command = "uc-agent-mcp"
env = { USECODE_MCP_API_KEY = "<your API key>" }
```

Run `grok inspect` to check that it was picked up.

## Muse Code

Add usecode to `~/.config/muse/settings.json`:

```json
{
  "mcp_servers": {
    "usecode": {
      "transport": "stdio",
      "command": "uc-agent-mcp",
      "env": {
        "USECODE_MCP_API_KEY": "<your API key>"
      }
    }
  }
}
```

Muse Code stops a run if a server it needs doesn't start. If you'd rather it
carry on without usecode, add `"mode": "optional"`.

## Hermes Agent

Add usecode to `~/.hermes/config.yaml`:

```yaml
mcp_servers:
  usecode:
    command: "uc-agent-mcp"
    env:
      USECODE_MCP_API_KEY: "<your API key>"
```

Check it with `hermes mcp test usecode`, or run `/reload-mcp` in a session
after editing.

## Oh My Pi

Plain pi leaves out MCP on purpose, but its fork Oh My Pi (`omp`) supports it.
Add usecode to `~/.omp/agent/mcp.json` (for every project) or `.omp/mcp.json`
in a project:

```json
{
  "mcpServers": {
    "usecode": {
      "command": "uc-agent-mcp",
      "env": {
        "USECODE_MCP_API_KEY": "<your API key>"
      }
    }
  }
}
```

Or run `/mcp add` in a session and follow the prompts. Check it with
`/mcp test usecode`.

## OpenClaw

One line:

```bash
openclaw mcp add usecode --command uc-agent-mcp --env USECODE_MCP_API_KEY=<your API key>
```

This saves it under `mcp.servers` in your OpenClaw config.

## Ori

Ori (from OpenRouter) doesn't run MCP servers itself. It launches another agent,
such as Claude Code, Codex or Cline. Set up usecode in that agent as shown
above and it's there whenever Ori starts it.

## Any other MCP client

`uc-agent-mcp` is a plain stdio MCP server, so anything that speaks MCP can run
it. Most clients take something close to this:

```json
{
  "mcpServers": {
    "usecode": {
      "command": "uc-agent-mcp",
      "env": {
        "USECODE_MCP_API_KEY": "<your API key>"
      }
    }
  }
}
```

## Give it a spin

Once it's connected, try asking your agent:

- "Check that usecode is healthy."
- "What server types can I create?"
- "Create a small Ubuntu server called `sandbox`, then tell me when it's up."
- "Start the AI model container."

## A few handy tips

- **Running your own stack?** Point the agent at it instead of usecode.dev by
  adding `USECODE_MCP_API_BASE_URLS=http://localhost:8430/api,http://localhost:8431/api`
  next to the API key.
- **"Command not found"?** Some desktop apps don't see your shell's `PATH`. Use
  the full path, `/usr/local/bin/uc-agent-mcp`, as the command.
- **Getting 401 errors?** Your key has probably expired or been revoked. Grab a
  fresh one from usecode.dev and update the config.
- **Want every setting?** The full list of `USECODE_MCP_*` options and tools is
  in [lib/bot/README.md](../lib/bot/README.md).
