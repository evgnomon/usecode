<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# uc-agent-api

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
  per-user provider credentials (`hetzner`, `digitalocean`, `upcloud`), encrypted at rest.
- `GET /servers/types`, `POST /servers/sync`, `GET /servers/catalog` — the provider catalog,
  in our own type terminology (`{series}-{city}`, e.g. `x1-fsn`).
- `POST /servers`, `DELETE /servers/{id}` — start a create/delete task and return it (`202`).
  New servers get the provider account key named by `ssh_key_name` in the create body
  (`id_ed25519` if left out). The optional `ssh_public_key` is imported under that name
  only when the account doesn't have that key (or a copy of it under another name) yet.
- `GET /servers`, `GET /servers/{id}`, `GET /tasks`, `GET /tasks/{id}`.

See [docs/usecode-agent-architecture.md](../../docs/usecode-agent-architecture.md)
("Provider resources and tasks").

## Hosts calling home

The usecode daemon (`usecoded`) on a host registers itself and checks in every few
minutes; `uc daemon register` sets that up (see `lib/daemon/README.md`).

- `POST /hosts/enrollments` — with the user's `X-API-Key`: a one-time token, good for an
  hour, that `uc daemon register` hands to a host. `{ "token", "expires_at" }`
- `POST /hosts/register` — `{ "token", "name", "machine_id", "version", "arch", "os" }`
  → `{ "id", "secret", "heartbeat_seconds" }`. Uses the token up. The same `machine_id`
  registering again for the same user keeps its row and gets a new secret.
- `POST /hosts/heartbeat` — `Authorization: Bearer <secret>`,
  `{ "version", "uptime_seconds", "modules": [{ "name", "status", "detail"? }] }`.
  A `401` means the host was removed, and the daemon stops calling.
- `GET /hosts?q=&state=all|online|offline|failing&sort=name|quiet&limit=50&offset=0` — a
  page of the user's hosts (at most 200), each `online` while it called within three
  heartbeats and `failing` when online with a module in error. `q` matches the name, OS,
  version and address. The answer has the `total` over all pages and `counts` per state for
  the search, so the console never needs every host at once.
- `DELETE /hosts/{id}` — forget a host; it stops calling home.

Enrollment tokens and secrets are minted with the owner's shard in front, like web
sessions, so `host_enrollments` and `hosts` live with the user. Only their hashes are kept.

## AI model API

Kick-starts `llama-server` as a container on the uc-agent-api host (via `podman`/`docker`).
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
make up   # from the repo root: http://localhost:8430 (and :8431)
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
- `USECODE_AGENT_HETZNER_API_BASE` / `USECODE_AGENT_DIGITALOCEAN_API_BASE` /
  `USECODE_AGENT_UPCLOUD_API_BASE` — override a
  provider's API base URL, e.g. to point at a fake in tests.
- `RUST_LOG` — log filter, default `info`.

The console (usecode.dev) and its accounts:

- `USECODE_AGENT_CONSOLE_URL` — where the console's pages are, default `http://localhost:8080`.
  They call the API straight from the browser, so it is always an allowed origin, and Stripe
  sends people back there. Cookies are `Secure` once it is `https://`.
- `USECODE_AGENT_COOKIE_DOMAIN` — the domain the session cookie is set for, e.g. `usecode.dev`,
  so the console's own server sees it too.
- `USECODE_AGENT_SMTP_PASSWORD` — mail for the sign up and log in codes is on once it is set;
  `_SMTP_HOST` (default `mail.clusterlean.net`), `_SMTP_PORT` (465, TLS; others STARTTLS),
  `_SMTP_USER` (`no-reply@usecode.dev`), `_SMTP_FROM`. Without it the codes go to the log on
  an `http://` console, and can't go out on an `https://` one.
- `USECODE_AGENT_STRIPE_SECRET_KEY`, `_STRIPE_PRICE_ID`, `_STRIPE_WEBHOOK_SECRET` — billing is
  on once all three are set; Stripe's webhook goes to `/stripe/webhook`.

A missing database is made on start, so a new deployment needs nothing done by hand.

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

## The console's account API

- `GET /account` — who's looking (`user` is null for nobody), and their billing.
- `POST /auth/email/signup`, `/auth/email/login`, `/auth/email/reset` — `{email, password}`;
  mails a six digit code and sets the `uc_otp` cookie.
- `POST /auth/email/verify` — `{code}`; logs the browser in with the `uc_session` cookie, a
  session API key good for 30 days. `X-API-Key` works everywhere the cookie does.
- `POST /billing/checkout`, `/billing/portal` — `{url}` to send the browser to.
- `POST /stripe/webhook` — Stripe's events, signed.

An unsafe request with an `Origin` from a page the API doesn't know is turned away (403).
`uc-agent-api import-console URL` copies the console's old database over.

## Ship it

`make deploy` builds the image at the latest push (origin/master), pushes it to the cluster's
registry and rolls `console/api` out at it, then checks `https://api.<host>/health`.
`make push` and `make rollout` do one half each. The `console` role of `uc kube sync` runs it;
`make deploy` at the top of the checkout ships the API, then the console.

## Migrations

Every `migrations/NNNN_name.sql` is built into the binary and applied in
order to every configured database. To add one, add the next numbered file;
that's all. Once applied, a file is never edited: a change is a new file.

They run on every start, and on their own:

```sh
uc-agent-api migrate            # or `make migrate`
uc-agent-api migrate --status   # or `make migrate-status`: applied and pending, per database
```

The `migrations` table is the ledger: each applied file with its checksum,
when, and which instance applied it. Instances that start together take
turns on a PostgreSQL advisory lock, so one applies what's pending and the
rest find it done. Each migration is its own transaction with a 60s
`lock_timeout`, so a failure stops at the last good one and the next start
carries on.

During a rolling deploy the old build keeps running on the new schema: a
build that finds newer migrations than it knows still starts. So a migration
only adds what the running build can do without, and anything to drop goes
in a later release. A build refuses to start if an applied file changed, or
the history otherwise doesn't match its own.

A database made by the earlier Alembic setup is adopted the first time:
everything up to its `alembic_version` goes in the ledger, and that table
is dropped.

The tests that need Postgres run against `make test-db` (podman, port
55433) and skip themselves without it.

## Code map

- `src/schema.rs` — every table's declared placement (`HashedOn` / `InheritsFrom` /
  `WholeSpace`), checked against the live schema at startup.
- `src/db.rs` — the shard registry: `virtual_shard`, the `shard_ranges` load/seed, and
  partition resolution.
- `src/store.rs` — row storage over the shards.
- `src/tasks.rs`, `src/server_tasks.rs` — the task engine and the server workflows.
- `src/providers/` — Hetzner, DigitalOcean and UpCloud clients.
- `src/routes/`, `src/web.rs` — the JSON API and the web UI.

## Notes

Mock chat messages are stored in memory, per process — with more than one API instance behind
the load balancer, a conversation is only visible to whichever instance happens to serve the
request. Everything else (OTP codes, users, API keys, web sessions, servers, tasks) is in
PostgreSQL. The model container's running state lives in the container runtime itself (not
in-process), so it survives API restarts; it is also per-host, so `/models/*` acts on whichever
instance serves the call.
