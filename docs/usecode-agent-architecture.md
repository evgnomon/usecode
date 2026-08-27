# AGENTS.md

Architecture notes for whoever (human or agent) is working on this repo.

## Horizontal scaling

**Every tier runs at least two instances, and nothing may assume there is
only one of anything.** `deploy/compose.yml` runs two Caddy load balancers,
two API instances, and two PostgreSQL instances. That is not a redundancy
detail bolted on at deploy time — it is a constraint the code is written
against, and the two mechanisms below are what make it hold.

### Named API instances and task assignees

**Every API instance has a name and refuses to start without one.**
`USECODE_AGENT_NODE_NAME` (e.g. `api-1`, or `worker-1`) is a required setting;
`Settings` has no default for it, so an unnamed process fails validation at
import. That is deliberate rather than fussy:

- **A task carries an `assignee`** — the node name of the instance that
  picked it up. `create_task` stamps it with the handling instance's own
  name, and `tasks.sweep()` advances only the tasks assigned to *this*
  instance. Without that, every instance would re-run every parked step on
  every tick, issuing duplicate provider calls.
- **The assignee is also where the task is stored.** `tasks` is
  partitioned on it (see "Partitioning" below), because sweeping is how
  tasks are read: an instance's whole backlog is one query against the one
  database its node name hashes to, instead of a walk over every instance
  filtering on assignee. The assignee is consequently part of the primary
  key and never null — a task is addressed as (assignee, id), so an
  unnamed node's work would have no address. `user_tasks`, which lives
  with the user, is the index behind GET /tasks.

Requests themselves are not sticky: Caddy round-robins over both API
instances and a request's rows are found through the authenticated user's
partition key, not through which instance served the previous request. Only
*tasks* are owned, because a task is a resumable workflow that must not be
stepped by two instances at once.

### Partitioned databases

**Every database instance carries the identical schema; what differs is
which rows are on which.** Placement is **computed, never stored**:

- A key hashes into one of **65536 virtual shards** — `db.virtual_shard`, a
  BLAKE2b of a UUID's raw bytes or of any other key's UTF-8 text. It is a
  pure function: no I/O, no coordination, same answer in every process.
- **`shard_ranges`**, a table on the main database, maps a contiguous run of
  buckets to the instance holding them — **one map per table**, since every
  row of it names the table whose map it belongs to. That is the only
  indirection, and the first database is the one that knows the whole
  topology.
- The instance a bucket resolves to is its **partition key**. A **null**
  partition key means the **main/first** database — the one
  `USECODE_AGENT_DATABASE_URL` points at.

So the entire address of a user-owned row is its owner's user id: hash it
against the `users` map, find the range, and that is the instance. **No
user-owned row can be retrieved without an owning user id** — except the
one row a user id cannot address, an `api_keys` row, which is why that
table hashes its own key instead of inheriting a placement.
`db.session(partition_key)` is the only way to obtain a session,
`db.partition_for_key` (and its `users` shorthand
`db.partition_for_user`) is the only way to compute one, and every `store`
method that touches a table with a foreign key either resolves it from a
user id or takes `partition_key` as its first argument with no default.
There is deliberately no "search every instance" path for row data.

Because a user and everything keyed by their id land on the *same*
instance, the partitioned tables carry **real foreign keys**, with
`ON DELETE CASCADE` — the co-location is enforced by the database, not by
convention.

**A model's foreign keys are what say where it lives.** Read a table's
fields in order:

- **No foreign key at all** — the table is partitioned *directly*: its own
  key is the whole address. Hash the key into a virtual shard, look the
  bucket up in **that table's** rows in `shard_ranges`, and that is the
  instance. This does **not** mean the main database — the main database is
  just where a bucket lands when the map points there. `users` hashes the
  user id; `user_directory` and `otps` hash the phone number.
- **A foreign key** — the row lives wherever the row it points at lives,
  inheriting that placement rather than hashing anything of its own, so it
  needs no `shard_ranges` rows at all. The **first** foreign key in the
  field order is the parent; any later foreign keys are ordinary references
  and have no say in placement (they resolve on the same instance because
  the parent chain already put them there).

A column that names a row on *another* instance is therefore deliberately
**not** a foreign key — `web_sessions.api_key_hash` and `api_keys.user_id`
are both plain columns, because the row each points at hashes into a map of
its own and no database could enforce the reference.

Applied transitively this makes placement total — every table is
partitioned either directly or through its parent, and every chain of
parents ends at a table that hashes its own key, so no table is left whose
instance has to be searched for. **There are no global tables and no
exceptions.**

Because there is a map per table, **every lookup names the table it is
reading**:

- `db.partition_for_key(table, key)` — hash `key` against `table`'s map.
- `db.partition_for_user(user_id)` — the `users` shorthand, and what every
  table with a foreign key back to `users` is addressed through.
- `db.partition_for_table(table)` — for a table in `db.WHOLE_SPACE_TABLES`,
  whose map is a single range covering every bucket.

A whole-space map is not an exception either; it is a map like any other
whose answer doesn't vary by bucket. Two kinds of table have one.
`shard_ranges` must, by necessity: it *is* the map, so it has to be
readable before any map has been read, and every one of its buckets
resolving to the main database is what makes that possible. The catalog
tables — `server_type_mappings`, `location_mappings`, `provider_resources`
— have one because they are read by scans over non-key predicates ("every
resource this provider has"), which no key hash could address.

Which tables are which:

- **Hash their own key** (no foreign key): `users`, `user_directory`,
  `otps`, `api_keys`.
- **Inherit a parent** (first foreign key decides): `user_api_keys`,
  `web_sessions`, `provider_credentials`, `servers`, `tasks` — all
  `user_id` -> `users`.
- **Whole-space map** (one range, main database): `shard_ranges`,
  `server_type_mappings`, `location_mappings`, `provider_resources`.

Note what the first group means in practice: a phone number and the user it
names are *not* co-located. `get_or_create_user` reads two unrelated maps
and can write the directory entry on one instance and the user row on
another. That is the scheme working, not a bug — the directory's job is to
turn a phone into a user id, and the id is what addresses everything after.

Three lookups don't start from a user id, and each has an answer:

- **A phone number**, at login. `user_directory` (phone -> user id) is the
  one mapping that exists, because at that point there is no user id yet to
  hash. It is deliberately the *only* one — the id it returns is enough to
  reach every other row. It is partitioned on the phone hash like any other
  root table.
- **An API key**, which is looked up by its own hash and, like an OTP,
  exists before there is a user id to hash: the key is what *produces* the
  user id. So `api_keys` is a root table partitioned on `key_hash` —
  authentication is one lookup on one instance, from the token alone. Its
  `user_id` is a plain column, and `user_api_keys` (a child of `users`) is
  the index that turns a user id back into their key hashes, so listing and
  revoking never scan.
- **A web session cookie**, also looked up by its own hash, but only ever
  handed out *after* the user is known — so `web_sessions` stays a child of
  `users` and the token is instead *minted* with the owner's virtual shard
  in front (`"1a2b.<secret>"`), the prefix routing the lookup to the
  owner's instance. The bucket is one of the `users` map, since that is the
  map the row's placement follows. A token with no prefix predates the
  scheme and resolves to the main database, which is the only place such a
  row can be.

Ranges are seeded **once per table**, on the first boot after that table
exists: its bucket space is split evenly over the main instance plus every
`USECODE_AGENT_SHARDS` entry. Per-table rather than all-or-nothing is what lets a
table added later get its map on the next boot without disturbing the
tables that already have one. After that they are never re-pointed
automatically, and a table that *already holds rows* on the main database
is seeded with the whole space there — that is where those rows physically
are, and seeding only writes the map. Growing such a deployment means
splitting a range and moving the buckets that changed hands, which is a
deliberate operation rather than a side effect of booting.

### Implementation

- `lib/api/src/usecode_agent_api/db.py` — the shard registry: `virtual_shard`,
  `partition_for_user`, `session(partition_key)`, the `shard_ranges`
  load/seed, and `partition_keys()` (which exists only for work that
  legitimately spans instances — applying migrations and the task sweep —
  never for finding a row).
- `lib/api/src/usecode_agent_api/store.py` — global vs partitioned methods, the
  phone directory, the API-key index, and the shard prefix on session
  cookies.
- `lib/api/src/usecode_agent_api/config.py` — `node_name` (required),
  `database_url` (the main instance) and `shards` (the
  others, keyed by partition key). `shards` is bootstrap configuration: it
  seeds `shard_ranges` on first boot, and it is what migrations are applied
  to, since the range table can't be read before it exists. Runtime
  resolution always reads the table.
- `lib/api/src/usecode_agent_api/app.py` — migrates *every* instance to head on
  startup, then seeds the shard ranges. Instances boot concurrently, so each
  upgrade runs under a PostgreSQL advisory lock (`migrations/env.py`); the
  losers find the database already at head.
- `deploy/Caddyfile` — both load balancers round-robin over both API
  instances, with `/health` as the health check. `/health` reports the
  instance's own name, which is the simplest way to see which one answered.

When adding a new table, decide *first* how it is addressed. If it holds
one user's own rows, give it a foreign key to its parent as its first
foreign-key field — `users` directly, or another table that already
resolves to the owner — give its store methods a leading `partition_key`
argument, and pass `client.partition_key` from the route. If it has no
foreign key it needs its own map, which `db.seed_shard_ranges` will create
on the next boot because `db.mapped_tables()` derives the list from the
schema itself; add it to `db.WHOLE_SPACE_TABLES` only if it is read by
scans rather than by its key. A table whose rows can be reached by neither
route has no address and is a bug.

## Provider resources and tasks

**Any resource whose fixed attributes a cloud provider knows about — a
server, a volume, a network, etc. — must be mirrored in our own database.**
These attributes don't drift on their own; a server's type, region, or
existence only changes because *we* asked the provider to change it. So
our database is the source of truth for "what resources exist and what
their fixed shape is," and the provider is just where they're actually
hosted. `servers` (`lib/api/src/usecode_agent_api/db_models.py`) is the first
example of this; volumes, networks, etc. should follow the same pattern
when they're added.

**Any workflow that changes provider-known resource state must run as a
task, not inline in a request handler.** Creating, modifying, or deleting
one of these resources means calling out to the provider's API, and that
call is not guaranteed to be a quick synchronous round trip — by
assumption it can be asynchronous and take hours to actually finish (e.g.
a slow deprovisioning). An HTTP request handler cannot block on that, so
the mutation is modeled as a **task**:

- A task is a row in the `tasks` table (`Task` in `db_models.py`):
  `kind` (which workflow, e.g. `"delete_server"`), `state` (which step of
  that workflow it's currently parked at), `assignee` (which API instance
  owns it — see "Horizontal scaling" above), `resources` (which of our own
  resource rows it's acting on), and `payload` (step-local working data).
  The task row lives on the instance its `assignee` hashes to, which is
  *not* where the resources it mutates live: the assignee travels
  alongside the id everywhere in `tasks.py`, while the rows a step touches
  are reached through the owner's partition (`TaskContext` carries both,
  as `assignee` and `user_partition_key`).
- Think of a task as a suspended async function. `state` is its program
  counter. Each step is a small handler that does one unit of work —
  typically one provider API call, or one poll of provider state — and
  then either returns the next state to suspend at (waiting for the next
  external event) or signals that the workflow is complete.
- **When a task finishes, the step that finishes it is responsible for
  both removing the resource row(s) from our database *and* the task
  removing itself from the `tasks` table.** A task never outlives the
  mutation it exists to perform — there is no "completed" task state to
  query later; a task existing means work is still in flight, and a task
  no longer existing means it either never started or already finished.
- Because a task can be parked mid-workflow for arbitrarily long, nothing
  resumes it from within the request that created it beyond the first
  step. Instead a periodic sweep (`tasks.sweep()`, driven by a background
  loop started in `app.py`'s lifespan) re-invokes the handler for the
  current step of every outstanding task, so a task waiting on a slow
  provider operation eventually gets resumed and completed on its own.
  Each instance sweeps only the tasks assigned to it, in one query against
  the single database that assignee hashes to — so two API instances never
  run the same step concurrently, and nothing enumerates the instances.

### Implementation

- `lib/api/src/usecode_agent_api/tasks.py` — the generic engine: `TaskContext`,
  the `@step(kind, state)` registration decorator, `create_task` (which
  stamps the handling instance as the assignee), `advance` (run one task's
  current step once, given its assignee), and `sweep` (advance this
  instance's outstanding tasks, called on a timer).
- `lib/api/src/usecode_agent_api/server_tasks.py` — the concrete `create_server`
  and `delete_server` tasks.
  - `create_server`: `requested` (issue the provider create call,
    remembering the provider-side id) → `confirming` (poll until the
    provider reports the server reachable — a public IPv4 address — then
    write the local `servers` row and finish). No `servers` row exists
    until the provider has actually finished provisioning, since the
    provider is the only source of truth for the server's fixed
    attributes (IP addresses, final status).
  - `delete_server`: `requested` (issue the provider delete call) →
    `confirming` (poll until the provider no longer lists the server, then
    delete the local `servers` row and finish).
  These are the reference implementations to copy when a new resource type
  needs create/update/delete workflows.
- `routes/servers.py`'s `POST /servers` and `DELETE /servers/{id}` only
  *start* their task and return it (`202 Accepted` + the task) instead of
  mutating synchronously. `routes/tasks.py` exposes the generic task API —
  `GET /tasks` lists the caller's in-flight tasks, `GET /tasks/{task_id}`
  polls one; once a task finishes, `GET /tasks/{task_id}` 404s (the task
  row is gone). For deletion, `GET /servers/{id}` also 404s at that point
  (the server row is gone); for creation, the new server now shows up in
  `GET /servers`. Any future resource type's routes reuse the same
  `routes/tasks.py` endpoints instead of growing their own copy.

Follow this same shape for any future resource type or workflow: define
the DB row(s) that mirror the provider's fixed state, define the task's
states as `@step` handlers, start the task from the route instead of
doing provider work inline, and let the sweep loop carry it to
completion.
