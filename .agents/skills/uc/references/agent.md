# uc agent

`uc` with no command is the coding agent. `uc agent` is its settings and
servers, all kept in `~/.config/usecode/agent.yaml`.

Inside a session, use the `chat` and `skill` tools rather than these
commands: they act on the running session. The commands below change what
new sessions start with.

```bash
uc agent model                           # the models of every provider with a key
uc agent model opus                      # start new sessions on it (number, part of an id, or id)
uc agent model-update                    # fetch the providers' model lists again
uc agent effort high                     # minimal, low, medium, high, xhigh or max
uc agent skill                           # the skills, the ones on marked
uc agent skill uc -agent-browser         # turn on, turn off; none for all off
uc agent usage                           # OpenRouter credits, Claude subscription windows
uc agent login claude | copilot          # interactive: give it to the user
uc agent import [FILE]                   # carry on from an exported conversation
uc agent serve [--listen ADDR]           # uc in a web page on 127.0.0.1:4777 (keeps running)
uc agent api                             # the HTTP backend (keeps running); migrate [--status]
uc agent mcp                             # the MCP server, on stdin/stdout
```

`compact`, `status`, `export`, `rewind`, `exit` only mean something typed at
a uc prompt.

`serve`, `api` and `mcp` don't end: `background: true`.

Flags of uc itself: `--ask`, `--output`, `--commands`, `--no-shell`,
`--long-cache`, `--parallel`/`--exa`/`--duckduckgo`; each also `UC_<FLAG>`
or under `uc:` in `config.yaml`. `provider:`, `model:` and `effort:` in
agent.yaml (or `UC_PROVIDER`, `UC_MODEL`, `UC_EFFORT`) pick the start.
