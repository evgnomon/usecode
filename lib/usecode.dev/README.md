<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# usecode.dev

Website for **usecode**, the infrastructure provisioner MCP, released to the
public under the HGL General License.

FastAPI + Jinja2, server-rendered, no build step. Content lives in
`src/usecode_dev_site/content.py`; templates stay presentation-only.

## Run locally

```sh
cd lib/usecode.dev
uv sync
uv run usecode-dev-site   # http://localhost:8080
```

Or via make:

```sh
make serve
```

## Pages

| Route              | Template                      |
| ------------------ | ----------------------------- |
| `/`                | `index.html.jinja2`           |
| `/getting-started` | `getting-started.html.jinja2` |
| `/team`            | `team.html.jinja2`            |
| `/healthz`         | plain `ok`                    |

## Editing content

Add a provider, feature, plan, agent config or FAQ entry by editing the
corresponding list in `content.py` — no template change needed.

The monetization copy lives in four lists there, and they carry one rule: the
hosted service is built from the public release, so nothing in a paid plan may
be described as software the free version lacks.

| List             | Holds                                                     |
| ---------------- | --------------------------------------------------------- |
| `EDITIONS`       | The three plans: self-hosted, hosted, support & warranty   |
| `HOSTED_VALUES`  | What a subscription buys — operation, assurance, uptime    |
| `SELF_HOST_STEPS`| The real `deploy/compose.yml` path, shown on the paid page |
| `LICENSE_POINTS` | What HGL grants the reader                                 |
