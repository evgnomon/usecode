"""Shard-aware database access.

usecode agent runs more than one PostgreSQL instance. Every instance carries the
**identical schema**; what differs is which rows live where.

Placement is computed, not stored, and **every table is partitioned** —
there is no "global" kind. A table with no foreign key is partitioned on
its own key; a table with a foreign key inherits the placement of the row
its first foreign key points at (see ``db_models``). So only the first kind
needs a map::

    bucket = virtual_shard(key)              # pure function, no I/O

The main database's ``shard_ranges`` table maps a contiguous run of buckets
to the physical instance holding them, **per table**: every row names the
table whose map it belongs to, and every lookup names the table it is
reading. Nothing is implied by context. The whole address of a user-owned
row is therefore its owner's id — hash it against the ``users`` map — and
the whole address of a directory entry is the phone number hashed against
the ``user_directory`` map. Because a user and everything keyed by their id
land on the same instance, those child tables carry **real foreign keys
to** ``users``.

``api_keys`` is one user-owned-looking table that is *not* a child:
a request carrying an API key has no user id yet, so it hashes the key hash
against its own map, and its ``user_id`` is a plain column pointing at
another instance. ``user_api_keys`` — which *is* a child of ``users`` — is
what turns a user id back into their key hashes.

``tasks`` is the other, for a reason about reads rather than about
requests: a task is only ever fetched by the API instance carrying it, on a
sweep of its own outstanding work, so it hashes its ``assignee`` and one
node's whole backlog is one query on one instance. ``user_tasks`` is its
reverse index, the same shape as ``user_api_keys``. Nothing enumerates the
instances at runtime as a result — the only place that walks every database
is migration, which reads static configuration (:func:`configured_urls`).

The instance a bucket resolves to is its **partition key**:

- A *null* partition key means the **main** (first) database — the one
  ``USECODE_AGENT_DATABASE_URL`` points at.
- Any other partition key names one of the ``USECODE_AGENT_SHARDS`` instances.

Consequently **no user-owned row can be reached without an owning user
id**: :func:`session` is the only way to get a session, it takes the
partition key as its argument, and :func:`partition_for_key` (or its
``users`` shorthand :func:`partition_for_user`) is the only way to compute
one. There is no "try every shard" path for row data.

Some tables' maps are a single range covering the whole bucket space on one
instance — :data:`WHOLE_SPACE_TABLES`. That is not an exception to the
scheme, just a map whose answer doesn't vary by bucket;
:func:`partition_for_table` is how such a table is addressed, since its
queries are scans rather than key lookups. ``shard_ranges`` is necessarily
one of them: it is the map, so it must be readable before any map is read.

Which kind a table is, is declared on the model as ``__placement__``
(``HashedOn`` / ``InheritsFrom`` / ``WholeSpace``; see ``db_models``).
Everything here is derived from those declarations rather than from a list
kept alongside them — :data:`WHOLE_SPACE_TABLES` and :func:`mapped_tables`
both read the models — and :func:`connect` calls
``db_models.check_placements()`` first, so a declaration that has drifted
from the schema stops the process instead of mis-routing a query.

``shard_ranges`` is seeded at startup from the ``USECODE_AGENT_SHARDS`` setting so
a fresh deployment can bootstrap, but runtime resolution always reads the
table. Ranges are never re-pointed once seeded: the buckets in a range are
physically on that instance, so handing a range to a different instance
means physically moving those rows.
"""

import hashlib
import logging
import uuid
from bisect import bisect_right
from contextlib import asynccontextmanager
from typing import AsyncIterator
from urllib.parse import urlsplit, urlunsplit

from sqlalchemy import func, select as sa_select
from sqlalchemy.ext.asyncio import AsyncEngine, async_sessionmaker, create_async_engine
from sqlmodel import SQLModel, select
from sqlmodel.ext.asyncio.session import AsyncSession

from .config import Settings
from .db_models import (
    HashedOn,
    ShardRange,
    User,
    WholeSpace,
    check_placements,
    placement_of,
    whole_space_tables,
)

# How many virtual shards the user-id space is hashed into. Fixed forever:
# changing it would re-address every existing row. It is deliberately far
# larger than any plausible number of database instances, so growing the
# deployment is a matter of splitting bucket ranges rather than re-hashing.
VIRTUAL_SHARDS = 65536

# PostgreSQL can't hold NULL in a primary key, so the main shard's rows in
# `shard_ranges` use the empty string as the on-disk spelling of "the null
# partition key". Nothing outside this module sees the sentinel.
MAIN_PARTITION_ROW_KEY = ""

# Tables whose map is a single range over the whole bucket space, rather
# than a tiling across the instances. Not an exception to the scheme — a
# map like any other, whose answer just doesn't vary by bucket. Derived
# from the models: a table is here because it declares
# `__placement__ = WholeSpace(...)`, which also records *why* it isn't
# hashed, so there is no list to keep in step with the schema.
#
# `partition_for_table` is how these are addressed; it refuses any table
# whose map has more than one range.
WHOLE_SPACE_TABLES = whole_space_tables()

_log = logging.getLogger(__name__)


class UnknownPartitionError(RuntimeError):
    def __init__(self, partition_key: str) -> None:
        super().__init__(f"Unknown partition key {partition_key!r}")
        self.partition_key = partition_key


class _Shard:
    """One database instance: where it is, and the lazily-built engine."""

    def __init__(self, url: str) -> None:
        self.url = url
        self._engine: AsyncEngine | None = None
        self._sessions: async_sessionmaker[AsyncSession] | None = None

    def sessions(self) -> async_sessionmaker[AsyncSession]:
        if self._sessions is None:
            self._engine = create_async_engine(self.url)
            self._sessions = async_sessionmaker(
                self._engine, class_=AsyncSession, expire_on_commit=False
            )
        return self._sessions

    async def dispose(self) -> None:
        if self._engine is not None:
            await self._engine.dispose()
        self._engine = None
        self._sessions = None


_settings: Settings | None = None
_main: _Shard | None = None
# Partition key -> shard, for every non-main partition. Loaded from the
# `shard_ranges` table on the main database and refreshed on a miss.
_shards: dict[str, _Shard] = {}
# The bucket ranges, per table: `_ranges[table]` is two parallel sorted
# lists, where `starts[i]` is the first bucket owned by `owners[i]`. A
# bucket is resolved with one binary search; see `partition_for_bucket`.
_ranges: dict[str, tuple[list[int], list[str | None]]] = {}


# -- Virtual shards ------------------------------------------------------


def virtual_shard(key: str | uuid.UUID) -> int:
    """Which of the :data:`VIRTUAL_SHARDS` buckets a key's row belongs to.

    A fixed hash of the key — BLAKE2b truncated to the bucket width — so
    every instance and every process computes the same answer with no
    coordination and no stored mapping. (Python's own ``hash()`` is salted
    per process and would not do.)

    A UUID hashes over its 16 raw bytes and anything else over its UTF-8
    text. The UUID case is spelled out rather than folded into the text one
    because it is the placement of every user that already exists: hashing
    a user id's *string* would put it in a different bucket and re-address
    every row in the deployment."""
    if isinstance(key, uuid.UUID):
        raw = key.bytes
    else:
        text = str(key)
        try:
            raw = uuid.UUID(text).bytes
        except ValueError:
            raw = text.encode()
    digest = hashlib.blake2b(raw, digest_size=2).digest()
    return int.from_bytes(digest, "big") % VIRTUAL_SHARDS


def mapped_tables() -> list[str]:
    """Every table that needs its own map: the ones that hash their own key
    or cover the whole space, as opposed to the ones that inherit a
    parent's placement. Read off each model's `__placement__`, so a new
    table gets a map by virtue of how it declares itself and not by being
    remembered here."""
    return sorted(
        name
        for name in SQLModel.metadata.tables
        if isinstance(placement_of(name), (HashedOn, WholeSpace))
    )


# -- URL plumbing --------------------------------------------------------
# Shards are configured as bare "host:port/database" targets rather than
# full DSNs, because every instance runs the identical schema under the
# identical role — the driver and credentials come from the main URL, so
# there is only one place to change them.


def _split_target(target: str) -> tuple[str, int, str]:
    host_port, _, database = target.partition("/")
    host, _, port = host_port.partition(":")
    return host, int(port or 5432), database


def main_target(settings: Settings) -> tuple[str, int, str]:
    """The main database's (host, port, database), read off its DSN."""
    parts = urlsplit(settings.database_url)
    return parts.hostname or "localhost", parts.port or 5432, parts.path.lstrip("/")


def shard_url(settings: Settings, host: str, port: int, database: str) -> str:
    """Rebuild the main DSN — same driver, same credentials — pointed at a
    different instance."""
    parts = urlsplit(settings.database_url)
    userinfo, at, _ = parts.netloc.rpartition("@")
    netloc = f"{userinfo}{at}{host}:{port}"
    return urlunsplit(
        (parts.scheme, netloc, f"/{database}", parts.query, parts.fragment)
    )


def configured_urls(settings: Settings) -> dict[str | None, str]:
    """Every database this deployment owns, keyed by partition key (None for
    the main one), taken from *static configuration* rather than from the
    range table. Used for the two things that must work before the table
    can be read: applying migrations, and seeding the table itself."""
    urls: dict[str | None, str] = {None: settings.database_url}
    for partition_key, target in settings.shards.items():
        host, port, database = _split_target(target)
        if not partition_key:
            continue  # the main shard is `database_url`, not a shards entry
        urls[partition_key] = shard_url(settings, host, port, database)
    return urls


# -- Lifecycle -----------------------------------------------------------


async def connect(settings: Settings) -> AsyncEngine:
    global _settings, _main
    check_placements()
    _settings = settings
    _main = _Shard(settings.database_url)
    _main.sessions()
    await refresh_shard_map()
    return engine()


async def disconnect() -> None:
    global _settings, _main
    for shard in list(_shards.values()):
        await shard.dispose()
    _shards.clear()
    _ranges.clear()
    if _main is not None:
        await _main.dispose()
    _main = None
    _settings = None


def engine() -> AsyncEngine:
    """The main database's engine."""
    if _main is None or _main._engine is None:
        raise RuntimeError("Database engine is not initialized")
    return _main._engine


# -- Shard map -----------------------------------------------------------


def _even_ranges(count: int) -> list[tuple[int, int]]:
    """Split the bucket space into `count` contiguous, near-equal ranges."""
    size, remainder = divmod(VIRTUAL_SHARDS, count)
    ranges: list[tuple[int, int]] = []
    start = 0
    for index in range(count):
        width = size + (1 if index < remainder else 0)
        ranges.append((start, start + width - 1))
        start += width
    return ranges


def _instance_targets(settings: Settings) -> list[tuple[str, tuple[str, int, str]]]:
    """Every instance this deployment is configured with, main first, as
    (partition key, (host, port, database))."""
    targets = [(MAIN_PARTITION_ROW_KEY, main_target(settings))]
    for partition_key in sorted(settings.shards):
        if not partition_key:
            continue  # the main shard is `database_url`, not a shards entry
        targets.append((partition_key, _split_target(settings.shards[partition_key])))
    return targets


async def seed_shard_ranges(settings: Settings) -> None:
    """Record this deployment's bucket-to-instance topology in the main
    database's ``shard_ranges`` table, so placement resolves from the
    database rather than from whatever configuration each instance happens
    to hold.

    One map per table in :func:`mapped_tables`, seeded once each: a table
    that already has rows here is left alone, since the buckets in a range
    are physically on that instance and re-pointing one would strand them.
    Seeding per table rather than all-or-nothing is what lets a table added
    later get its map on the next boot without disturbing the tables that
    already have one.

    A table in :data:`WHOLE_SPACE_TABLES` gets a single range on the main
    database. So does a table that *already holds rows* on the main
    database in a deployment that has other instances — those rows are
    physically here, and this function only writes the map, it cannot move
    them. Spreading such a table then means splitting its range and moving
    the buckets that changed hands, which is a deliberate operation, not a
    side effect of booting."""
    targets = _instance_targets(settings)
    async with session(None) as db_session:
        mapped = (
            await db_session.exec(select(ShardRange.table).distinct())
        ).all()
        already = set(mapped)

        for table in mapped_tables():
            if table in already:
                continue

            table_targets = targets
            if table in WHOLE_SPACE_TABLES:
                table_targets = targets[:1]
            elif len(targets) > 1:
                existing_rows = await db_session.scalar(
                    sa_select(func.count()).select_from(
                        SQLModel.metadata.tables[table]
                    )
                )
                if existing_rows:
                    _log.warning(
                        "Seeding all %d virtual shards of %s on the main database: "
                        "it already holds %d row(s), which are physically there. "
                        "Give %s a bucket range only by splitting one and moving "
                        "those rows.",
                        VIRTUAL_SHARDS,
                        table,
                        existing_rows,
                        ", ".join(key for key, _ in targets[1:]),
                    )
                    table_targets = targets[:1]

            for (partition_key, (host, port, database)), (start, end) in zip(
                table_targets, _even_ranges(len(table_targets))
            ):
                db_session.add(
                    ShardRange(
                        table=table,
                        start_bucket=start,
                        end_bucket=end,
                        partition_key=partition_key,
                        host=host,
                        port=port,
                        database=database,
                    )
                )
        await db_session.commit()
    await refresh_shard_map()


async def refresh_shard_map() -> None:
    """Reload every table's bucket-range map from the main database."""
    if _settings is None:
        raise RuntimeError("Database engine is not initialized")
    async with session(None) as db_session:
        ranges = (
            await db_session.exec(
                select(ShardRange).order_by(ShardRange.table, ShardRange.start_bucket)
            )
        ).all()

    seen: set[str] = set()
    rebuilt: dict[str, tuple[list[int], list[str | None]]] = {}
    for shard_range in ranges:
        starts, owners = rebuilt.setdefault(shard_range.table, ([], []))
        starts.append(shard_range.start_bucket)
        if shard_range.partition_key == MAIN_PARTITION_ROW_KEY:
            owners.append(None)  # the null partition is the main shard
            continue
        owners.append(shard_range.partition_key)
        seen.add(shard_range.partition_key)
        url = shard_url(
            _settings, shard_range.host, shard_range.port, shard_range.database
        )
        existing = _shards.get(shard_range.partition_key)
        if existing is None or existing.url != url:
            _shards[shard_range.partition_key] = _Shard(url)
    for stale in set(_shards) - seen:
        await _shards.pop(stale).dispose()

    _ranges.clear()
    _ranges.update(rebuilt)


async def _table_ranges(table: str) -> tuple[list[int], list[str | None]]:
    """One table's map, re-reading it once if this process hasn't seen it.
    A table added since the last refresh — or since this process booted —
    is the ordinary reason for a miss."""
    found = _ranges.get(table)
    if found is None:
        await refresh_shard_map()
        found = _ranges.get(table)
    if found is None:
        raise RuntimeError(
            f"No shard ranges for table {table!r}; the shard_ranges table on "
            "the main database has no map for it. A table with no foreign key "
            "needs one seeded (see seed_shard_ranges); a table with a foreign "
            "key should be addressed through its parent instead."
        )
    return found


async def partition_for_bucket(table: str, bucket: int) -> str | None:
    """The instance holding one of `table`'s virtual shards, from that
    table's rows in the ``shard_ranges`` map."""
    if not 0 <= bucket < VIRTUAL_SHARDS:
        raise ValueError(f"Virtual shard {bucket} out of range")
    starts, owners = await _table_ranges(table)
    index = bisect_right(starts, bucket) - 1
    if index < 0:
        # The map may have been extended since we last looked.
        await refresh_shard_map()
        starts, owners = await _table_ranges(table)
        index = bisect_right(starts, bucket) - 1
    if index < 0:
        raise RuntimeError(
            f"No shard range of {table!r} covers virtual shard {bucket}; its "
            "rows in the shard_ranges table on the main database do not tile "
            "the space"
        )
    return owners[index]


async def partition_for_key(table: str, key: str | uuid.UUID) -> str | None:
    """The instance holding `table`'s row for `key` — hash the key, read
    that table's map. This is the only way a partition key is obtained for
    a table that hashes its own key; a table with a foreign key is reached
    through its parent's key instead."""
    return await partition_for_bucket(table, virtual_shard(key))


async def partition_for_table(table: str) -> str | None:
    """The single instance holding *all* of `table` — for the tables whose
    map is one whole-space range (see :data:`WHOLE_SPACE_TABLES`), which is
    what makes a scan over non-key predicates answerable at all. Refuses a
    table that is spread, because for one there is no such instance and the
    caller has to name a key."""
    if table not in WHOLE_SPACE_TABLES:
        raise RuntimeError(
            f"{table!r} is not a whole-space table, so it has no single "
            "instance to name — look a row up by key with partition_for_key. "
            "(Refused on the declaration, not on how the map happens to be "
            "split today, so a table that is spread later doesn't silently "
            "start returning wrong answers.)"
        )
    _, owners = await _table_ranges(table)
    if len(owners) != 1:
        raise RuntimeError(
            f"{table!r} is declared whole-space but its map has "
            f"{len(owners)} ranges; the shard_ranges rows for it are wrong"
        )
    return owners[0]


async def partition_for_user(user_id: str | uuid.UUID) -> str | None:
    """The instance holding everything keyed by this user id — the `users`
    row itself and every table whose first foreign key leads back to it.
    Shorthand for ``partition_for_key("users", user_id)``, and named because
    it is by far the most common lookup in the codebase."""
    return await partition_for_key(User.__tablename__, user_id)


async def _shard_for(partition_key: str | None) -> _Shard:
    if _main is None:
        raise RuntimeError("Database engine is not initialized")
    if partition_key is None or partition_key == MAIN_PARTITION_ROW_KEY:
        return _main
    shard = _shards.get(partition_key)
    if shard is None:
        # Another instance may have added the partition since we last
        # looked; re-read the map once before giving up.
        await refresh_shard_map()
        shard = _shards.get(partition_key)
    if shard is None:
        raise UnknownPartitionError(partition_key)
    return shard


@asynccontextmanager
async def session(partition_key: str | None) -> AsyncIterator[AsyncSession]:
    """A session against the database holding `partition_key`'s rows. A null
    key means the main/first database."""
    shard = await _shard_for(partition_key)
    async with shard.sessions()() as db_session:
        yield db_session
