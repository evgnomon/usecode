"""Every table here is partitioned, directly or through its parent. There
are no exceptions and no "global" tables.

A model with **no foreign key** is partitioned on its **own key**: hash it
into a virtual shard (`db.virtual_shard`), look the bucket up in the
`shard_ranges` rows for that table, and that is the instance holding the
row. `users` hashes the user id; `user_directory` and `otps` hash the phone
number, which is why they need maps of their own rather than following the
user's; `api_keys` hashes the key hash; `tasks` hashes the assignee. It
does *not* mean the row is on the main database — the main database is
simply where a bucket lands when the map points there.

`api_keys` is a root table for the same reason `otps` is one: a request
carrying an API key has no user id yet — the key is what produces one — so
the key hash is the only address it can have. Its `user_id` is a plain
column rather than a foreign key, since the owning `users` row hashes
elsewhere; `user_api_keys` is the index that answers the reverse question.

`tasks` is a root table for a different reason: not because of what a
request carries, but because of how the rows are *read*. A task is never
looked up by a user asking for it — each API instance sweeps its own
outstanding work on a timer — so the query that has to be cheap is "this
node's tasks", and hashing the assignee answers it on one instance instead
of a walk across every one of them. Its `user_id` is a plain column too,
and `user_tasks` is its reverse index.

A model **with** a foreign key lives on the instance holding the row its
*first* foreign key points at — no hashing of its own, it inherits its
parent's placement, so it needs no `shard_ranges` rows at all. Later
foreign keys are ordinary references and say nothing about placement.
Following the first FK up the chain ends at a table that hashes its own
key, which is why placement is always computed and never searched for. A
column that names a row on another instance — `WebSession.api_key_hash`,
`ApiKey.user_id` — is deliberately *not* a foreign key, because no database
could enforce it.

Every lookup therefore names a table: `db.partition_for_key(table, key)`
reads that table's map, and `db.partition_for_table(table)` answers for a
table whose whole bucket space is one range. The catalog tables
(`server_type_mappings`, `location_mappings`, `provider_resources`) are of
that second kind, because they are read by scans over non-key predicates
rather than by their key — a hash could not address those queries. So is
`shard_ranges` itself, by necessity: it is the map, so it has to be
readable before any map has been read. Neither is an exception to the
rule — a single whole-space range is a map like any other, one whose answer
happens not to vary by bucket.

None of this is left to be inferred from the columns: every model states
its placement as `__placement__` — `HashedOn(key)`, `InheritsFrom(column,
parent)`, or `WholeSpace(why)` — so the question "where does this row
live?" is answered by the model itself. `db` reads those declarations
instead of keeping lists of its own, and `check_placements()` refuses to
start if one disagrees with the schema it is declared against.
"""

import uuid
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import ClassVar

from sqlalchemy import JSON, Column, DateTime, UniqueConstraint
from sqlmodel import Field, SQLModel


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


def _tz_column(**kwargs) -> Column:
    return Column(DateTime(timezone=True), **kwargs)


# -- Placement -----------------------------------------------------------
# Every model below states, next to its columns, which of the three kinds
# of placement it has. The kinds are exactly the ones the module docstring
# describes; spelling them out here means "where does this row live?" is
# answered by reading the model, not by remembering a rule or by finding
# the table's name in a set somewhere else.
#
# `db` derives its behaviour from these declarations rather than keeping
# its own list: `mapped_tables()` is the tables that need `shard_ranges`
# rows (`HashedOn` and `WholeSpace`), `WHOLE_SPACE_TABLES` is the
# `WholeSpace` ones, and `check_placements()` fails at startup if a
# declaration and the schema disagree — so a model whose foreign keys
# change cannot keep a stale declaration.


@dataclass(frozen=True)
class HashedOn:
    """Partitioned on this table's own key: `db.virtual_shard(key)` picks
    the bucket, and this table's rows in `shard_ranges` say which instance
    owns it. For tables with no foreign key, which therefore have nothing
    to inherit placement from. `key` must be a primary-key column, since a
    row has to be addressable by what callers actually hold."""

    key: str


@dataclass(frozen=True)
class InheritsFrom:
    """Lives on the instance holding the row `column` points at — the
    table's *first* foreign key. No hashing and no `shard_ranges` rows of
    its own; being co-located with the parent is also what lets `column` be
    a real foreign key. `parent` is the table it resolves through, recorded
    so the chain up to a `HashedOn` table can be read off the models."""

    column: str
    parent: str


@dataclass(frozen=True)
class WholeSpace:
    """One range covering every bucket, on the main database. Not an
    exception to the scheme — a map like any other, whose answer happens
    not to vary by bucket — and `db.partition_for_table` is how such a
    table is addressed. `why` is required because "don't hash this one"
    always needs a reason: a table is whole-space when its queries are
    scans over non-key predicates, or (for `shard_ranges`) when it is the
    map and so must be readable before any map has been read."""

    why: str


Placement = HashedOn | InheritsFrom | WholeSpace


def table_models() -> dict[str, type[SQLModel]]:
    """Every mapped model, by table name."""
    found: dict[str, type[SQLModel]] = {}
    pending = list(SQLModel.__subclasses__())
    while pending:
        model = pending.pop()
        pending.extend(model.__subclasses__())
        table = getattr(model, "__table__", None)
        if table is not None:
            found[table.name] = model
    return found


def placement_of(table: str) -> Placement:
    """One table's declared placement."""
    model = table_models().get(table)
    if model is None:
        raise KeyError(f"No model defines table {table!r}")
    placement = getattr(model, "__placement__", None)
    if placement is None:
        raise RuntimeError(
            f"{model.__name__} declares no __placement__; every table states "
            "how it is placed (HashedOn / InheritsFrom / WholeSpace)"
        )
    return placement


def whole_space_tables() -> frozenset[str]:
    """The tables declared `WholeSpace` — what `db.WHOLE_SPACE_TABLES` is."""
    return frozenset(
        name
        for name, model in table_models().items()
        if isinstance(getattr(model, "__placement__", None), WholeSpace)
    )


def check_placements() -> None:
    """Fail if any model's declared placement disagrees with its schema.
    Called at startup, before the shard map is seeded: a placement that has
    drifted from the foreign keys would route queries to the wrong
    instance, which no later check would notice."""
    for name, model in sorted(table_models().items()):
        table = model.__table__
        placement = placement_of(name)
        first_fk = next(
            (column for column in table.columns if column.foreign_keys), None
        )
        if isinstance(placement, InheritsFrom):
            if first_fk is None:
                raise RuntimeError(
                    f"{model.__name__} inherits placement from "
                    f"{placement.column!r} but {name} has no foreign key"
                )
            if first_fk.name != placement.column:
                raise RuntimeError(
                    f"{model.__name__} inherits placement from "
                    f"{placement.column!r}, but {name}'s first foreign key is "
                    f"{first_fk.name!r} — placement follows the first one"
                )
            parents = {fk.column.table.name for fk in first_fk.foreign_keys}
            if parents != {placement.parent}:
                raise RuntimeError(
                    f"{model.__name__} declares parent {placement.parent!r} "
                    f"but {name}.{first_fk.name} points at {sorted(parents)}"
                )
            continue

        if first_fk is not None:
            raise RuntimeError(
                f"{model.__name__} is declared {type(placement).__name__}, "
                f"which needs its own shard_ranges map, but {name} has a "
                f"foreign key on {first_fk.name!r} and so inherits its "
                "parent's instance — declare InheritsFrom instead"
            )
        if isinstance(placement, HashedOn):
            column = table.columns.get(placement.key)
            if column is None:
                raise RuntimeError(
                    f"{model.__name__} hashes on {placement.key!r}, which is "
                    f"not a column of {name}"
                )
            if not column.primary_key:
                raise RuntimeError(
                    f"{model.__name__} hashes on {placement.key!r}, which is "
                    f"not part of {name}'s primary key — a row has to be "
                    "addressable by the key callers hold"
                )


class ShardRange(SQLModel, table=True):
    """Which physical database instance holds a contiguous run of **virtual
    shards**, for one table. Only the **main** (first) database's copy of
    this table is ever read — that instance is the one that knows the whole
    topology, exactly as it is the one a null partition key resolves to.

    Every table that hashes its own key (every table with no foreign key —
    see the module docstring) has its own set of rows here, named by
    `table`. Carrying the table name makes placement explicit rather than
    implied: a lookup says *which* map it is reading, and tables can be
    spread differently from one another. Tables that do have a foreign key
    need no rows at all, since they inherit their parent's instance.

    A row's virtual shard is `db.virtual_shard(key)`: a fixed hash of that
    table's own key — the user id for `users`, the phone number for
    `user_directory` — into `db.VIRTUAL_SHARDS` (65536) buckets. Nothing
    stores it; it is recomputed from the key every time.

    `start_bucket`/`end_bucket` are inclusive, and for each `table` the rows
    tile 0..VIRTUAL_SHARDS-1 with no gaps or overlaps. `partition_key` is
    the instance's name, stored as the empty string for the main database
    (`db.MAIN_PARTITION_ROW_KEY`) since PostgreSQL can't hold NULL in a
    primary key; everywhere else in the codebase that partition is spelled
    `None`.

    A table whose whole bucket space is one range on the main database is
    not an exception to any of this — it is a map like every other, whose
    answer happens to be the same for every bucket. `shard_ranges` itself is
    that case by necessity: it is the map, so it has to be readable before
    any map has been read.

    Rows are seeded once per table from the `USECODE_AGENT_SHARDS` setting and
    never re-pointed afterwards: a bucket's rows are physically on that
    instance, so moving the range without moving the rows would strand them.
    Splitting a range is how an instance gets added later — and it means
    physically moving the buckets that changed hands."""

    __tablename__ = "shard_ranges"
    __placement__: ClassVar[Placement] = WholeSpace(
        "it is the map, so it has to be readable before any map has been read"
    )

    # Which table's map this row belongs to, e.g. "users".
    table: str = Field(primary_key=True)
    start_bucket: int = Field(primary_key=True)  # inclusive
    end_bucket: int  # inclusive
    partition_key: str
    host: str
    port: int = Field(default=5432)
    database: str
    created_at: datetime = Field(
        default_factory=_utcnow, sa_column=_tz_column(nullable=False)
    )


class UserDirectory(SQLModel, table=True):
    """Phone number -> user id.

    Everything a user owns is found from their **user id**, which hashes
    straight to the instance holding it, so no child table needs its own
    placement record. A phone number is the one key that isn't a user id: at
    login there is no user id yet, so this directory is what turns the phone
    into one. It is deliberately the only such mapping — the id it returns
    is enough to reach every other row.

    It has no foreign key, so it is partitioned on its own key like any
    other root table: `db.virtual_shard(phone)` picks the bucket and the
    `user_directory` rows of `shard_ranges` say which instance holds it.
    That the phone is not a user id is exactly why it needs a map of its
    own — its rows do not follow the user's."""

    __tablename__ = "user_directory"
    __placement__: ClassVar[Placement] = HashedOn("phone")

    phone: str = Field(primary_key=True)
    user_id: uuid.UUID = Field(index=True)
    created_at: datetime = Field(
        default_factory=_utcnow, sa_column=_tz_column(nullable=False)
    )


class User(SQLModel, table=True):
    """The root of the user-owned tree, and what every foreign key below
    eventually points at. No foreign key of its own, so it is partitioned on
    `id`: `db.virtual_shard(id)` recomputes the bucket and the `users` rows
    of `shard_ranges` on the main database say which instance holds it.
    There is no stored partition key."""

    __tablename__ = "users"
    __placement__: ClassVar[Placement] = HashedOn("id")

    id: uuid.UUID = Field(default_factory=uuid.uuid4, primary_key=True)
    phone: str = Field(unique=True, index=True)
    created_at: datetime = Field(
        default_factory=_utcnow, sa_column=_tz_column(nullable=False)
    )


class Otp(SQLModel, table=True):
    """A pending login code. Keyed by phone number, and every read and write
    of it names that phone, so — having no foreign key — it is partitioned on
    the phone hash exactly as `user_directory` is. It cannot be keyed by user
    id: an OTP exists before the user it will log in does."""

    __tablename__ = "otps"
    __placement__: ClassVar[Placement] = HashedOn("phone")

    phone: str = Field(primary_key=True)
    code_hash: str
    expires_at: datetime = Field(sa_column=_tz_column(nullable=False))
    resend_after: datetime = Field(sa_column=_tz_column(nullable=False))
    attempts: int = Field(default=0)
    created_at: datetime = Field(
        default_factory=_utcnow, sa_column=_tz_column(nullable=False)
    )


class ApiKey(SQLModel, table=True):
    """A bearer token, found by the token and nothing else.

    Every request that carries one arrives with the key alone: there is no
    user id yet — the key is what produces one — so the key cannot be
    addressed through its owner. It has **no foreign key**, and is therefore
    partitioned on its own key like any other root table:
    `db.virtual_shard(key_hash)` picks the bucket and the `api_keys` rows of
    `shard_ranges` say which instance holds it. That makes authentication a
    single lookup on a single instance, from the token by itself.

    `user_id` is a plain column, not a foreign key: the owning `users` row
    hashes on the user id and this row hashes on the key hash, so the two
    are on unrelated instances and no database could enforce the reference.
    It is written once at issue time and never null — a key always belongs
    to someone; what changes is that the belonging is a fact recorded here
    rather than a constraint.

    Going the other way — *this user's* keys, for listing and revoking —
    is what `UserApiKey` is for, since a user id says nothing about where
    their keys hashed to."""

    __tablename__ = "api_keys"
    __placement__: ClassVar[Placement] = HashedOn("key_hash")

    key_hash: str = Field(primary_key=True)
    # Not a foreign key: the `users` row is on another instance. See above.
    user_id: uuid.UUID = Field(index=True)
    label: str = Field(default="")
    created_at: datetime = Field(
        default_factory=_utcnow, sa_column=_tz_column(nullable=False)
    )
    last_used_at: datetime | None = Field(
        default=None, sa_column=_tz_column(nullable=True)
    )
    revoked_at: datetime | None = Field(
        default=None, sa_column=_tz_column(nullable=True)
    )


class UserApiKey(SQLModel, table=True):
    """A user's own index of the API keys they were issued: user id -> key
    hash. Its first (and only) foreign key is `user_id`, so it lives with
    the `users` row and carries a real constraint to it.

    `ApiKey` hashes on the key, which is the only thing an arriving request
    has. Nothing about a user id predicts those buckets, so without this
    index answering "which keys does this user have?" would mean scanning
    every instance — the one thing the scheme does not do. Listing reads
    these rows on the owner's instance and then fetches each key from the
    instance its hash resolves to.

    It records issuance, not liveness: a revoked key keeps both its row and
    its index entry, and `revoked_at` on the `api_keys` row remains the only
    answer to whether the key still works."""

    __tablename__ = "user_api_keys"
    __placement__: ClassVar[Placement] = InheritsFrom("user_id", "users")

    user_id: uuid.UUID = Field(
        foreign_key="users.id", primary_key=True, ondelete="CASCADE"
    )
    # The `api_keys` row this points at is on another instance, so deleting
    # a user cascades this index away but not the keys themselves; they are
    # unreachable once no token for them exists.
    key_hash: str = Field(primary_key=True)
    created_at: datetime = Field(
        default_factory=_utcnow, sa_column=_tz_column(nullable=False)
    )


class WebSession(SQLModel, table=True):
    """A browser cookie. Placed by `user_id`, its first foreign key, so it
    sits with its owner — unlike `ApiKey`, a session token is only ever
    presented after the API key that minted it has already resolved the
    user."""

    __tablename__ = "web_sessions"
    __placement__: ClassVar[Placement] = InheritsFrom("user_id", "users")

    token_hash: str = Field(primary_key=True)
    user_id: uuid.UUID = Field(foreign_key="users.id", index=True, ondelete="CASCADE")
    # Which API key this session was minted alongside, so logging out can
    # revoke it too. A plain column: `api_keys` is partitioned on the key
    # hash, so that row is on an unrelated instance.
    api_key_hash: str
    created_at: datetime = Field(
        default_factory=_utcnow, sa_column=_tz_column(nullable=False)
    )
    revoked_at: datetime | None = Field(
        default=None, sa_column=_tz_column(nullable=True)
    )


class ProviderCredential(SQLModel, table=True):
    __tablename__ = "provider_credentials"
    __placement__: ClassVar[Placement] = InheritsFrom("user_id", "users")

    # Keyed by user, so it hashes to the same instance the owning `users`
    # row is on (see ShardRange) — which is what lets this be a real foreign
    # key rather than a convention the application has to uphold.
    user_id: uuid.UUID = Field(
        foreign_key="users.id", primary_key=True, ondelete="CASCADE"
    )
    provider: str = Field(primary_key=True)
    # Encrypted JSON blob, e.g. {"apiKey": "..."} — shape is provider-specific.
    credentials_encrypted: str
    created_at: datetime = Field(
        default_factory=_utcnow, sa_column=_tz_column(nullable=False)
    )
    updated_at: datetime = Field(
        default_factory=_utcnow, sa_column=_tz_column(nullable=False)
    )


class ServerTypeMapping(SQLModel, table=True):
    """Maps our own server-type series (e.g. "x1", "y1") to a provider and
    that provider's instance-type slug. Minted once (by POST /servers/sync
    or GET /servers/types) and fixed from then on, since each series is
    only ever offered by one provider."""

    __tablename__ = "server_type_mappings"
    __placement__: ClassVar[Placement] = WholeSpace(
        "read by scans over non-key predicates — every series a provider "
        "offers — which hashing the series would not address"
    )

    series: str = Field(primary_key=True)
    provider: str
    provider_server_type: str
    # Our own city codes (see LocationMapping) this series is known to be
    # available in, accumulated across GET /servers/types and POST
    # /servers/sync calls — never removed, only added to.
    cities: list = Field(default_factory=list, sa_column=Column(JSON, nullable=False))


class LocationMapping(SQLModel, table=True):
    """Maps our own location code (the city half of a "{series}-{city}"
    server type, e.g. "fsn" or "nyc3") plus a provider to that provider's raw
    location/region code. Keyed on (code, provider) rather than code alone
    because different providers can independently mint the same city code
    (e.g. both deriving "fra" for Frankfurt) — a series is always tied to one
    provider (see ServerTypeMapping), so resolving a "{series}-{city}" type
    must look up the row for that specific provider, not just the city.
    Minted once per (code, provider) by POST /servers/sync and fixed from
    then on — this is what lets POST /servers translate a city back to the
    provider's own code instead of guessing."""

    __tablename__ = "location_mappings"
    __placement__: ClassVar[Placement] = WholeSpace(
        "read by scans over non-key predicates — every city known for a "
        "provider — which hashing the code would not address"
    )

    code: str = Field(primary_key=True)
    provider: str = Field(primary_key=True)
    provider_location_code: str


class ProviderResource(SQLModel, table=True):
    """Raw catalog data mirrored from a provider by POST /servers/sync:
    locations, server types, OS images — everything needed to fill out a
    create-server spec for that provider. `data` is the provider's own JSON
    shape for the resource, unmodified; `code` is the provider's own
    identifier for it (e.g. Hetzner location "fsn1", image "ubuntu-24.04",
    DigitalOcean size "s-2vcpu-2gb")."""

    __tablename__ = "provider_resources"
    __placement__: ClassVar[Placement] = WholeSpace(
        "read by scans over non-key predicates — every resource of a kind a "
        "provider has — which hashing the code would not address"
    )
    __table_args__ = (
        UniqueConstraint(
            "provider", "kind", "code", name="uq_provider_resources_kind_code"
        ),
    )

    id: uuid.UUID = Field(default_factory=uuid.uuid4, primary_key=True)
    provider: str = Field(index=True)
    kind: str = Field(index=True)  # "location", "server_type", or "image"
    code: str
    data: dict = Field(sa_column=Column(JSON, nullable=False))
    updated_at: datetime = Field(
        default_factory=_utcnow, sa_column=_tz_column(nullable=False)
    )


class Server(SQLModel, table=True):
    __tablename__ = "servers"
    __placement__: ClassVar[Placement] = InheritsFrom("user_id", "users")
    __table_args__ = (
        UniqueConstraint(
            "provider", "provider_server_id", name="uq_servers_provider_id"
        ),
    )

    id: uuid.UUID = Field(default_factory=uuid.uuid4, primary_key=True)
    # Co-located with its owner — see ProviderCredential.
    user_id: uuid.UUID = Field(foreign_key="users.id", index=True, ondelete="CASCADE")
    provider: str
    provider_server_id: str = Field(index=True)
    type: str
    name: str
    status: str
    public_ip4: str | None = None
    public_ip6: str | None = None
    created_at: datetime = Field(
        default_factory=_utcnow, sa_column=_tz_column(nullable=False)
    )
    updated_at: datetime = Field(
        default_factory=_utcnow, sa_column=_tz_column(nullable=False)
    )


class Task(SQLModel, table=True):
    """A suspended, resumable workflow that mutates provider-owned resources.

    See AGENTS.md ("Provider resources and tasks") for why this exists: any
    resource whose fixed attributes we mirror locally (servers, volumes,
    networks, ...) may only be created/changed/destroyed via a task, because
    the provider-side operation can be asynchronous and take a long time.
    `state` is the workflow's program counter — the step it is parked at,
    waiting for the next external event (a provider API call, a poll
    result). `resources` records which of our own DB rows the task is
    acting on, so callers can tell a resource is mid-workflow without
    reading task internals.

    It has **no foreign key** and is partitioned on `assignee`, the node
    name of the API instance carrying it: that is how tasks are actually
    read. Nobody waits on a task by id — every instance sweeps its own
    outstanding work on a timer — so the one query that has to be cheap is
    "this node's tasks", and hashing the node name answers it on a single
    instance instead of a walk over all of them. `assignee` is therefore
    part of the primary key and never null: a task is addressed as
    (assignee, id), and an unassigned task would have no address at all.

    Hashing the assignee places a task away from the user rows it mutates,
    so `user_id` is a plain column rather than a foreign key — the same
    situation as `ApiKey.user_id`, and for the same reason: no database
    could enforce a reference to another instance. Step handlers resolve
    the *owner's* partition from `user_id` and reach servers and
    credentials through that. `UserTask` is the index that answers the
    reverse question — which tasks a user has — since a user id says
    nothing about which node picked their work up.
    """

    __tablename__ = "tasks"
    __placement__: ClassVar[Placement] = HashedOn("assignee")

    # Which API instance owns this task: the node name of the instance that
    # picked it up. Part of the key, because it is also this row's address
    # (see above), and never null so that address always exists. Only the
    # assignee advances the task, so two instances sweeping concurrently
    # never run the same step twice. It leads the primary key so that a
    # node's sweep — "everything assigned to me" — is a scan of one
    # contiguous run of it rather than a secondary lookup.
    assignee: str = Field(primary_key=True)
    id: uuid.UUID = Field(default_factory=uuid.uuid4, primary_key=True)
    # Not a foreign key: the `users` row is on another instance. See above.
    user_id: uuid.UUID = Field(index=True)
    kind: str = Field(index=True)
    state: str
    # [{"type": "server", "id": "<our row id>"}, ...] — informational, and
    # used to detect a resource that already has a task in flight.
    resources: list = Field(
        default_factory=list, sa_column=Column(JSON, nullable=False)
    )
    # Step-local working data, e.g. {"provider": "hetzner", "provider_server_id": "123"}.
    payload: dict = Field(default_factory=dict, sa_column=Column(JSON, nullable=False))
    error: str | None = None
    created_at: datetime = Field(
        default_factory=_utcnow, sa_column=_tz_column(nullable=False)
    )
    updated_at: datetime = Field(
        default_factory=_utcnow, sa_column=_tz_column(nullable=False)
    )


class UserTask(SQLModel, table=True):
    """A user's own index of the tasks running on their behalf: user id ->
    (task id, assignee). Its first (and only) foreign key is `user_id`, so
    it lives with the `users` row and carries a real constraint to it.

    `Task` hashes on the assignee, which is what the sweep holds. Nothing
    about a user id predicts which node picked their work up, so without
    this index answering "what is in flight for me?" (GET /tasks) would
    mean scanning every instance — the one thing the scheme does not do.
    `assignee` is carried here because it is half of the task's address:
    the index row is what turns a task id into an instance to read it from.

    A row lives exactly as long as the task does — the engine deletes both
    when a workflow completes — so this index is also the record of what is
    still outstanding for a user."""

    __tablename__ = "user_tasks"
    __placement__: ClassVar[Placement] = InheritsFrom("user_id", "users")

    user_id: uuid.UUID = Field(
        foreign_key="users.id", primary_key=True, ondelete="CASCADE"
    )
    task_id: uuid.UUID = Field(primary_key=True)
    # The `tasks` row this names is on another instance, so deleting a user
    # cascades the index away but not the task; the sweep that owns it is
    # what finishes and removes it.
    assignee: str
    created_at: datetime = Field(
        default_factory=_utcnow, sa_column=_tz_column(nullable=False)
    )
