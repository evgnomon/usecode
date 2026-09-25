<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# usecode-agent-api

Rust (axum + sqlx) backend for usecode agent: OTP login, API keys, cloud
provider credentials, servers and their background tasks, the AI model
container, and a server-rendered chat UI.

## Server-rendered app (HTMX + Jinja2 templates)

- `GET /` — serves the chat UI.
- `POST /web/auth/request-otp` — requests OTP from login form.
- `POST /web/auth/verify-otp` — verifies OTP and starts web session.
- `POST /web/auth/logout` — logs out web session.
- `GET /web/chat/{agent_id}` — loads a chat panel with HTMX.
- `POST /web/chat/{agent_id}/messages` — sends message and returns updated conversation.

Templates (`templates/`) and styles (`static/css/app.css`, built from
`lib/app`) are compiled into the binary.

## JSON auth API

- `POST /auth/otp/request` — `{ "phone": "+14155552671" }`
- `POST /auth/otp/verify` — `{ "phone": "...", "code": "..." }` → returns `{ "api_key": "..." }`
- `GET /auth/me` — send `X-API-Key: <api_key>` header
- `POST /auth/logout` — send `X-API-Key: <api_key>` header
- `POST /auth/api-keys`, `GET /auth/api-keys`, `DELETE /auth/api-keys/{id}` — manage the
  caller's API keys

Errors are `{"detail": "..."}`; a request that fails validation is a 422 whose
`detail` is a list of `{type, loc, msg}`.

## Providers, servers and tasks

- `PUT|GET|DELETE /providers/{provider}/credentials`, `GET /providers/credentials` —
  per-user provider credentials (`hetzner`, `digitalocean`), encrypted at rest.
- `GET /servers/types`, `POST /servers/sync`, `GET /servers/catalog` — the provider catalog,
  in our own type terminology (`{series}-{city}`, e.g. `x1-fsn`).
- `POST /servers`, `DELETE /servers/{id}` — start a create/delete task and return it (`202`).
- `GET /servers`, `GET /servers/{id}`, `GET /tasks`, `GET /tasks/{id}`.

See [docs/usecode-agent-architecture.md](../../docs/usecode-agent-architecture.md)
("Provider resources and tasks").

## AI model API

Kick-starts `llama-server` as a container on the usecode-agent-api host (via `podman`/`docker`).
All `/models/*` endpoints require an API key.

- `GET /models/options` — configurable fields, each with its default and (where applicable) its
  allowed options.
- `GET /models/status` — whether the model container is running.
- `POST /models/start` — body is a JSON object of optional field overrides (see
  `/models/options`), e.g. `{"hf_repo": "ggml-org/Qwen3-4B-GGUF:Q4_0"}`.
  Omitted fields fall back to their default, reproducing:
  `llama-server -hf ggml-org/Qwen3-0.6B-GGUF:Q4_0 --device Vulkan0 -ngl 99 --alias local-model
  -c 32768 --host 127.0.0.1 --port 8080`.
- `POST /models/stop` — stops and removes the container.

## Run

Every API instance must be given a name — the process refuses to start
without one, because a task's assignee is a node name, and it is also what
addresses the task (see
[docs/usecode-agent-architecture.md](../../docs/usecode-agent-architecture.md),
"Horizontal scaling"). It needs a PostgreSQL database; migrations run on
startup.

```sh
USECODE_AGENT_NODE_NAME=api-1 cargo run   # http://localhost:8000
```

`make build` / `make install` / `make check` work as in the other Rust
libraries.

The full stack — two Caddy load balancers, two named API instances, two
PostgreSQL instances — comes up with:

```sh
podman-compose -f deploy/compose.yml up -d --build   # https://localhost:4430 (and :4431)
```

## Configuration

Settings are read from `USECODE_AGENT_*` environment variables, and from a
`.env` file in the working directory (the environment wins). Copy
`.env.example` to `.env` and adjust as needed. By default no SMS provider is
configured, so OTP codes are logged to the console and echoed back in the
`/auth/otp/request` response (`debug_code`) so the frontend can be exercised
without a real SMS provider. Set `USECODE_AGENT_DEBUG_EXPOSE_OTP=false` and
`USECODE_AGENT_SMS_API_URL` / `USECODE_AGENT_SMS_API_KEY` (a generic HTTP
provider that accepts `{to, sender, message}`) before deploying.

- `USECODE_AGENT_BIND` — listen address, default `0.0.0.0:8000`.
- `USECODE_AGENT_MODEL_CONTAINER_CLI` — container tooling used for `/models/*`: `podman`
  (default) or `docker`.
- `USECODE_AGENT_HETZNER_API_BASE` / `USECODE_AGENT_DIGITALOCEAN_API_BASE` — override a
  provider's API base URL, e.g. to point at a fake in tests.
- `RUST_LOG` — log filter, default `info`.

Horizontal scaling settings:

- `USECODE_AGENT_NODE_NAME` (required) — this instance's name, e.g. `api-1`. Tasks this instance
  starts are assigned to it, only it sweeps them, and the name is what places them: `tasks`
  is partitioned on the assignee, so a node's whole backlog is one query on one database.
- `USECODE_AGENT_DATABASE_URL` — the main/first PostgreSQL instance: what a null partition key
  resolves to, and where the `shard_ranges` map is read from. The SQLAlchemy spelling
  `postgresql+asyncpg://` is still accepted.
- `USECODE_AGENT_SHARDS` — the other instances keyed by partition key, as JSON, e.g.
  `{"b": "postgres-2:5432/usecode-agent"}`. Driver and credentials are reused from
  `USECODE_AGENT_DATABASE_URL`, since every instance runs the identical schema under the identical
  role. On the first boot of an empty deployment the 65536 virtual shards are split evenly
  over these instances plus the main one — **once per table**, since `shard_ranges` holds a
  map per table. Afterwards a row's instance is found by hashing the key of the table it
  belongs to, or of the parent it hangs off (see `src/schema.rs`).

## Migrations

`migrations/*.sql` are applied in order to every configured database on
startup, in one transaction per database under a PostgreSQL advisory lock.
Progress is recorded in the `alembic_version` table under the same revision
ids the earlier Python/Alembic implementation used, so a database it created
is picked up where it left off. To add a migration, add the next
`NNNN_name.sql` file and list it in `src/migrate.rs`.

## Code map

- `src/schema.rs` — every table's declared placement (`HashedOn` / `InheritsFrom` /
  `WholeSpace`), checked against the live schema at startup.
- `src/db.rs` — the shard registry: `virtual_shard`, the `shard_ranges` load/seed, and
  partition resolution.
- `src/store.rs` — row storage over the shards.
- `src/tasks.rs`, `src/server_tasks.rs` — the task engine and the server workflows.
- `src/providers/` — Hetzner and DigitalOcean clients.
- `src/routes/`, `src/web.rs` — the JSON API and the web UI.

## Notes

Mock chat messages are stored in memory, per process — with more than one API instance behind
the load balancer, a conversation is only visible to whichever instance happens to serve the
request. Everything else (OTP codes, users, API keys, web sessions, servers, tasks) is in
PostgreSQL. The model container's running state lives in the container runtime itself (not
in-process), so it survives API restarts; it is also per-host, so `/models/*` acts on whichever
instance serves the call.
