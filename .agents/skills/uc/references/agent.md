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
uc agent session                         # pick a saved session (date, dir, first line) and carry on in it
uc agent session 2 | NAME                # the 2nd newest, or by name; no terminal: lists them
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

A model more than one provider serves comes from the provider named after
it (`uc agent model opus copilot`), else the one `model_providers:` in
agent.yaml pins it to (`{claude-opus-5.5: copilot}`), else the first of
`provider_order:` (`[claude, copilot, openrouter]`, or
`UC_PROVIDER_ORDER`/`--provider-order` for one run) that has it, else the
session's provider. The subagent tiers (`fast`, `balanced`, `strong`)
follow the same order. `uc agent model --sub` shows what each kind runs on.
`model_aliases:` in agent.yaml says what a name means, followed through
other aliases (`{opus: claude-opus, claude-opus: claude-opus-5.5}`),
wherever a model is named, including the keys of `model_providers:`.
A name without a version, after aliases, is the newest of its family any
provider serves (`opus`, `claude-opus` → the newest `claude-opus-X.Y`). The
model lists are cached a day in `~/.cache/usecode/models.json`;
`uc agent model-update` fetches them now.

Each kind of subagent can have its own model: `uc agent model --sub
edit=opus explore=haiku` saves them as `subagent_models:` in agent.yaml
(`edit=default` takes one out). A kind's own wins over `subagent_model:`
and `UC_SUBAGENT_MODEL`; `UC_SUBAGENT_MODEL_<KIND>` wins over it.
