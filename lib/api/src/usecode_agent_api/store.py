"""Row storage, split across the database instances described in db.py.

**Every table here is partitioned; none is global.** A table with no
foreign key hashes its own key and reads its own map, so every lookup in
this module names the table it is resolving against:

- `db.partition_for_key(table, key)` — the phone number against
  `user_directory`'s or `otps`' map, the user id against `users`'.
- `db.partition_for_user(user_id)` — the `users` shorthand, used for the
  tables whose first foreign key leads back to a user (web sessions,
  provider credentials, servers, and the `user_api_keys` / `user_tasks`
  indexes). Those inherit their owner's instance and have no map of their
  own, which is exactly why they can carry real foreign keys to `users`.
- `db.partition_for_table(table)` — the catalog tables, whose map is one
  whole-space range because they are read by scans over non-key predicates
  rather than by their key.

A method touching a table with a foreign key either resolves the partition
from a user id or takes `partition_key` as its first argument with no
default: such a row is unreachable without it, which is the point. Callers
get the key from the authenticated user (`ApiKeyRecord.partition_key` /
`WebSessionRecord.partition_key`).

Two things are looked up by their own hash rather than by a user id, and
they are addressed differently because they arrive at different moments:

- An **API key** arrives before any user id exists to hash, so `api_keys`
  is a root table partitioned on the key hash — `db.partition_for_key`
  against its own map routes authentication to one instance from the token
  alone. `user_api_keys`, which does live with the user, is what turns a
  user id back into their key hashes for listing and revoking.
- A **web session token** is only ever handed out after the user is known,
  so `web_sessions` stays a child of `users` and the token is minted with
  the owner's virtual shard as a short hex prefix (`"1a2b.<secret>"`),
  which routes the cookie lookup to the owner's instance.

A **task** is a third case, hashed on neither a token nor a user id but on
its `assignee` — the API instance carrying it — because that is the query
that has to be cheap: each instance sweeps its own outstanding work, and
hashing the node name puts all of it on one database. Task methods here
take the assignee and resolve the partition themselves, the way the API
key ones take the token; `user_tasks` is the index that turns a user id
back into (task id, assignee).
"""

import hashlib
import secrets
import uuid
from dataclasses import dataclass
from datetime import datetime, timezone

from sqlalchemy.exc import IntegrityError
from sqlmodel import select

from . import db
from .db_models import (
    ApiKey,
    LocationMapping,
    Otp,
    ProviderCredential,
    ProviderResource,
    Server,
    ServerTypeMapping,
    Task,
    User,
    UserApiKey,
    UserDirectory,
    UserTask,
    WebSession,
)


def _hash(value: str) -> str:
    return hashlib.sha256(value.encode()).hexdigest()


def _ts(value: datetime) -> float:
    return value.replace(tzinfo=value.tzinfo or timezone.utc).timestamp()


def _uuid(value: str) -> uuid.UUID:
    return uuid.UUID(value)


# -- Bearer tokens ------------------------------------------------------


async def _api_key_partition(api_key: str) -> str | None:
    """The instance holding this API key's row: hash the token, hash that
    against the `api_keys` map. Nothing else is needed — that is what makes
    `api_keys` a root table rather than a child of `users`."""
    return await db.partition_for_key(ApiKey.__tablename__, _hash(api_key))


async def _task_partition(assignee: str) -> str | None:
    """The instance holding a task: hash the node name carrying it against
    the `tasks` map. See `Task.__placement__` for why that is the address."""
    return await db.partition_for_key(Task.__tablename__, assignee)


# A web session token is looked up by its own hash, but its row is a child
# of `users`, so on its own the token says nothing about where that row is.
# It is therefore minted with the owner's virtual shard in front —
# "1a2b.<secret>" — which routes the lookup to exactly one instance. The
# hash stored in the database covers the whole token, prefix included.

_TOKEN_PREFIX_WIDTH = 4  # hex digits, enough for db.VIRTUAL_SHARDS - 1


def _mint_session_token(user_id: str) -> str:
    bucket = db.virtual_shard(user_id)
    return f"{bucket:0{_TOKEN_PREFIX_WIDTH}x}.{secrets.token_urlsafe(32)}"


# A deployment old enough to still hold unprefixed tokens never had more
# than one instance (see db.seed_shard_ranges), so the main database — the
# null partition key — is the only place such a row can be.
_LEGACY_TOKEN_PARTITION: str | None = None


async def _session_partition(token: str) -> str | None:
    """The instance holding the `web_sessions` row for this token. The
    prefix is a bucket of the *owner's* `users` map, since that is what the
    row's placement follows. Tokens minted before the prefix existed have no
    separator at all (`secrets.token_urlsafe` never produces a ".")."""
    prefix, separator, secret = token.partition(".")
    if not separator or not secret or len(prefix) != _TOKEN_PREFIX_WIDTH:
        return _LEGACY_TOKEN_PARTITION
    try:
        bucket = int(prefix, 16)
    except ValueError:
        return _LEGACY_TOKEN_PARTITION
    return await db.partition_for_bucket(User.__tablename__, bucket)


@dataclass
class OtpRecord:
    code: str
    expires_at: float
    resend_after: float
    attempts: int = 0


@dataclass
class ApiKeyRecord:
    # The key's hash, which is both its identity and its address: it is what
    # routes to the instance holding it, and what the API exposes as the
    # key's id. The token itself is returned exactly once, at issue time.
    id: str
    user_id: str
    phone: str
    # The database instance holding this *user's* partitioned rows; passed
    # to every partitioned store call made on their behalf. Not where the
    # api_keys row itself is — that one hashes on the key hash.
    partition_key: str | None
    label: str
    created_at: float
    last_used_at: float | None


def _api_key_record(
    key: ApiKey, phone: str, user_partition: str | None
) -> ApiKeyRecord:
    return ApiKeyRecord(
        id=key.key_hash,
        user_id=str(key.user_id),
        phone=phone,
        partition_key=user_partition,
        label=key.label,
        created_at=_ts(key.created_at),
        last_used_at=_ts(key.last_used_at) if key.last_used_at else None,
    )


@dataclass
class ServerRecord:
    id: str
    user_id: str
    provider: str
    provider_server_id: str
    type: str
    name: str
    status: str
    public_ip4: str | None
    public_ip6: str | None
    created_at: float
    updated_at: float


@dataclass
class TaskRecord:
    id: str
    user_id: str
    kind: str
    # Node name of the API instance that owns this task, and half of the
    # row's address: `tasks` is partitioned on it.
    assignee: str
    state: str
    resources: list
    payload: dict
    error: str | None
    created_at: float
    updated_at: float


@dataclass
class WebSessionRecord:
    user_id: str
    phone: str
    partition_key: str | None
    api_key_hash: str
    created_at: float


class Store:
    """SQLModel-backed storage over the sharded databases (see db.py)."""

    # -- OTP (partitioned on the phone number) ----------------------------

    async def put_otp(self, phone: str, record: OtpRecord) -> None:
        partition_key = await db.partition_for_key(Otp.__tablename__, phone)
        async with db.session(partition_key) as session:
            otp = await session.get(Otp, phone)
            if otp is None:
                otp = Otp(phone=phone)
            otp.code_hash = _hash(record.code)
            otp.expires_at = datetime.fromtimestamp(record.expires_at, tz=timezone.utc)
            otp.resend_after = datetime.fromtimestamp(
                record.resend_after, tz=timezone.utc
            )
            otp.attempts = 0
            session.add(otp)
            await session.commit()

    async def get_otp(self, phone: str) -> OtpRecord | None:
        partition_key = await db.partition_for_key(Otp.__tablename__, phone)
        async with db.session(partition_key) as session:
            otp = await session.get(Otp, phone)
            if otp is None:
                return None
            return OtpRecord(
                code=otp.code_hash,
                expires_at=_ts(otp.expires_at),
                resend_after=_ts(otp.resend_after),
                attempts=otp.attempts,
            )

    async def check_otp_code(self, phone: str, code: str) -> bool:
        partition_key = await db.partition_for_key(Otp.__tablename__, phone)
        async with db.session(partition_key) as session:
            otp = await session.get(Otp, phone)
            if otp is None:
                return False
            return secrets.compare_digest(otp.code_hash, _hash(code))

    async def increment_attempts(self, phone: str) -> int:
        partition_key = await db.partition_for_key(Otp.__tablename__, phone)
        async with db.session(partition_key) as session:
            otp = await session.get(Otp, phone)
            if otp is None:
                return 0
            otp.attempts += 1
            session.add(otp)
            await session.commit()
            return otp.attempts

    async def clear_otp(self, phone: str) -> None:
        partition_key = await db.partition_for_key(Otp.__tablename__, phone)
        async with db.session(partition_key) as session:
            otp = await session.get(Otp, phone)
            if otp is not None:
                await session.delete(otp)
                await session.commit()

    # -- Users (partitioned on the user id, found via the directory) -------

    async def get_or_create_user(self, phone: str) -> tuple[str, str | None]:
        """Returns (user_id, partition_key).

        Two maps are read here, which is the whole point of naming the table
        on every lookup: the phone number resolves against
        `user_directory`'s map, and the user id it yields resolves against
        `users`'. The two are unrelated — a phone and the user it names are
        not co-located — so the directory entry is written on one instance
        and the user row on another. Everything after this is recomputed
        from the id, so nothing is looked up twice, and adding an instance
        can never make an existing user unreachable (its bucket only moves
        if the rows are moved with it)."""
        directory_partition = await db.partition_for_key(
            UserDirectory.__tablename__, phone
        )
        async with db.session(directory_partition) as session:
            entry = await session.get(UserDirectory, phone)
        if entry is not None:
            return str(entry.user_id), await db.partition_for_user(entry.user_id)

        # Write the user row before the directory entry: a crash in between
        # leaves an unreferenced user row, which is inert, whereas the other
        # order would leave the directory pointing at a user that does not
        # exist and lock the phone number out for good.
        user = User(phone=phone)
        user_partition = await db.partition_for_user(user.id)
        async with db.session(user_partition) as session:
            session.add(user)
            await session.commit()

        async with db.session(directory_partition) as session:
            session.add(UserDirectory(phone=phone, user_id=user.id))
            try:
                await session.commit()
            except IntegrityError:
                # Another request registered this phone number first; theirs
                # is the user that exists as far as everyone else is
                # concerned, so drop ours rather than leaving two.
                await session.rollback()
                winner = await session.get(UserDirectory, phone)
                assert winner is not None
                await self._delete_user(user_partition, user.id)
                return str(winner.user_id), await db.partition_for_user(winner.user_id)
        return str(user.id), user_partition

    async def _delete_user(self, partition_key: str | None, user_id: uuid.UUID) -> None:
        async with db.session(partition_key) as session:
            user = await session.get(User, user_id)
            if user is not None:
                await session.delete(user)
                await session.commit()

    async def get_user_partition(self, user_id: str) -> str | None:
        """Where this user's rows are, from the id alone — no lookup."""
        return await db.partition_for_user(user_id)

    # -- API keys (partitioned on the key hash) ---------------------------
    # An arriving request has the token and nothing else, so the token's
    # hash is the whole address: `api_keys` is a root table with its own
    # map. The reverse direction — a user's own keys — goes through the
    # `user_api_keys` index, which lives with the user.

    async def issue_api_key(self, user_id: str, label: str = "") -> str:
        api_key = secrets.token_urlsafe(32)
        key_hash = _hash(api_key)
        async with db.session(
            await db.partition_for_key(ApiKey.__tablename__, key_hash)
        ) as session:
            session.add(ApiKey(key_hash=key_hash, user_id=_uuid(user_id), label=label))
            await session.commit()

        # Index second: a crash in between leaves a working key that does
        # not show up in listings, whereas the other order would list a key
        # that cannot be authenticated with and cannot be revoked either.
        async with db.session(await db.partition_for_user(user_id)) as session:
            session.add(UserApiKey(user_id=_uuid(user_id), key_hash=key_hash))
            await session.commit()
        return api_key

    async def get_api_key(self, api_key: str) -> ApiKeyRecord | None:
        key_hash = _hash(api_key)
        async with db.session(await _api_key_partition(api_key)) as session:
            key = await session.get(ApiKey, key_hash)
            if key is None or key.revoked_at is not None:
                return None
            key.last_used_at = datetime.now(timezone.utc)
            session.add(key)
            await session.commit()

        # The owner is on their own instance, which is also the one every
        # call made on their behalf will use.
        user_partition = await db.partition_for_user(key.user_id)
        async with db.session(user_partition) as session:
            user = await session.get(User, key.user_id)
        if user is None:
            return None
        return _api_key_record(key, user.phone, user_partition)

    async def list_api_keys(self, user_id: str) -> list[ApiKeyRecord]:
        """This user's live keys, read through the `user_api_keys` index.

        The index says *which* keys exist; each key's own row — label, last
        use, whether it is revoked — is on the instance its hash resolves
        to, so the hashes are grouped by instance and fetched one query
        per instance rather than one per key."""
        user_partition = await db.partition_for_user(user_id)
        user_uuid = _uuid(user_id)
        async with db.session(user_partition) as session:
            hashes = (
                await session.exec(
                    select(UserApiKey.key_hash).where(UserApiKey.user_id == user_uuid)
                )
            ).all()
            user = await session.get(User, user_uuid)
        if user is None:
            return []

        by_partition: dict[str | None, list[str]] = {}
        for key_hash in hashes:
            partition_key = await db.partition_for_key(ApiKey.__tablename__, key_hash)
            by_partition.setdefault(partition_key, []).append(key_hash)

        records: list[ApiKeyRecord] = []
        for partition_key, partition_hashes in by_partition.items():
            async with db.session(partition_key) as session:
                keys = (
                    await session.exec(
                        select(ApiKey).where(
                            ApiKey.key_hash.in_(partition_hashes),
                            ApiKey.user_id == user_uuid,
                            ApiKey.revoked_at == None,  # noqa: E711
                        )
                    )
                ).all()
            records.extend(
                _api_key_record(key, user.phone, user_partition) for key in keys
            )
        records.sort(key=lambda record: record.created_at, reverse=True)
        return records

    async def revoke_api_key(self, api_key: str) -> None:
        async with db.session(await _api_key_partition(api_key)) as session:
            key = await session.get(ApiKey, _hash(api_key))
            if key is not None and key.revoked_at is None:
                key.revoked_at = datetime.now(timezone.utc)
                session.add(key)
                await session.commit()

    async def revoke_api_key_for_user(self, user_id: str, key_hash: str) -> bool:
        """Revoke one of a user's keys by its hash — the id the API hands
        out. The index row is what proves the key is theirs; without it a
        caller could revoke a hash belonging to someone else, since the
        `api_keys` row itself is on an instance that knows nothing about who
        is asking."""
        try:
            user_uuid = _uuid(user_id)
        except ValueError:
            return False
        async with db.session(await db.partition_for_user(user_uuid)) as session:
            owned = await session.get(UserApiKey, (user_uuid, key_hash))
        if owned is None:
            return False

        async with db.session(
            await db.partition_for_key(ApiKey.__tablename__, key_hash)
        ) as session:
            key = await session.get(ApiKey, key_hash)
            if key is None or key.revoked_at is not None:
                return False
            key.revoked_at = datetime.now(timezone.utc)
            session.add(key)
            await session.commit()
            return True

    # -- Web sessions (partitioned, routed by the token's shard prefix) ---

    async def issue_web_session(self, user_id: str, api_key_hash: str) -> str:
        session_token = _mint_session_token(user_id)
        async with db.session(await db.partition_for_user(user_id)) as session:
            session.add(
                WebSession(
                    token_hash=_hash(session_token),
                    user_id=_uuid(user_id),
                    api_key_hash=api_key_hash,
                )
            )
            await session.commit()
        return session_token

    async def get_web_session(self, session_token: str) -> WebSessionRecord | None:
        partition_key = await _session_partition(session_token)
        async with db.session(partition_key) as session:
            web_session = await session.get(WebSession, _hash(session_token))
            if web_session is None or web_session.revoked_at is not None:
                return None
            user = await session.get(User, web_session.user_id)
            assert user is not None
            return WebSessionRecord(
                user_id=str(web_session.user_id),
                phone=user.phone,
                partition_key=partition_key,
                api_key_hash=web_session.api_key_hash,
                created_at=_ts(web_session.created_at),
            )

    async def revoke_web_session(self, session_token: str) -> None:
        async with db.session(await _session_partition(session_token)) as session:
            web_session = await session.get(WebSession, _hash(session_token))
            if web_session is not None:
                web_session.revoked_at = datetime.now(timezone.utc)
                session.add(web_session)
                await session.commit()

    # -- Provider credentials (partitioned) -------------------------------
    # A user's provider secrets live with the rest of their rows, on the
    # instance their user id hashes to — which is also where the `users` row
    # the foreign key points at is.

    async def set_provider_credentials(
        self,
        partition_key: str | None,
        user_id: str,
        provider: str,
        credentials_encrypted: str,
    ) -> None:
        async with db.session(partition_key) as session:
            credential = await session.get(
                ProviderCredential, (_uuid(user_id), provider)
            )
            if credential is None:
                credential = ProviderCredential(
                    user_id=_uuid(user_id), provider=provider
                )
            credential.credentials_encrypted = credentials_encrypted
            credential.updated_at = datetime.now(timezone.utc)
            session.add(credential)
            await session.commit()

    async def get_provider_credentials(
        self, partition_key: str | None, user_id: str, provider: str
    ) -> str | None:
        async with db.session(partition_key) as session:
            credential = await session.get(
                ProviderCredential, (_uuid(user_id), provider)
            )
            return credential.credentials_encrypted if credential else None

    async def delete_provider_credentials(
        self, partition_key: str | None, user_id: str, provider: str
    ) -> bool:
        async with db.session(partition_key) as session:
            credential = await session.get(
                ProviderCredential, (_uuid(user_id), provider)
            )
            if credential is None:
                return False
            await session.delete(credential)
            await session.commit()
            return True

    async def list_configured_providers(
        self, partition_key: str | None, user_id: str
    ) -> list[str]:
        async with db.session(partition_key) as session:
            credentials = (
                await session.exec(
                    select(ProviderCredential).where(
                        ProviderCredential.user_id == _uuid(user_id)
                    )
                )
            ).all()
            return [credential.provider for credential in credentials]

    # -- Server type mappings (one whole-space range; see db.py) ----------

    async def get_server_type_mapping(self, series: str) -> ServerTypeMapping | None:
        partition_key = await db.partition_for_table(ServerTypeMapping.__tablename__)
        async with db.session(partition_key) as session:
            return await session.get(ServerTypeMapping, series)

    # Prefix used when a provider's server type doesn't have a mapping yet
    # and one needs to be minted on the fly, e.g. "x9" or "y9".
    _SERIES_PREFIXES = {"hetzner": "x", "digitalocean": "y"}

    async def get_or_create_series_for_provider_type(
        self, provider: str, provider_server_type: str, cities: list[str] | None = None
    ) -> str:
        """Look up the series for a provider's raw server type, minting a
        new one (and persisting it) if this type has never been seen
        before. Guarantees every server type we ever expose is one of our
        own "{series}-{city}" identifiers, never the provider's own name.

        `cities` (our own city codes this type is currently known to be
        available in) is merged into the series' stored city list — added
        to, never removed from, so a provider momentarily omitting a city
        from one response doesn't erase it."""
        partition_key = await db.partition_for_table(ServerTypeMapping.__tablename__)
        async with db.session(partition_key) as session:
            mapping = (
                await session.exec(
                    select(ServerTypeMapping).where(
                        ServerTypeMapping.provider == provider,
                        ServerTypeMapping.provider_server_type == provider_server_type,
                    )
                )
            ).first()
            if mapping is not None:
                if cities:
                    merged = sorted(set(mapping.cities) | set(cities))
                    if merged != mapping.cities:
                        mapping.cities = merged
                        session.add(mapping)
                        await session.commit()
                return mapping.series

            prefix = self._SERIES_PREFIXES.get(provider, provider[:1])
            existing = (
                await session.exec(
                    select(ServerTypeMapping.series).where(
                        ServerTypeMapping.provider == provider
                    )
                )
            ).all()
            used = [
                int(series[len(prefix) :])
                for series in existing
                if series.startswith(prefix) and series[len(prefix) :].isdigit()
            ]
            series = f"{prefix}{max(used, default=0) + 1}"
            session.add(
                ServerTypeMapping(
                    series=series,
                    provider=provider,
                    provider_server_type=provider_server_type,
                    cities=sorted(set(cities or [])),
                )
            )
            await session.commit()
            return series

    async def get_server_type_cities(self, series: str) -> list[str]:
        mapping = await self.get_server_type_mapping(series)
        return mapping.cities if mapping is not None else []

    # -- Location mappings (one whole-space range; see db.py) -------------

    async def get_location_mapping(
        self, code: str, provider: str
    ) -> LocationMapping | None:
        partition_key = await db.partition_for_table(LocationMapping.__tablename__)
        async with db.session(partition_key) as session:
            return await session.get(LocationMapping, (code, provider))

    async def set_location_mapping(
        self, code: str, provider: str, provider_location_code: str
    ) -> None:
        """Record our (code, provider) -> provider location mapping if it
        isn't already known. Never overwrites an existing mapping, so once
        minted it's fixed even if the provider's own data shifts later."""
        partition_key = await db.partition_for_table(LocationMapping.__tablename__)
        async with db.session(partition_key) as session:
            existing = await session.get(LocationMapping, (code, provider))
            if existing is not None:
                return
            session.add(
                LocationMapping(
                    code=code,
                    provider=provider,
                    provider_location_code=provider_location_code,
                )
            )
            await session.commit()

    # -- Provider catalog (one whole-space range; see db.py) ----------------
    # Raw locations/server-types/images mirrored from each provider by
    # POST /servers/sync, used to answer "what's available" without hitting
    # the provider's API live every time.

    async def upsert_provider_resource(
        self, provider: str, kind: str, code: str, data: dict
    ) -> None:
        partition_key = await db.partition_for_table(ProviderResource.__tablename__)
        async with db.session(partition_key) as session:
            resource = (
                await session.exec(
                    select(ProviderResource).where(
                        ProviderResource.provider == provider,
                        ProviderResource.kind == kind,
                        ProviderResource.code == code,
                    )
                )
            ).first()
            if resource is None:
                resource = ProviderResource(provider=provider, kind=kind, code=code)
            resource.data = data
            resource.updated_at = datetime.now(timezone.utc)
            session.add(resource)
            await session.commit()

    async def list_provider_resources(
        self, provider: str | None = None, kind: str | None = None
    ) -> list[ProviderResource]:
        partition_key = await db.partition_for_table(ProviderResource.__tablename__)
        async with db.session(partition_key) as session:
            query = select(ProviderResource)
            if provider is not None:
                query = query.where(ProviderResource.provider == provider)
            if kind is not None:
                query = query.where(ProviderResource.kind == kind)
            return (await session.exec(query.order_by(ProviderResource.code))).all()

    # -- Servers (partitioned) ---------------------------------------------
    # A server row lives on its owner's instance, so nothing here can be
    # reached without the owner's partition key.

    @staticmethod
    def _server_record(server: Server) -> "ServerRecord":
        return ServerRecord(
            id=str(server.id),
            user_id=str(server.user_id),
            provider=server.provider,
            provider_server_id=server.provider_server_id,
            type=server.type,
            name=server.name,
            status=server.status,
            public_ip4=server.public_ip4,
            public_ip6=server.public_ip6,
            created_at=_ts(server.created_at),
            updated_at=_ts(server.updated_at),
        )

    async def create_server(
        self,
        partition_key: str | None,
        user_id: str,
        provider: str,
        provider_server_id: str,
        type: str,
        name: str,
        status: str,
        public_ip4: str | None,
        public_ip6: str | None,
    ) -> ServerRecord:
        async with db.session(partition_key) as session:
            server = Server(
                user_id=_uuid(user_id),
                provider=provider,
                provider_server_id=provider_server_id,
                type=type,
                name=name,
                status=status,
                public_ip4=public_ip4,
                public_ip6=public_ip6,
            )
            session.add(server)
            await session.commit()
            await session.refresh(server)
            return self._server_record(server)

    async def get_server(
        self, partition_key: str | None, user_id: str, server_id: str
    ) -> ServerRecord | None:
        try:
            server_uuid, user_uuid = _uuid(server_id), _uuid(user_id)
        except ValueError:
            return None
        async with db.session(partition_key) as session:
            server = (
                await session.exec(
                    select(Server).where(
                        Server.id == server_uuid, Server.user_id == user_uuid
                    )
                )
            ).first()
            return self._server_record(server) if server else None

    async def list_servers(
        self, partition_key: str | None, user_id: str
    ) -> list[ServerRecord]:
        async with db.session(partition_key) as session:
            servers = (
                await session.exec(
                    select(Server)
                    .where(Server.user_id == _uuid(user_id))
                    .order_by(Server.created_at.desc())
                )
            ).all()
            return [self._server_record(server) for server in servers]

    async def delete_server(
        self, partition_key: str | None, user_id: str, server_id: str
    ) -> ServerRecord | None:
        try:
            server_uuid, user_uuid = _uuid(server_id), _uuid(user_id)
        except ValueError:
            return None
        async with db.session(partition_key) as session:
            server = (
                await session.exec(
                    select(Server).where(
                        Server.id == server_uuid, Server.user_id == user_uuid
                    )
                )
            ).first()
            if server is None:
                return None
            record = self._server_record(server)
            await session.delete(server)
            await session.commit()
            return record

    async def set_server_status(
        self, partition_key: str | None, user_id: str, server_id: str, status: str
    ) -> ServerRecord | None:
        try:
            server_uuid, user_uuid = _uuid(server_id), _uuid(user_id)
        except ValueError:
            return None
        async with db.session(partition_key) as session:
            server = (
                await session.exec(
                    select(Server).where(
                        Server.id == server_uuid, Server.user_id == user_uuid
                    )
                )
            ).first()
            if server is None:
                return None
            server.status = status
            server.updated_at = datetime.now(timezone.utc)
            session.add(server)
            await session.commit()
            await session.refresh(server)
            return self._server_record(server)

    async def upsert_server_by_provider_id(
        self,
        partition_key: str | None,
        user_id: str,
        provider: str,
        provider_server_id: str,
        type: str,
        name: str,
        status: str,
        public_ip4: str | None,
        public_ip6: str | None,
    ) -> tuple[ServerRecord, bool]:
        """Insert or update a server matched by (provider, provider_server_id).
        Returns (record, created)."""
        async with db.session(partition_key) as session:
            server = (
                await session.exec(
                    select(Server).where(
                        Server.user_id == _uuid(user_id),
                        Server.provider == provider,
                        Server.provider_server_id == provider_server_id,
                    )
                )
            ).first()
            created = server is None
            if server is None:
                server = Server(
                    user_id=_uuid(user_id),
                    provider=provider,
                    provider_server_id=provider_server_id,
                )
            server.type = type
            server.name = name
            server.status = status
            server.public_ip4 = public_ip4
            server.public_ip6 = public_ip6
            server.updated_at = datetime.now(timezone.utc)
            session.add(server)
            await session.commit()
            await session.refresh(server)
            return self._server_record(server), created

    # -- Tasks (partitioned on the assignee) --------------------------------
    # A task is a suspended, resumable workflow acting on provider-owned
    # resources. See AGENTS.md ("Provider resources and tasks"). It is a
    # root table hashed on `assignee`, the name of the API instance that
    # picked it up: only the assignee advances a task, and it does so by
    # sweeping — so "this node's tasks" is the read that decides placement,
    # and it lands on one instance. The methods below take the assignee for
    # that reason, and resolve the partition themselves.
    #
    # A task therefore does *not* sit with the user rows its steps mutate;
    # those are reached through `db.partition_for_user(task.user_id)`. The
    # reverse question — a user's own in-flight tasks — goes through the
    # `user_tasks` index, which does live with the user.

    @staticmethod
    def _task_record(task: Task) -> "TaskRecord":
        return TaskRecord(
            id=str(task.id),
            user_id=str(task.user_id),
            kind=task.kind,
            assignee=task.assignee,
            state=task.state,
            resources=task.resources,
            payload=task.payload,
            error=task.error,
            created_at=_ts(task.created_at),
            updated_at=_ts(task.updated_at),
        )

    async def create_task(
        self,
        user_id: str,
        kind: str,
        assignee: str,
        state: str,
        resources: list,
        payload: dict,
    ) -> TaskRecord:
        """Create a task owned by `assignee`, on the instance that node's
        name hashes to, and index it under its user."""
        async with db.session(await _task_partition(assignee)) as session:
            task = Task(
                user_id=_uuid(user_id),
                kind=kind,
                assignee=assignee,
                state=state,
                resources=resources,
                payload=payload,
            )
            session.add(task)
            await session.commit()
            await session.refresh(task)
            record = self._task_record(task)

        # Index second, as for API keys: a crash in between leaves a task
        # that still runs to completion but doesn't show up in GET /tasks,
        # whereas the other order would list a task that does not exist.
        async with db.session(await db.partition_for_user(user_id)) as session:
            session.add(
                UserTask(
                    user_id=_uuid(user_id),
                    task_id=_uuid(record.id),
                    assignee=assignee,
                )
            )
            await session.commit()
        return record

    async def get_task(self, assignee: str, task_id: str) -> TaskRecord | None:
        """One task, addressed the only way a task can be: by the node
        holding it and its id."""
        try:
            task_uuid = _uuid(task_id)
        except ValueError:
            return None
        async with db.session(await _task_partition(assignee)) as session:
            task = await session.get(Task, {"assignee": assignee, "id": task_uuid})
            return self._task_record(task) if task else None

    async def list_tasks(self, assignee: str) -> list[TaskRecord]:
        """Everything `assignee` is carrying — what the sweep runs on. One
        query on one instance, since that is what the table is hashed for;
        an instance must never advance a step another instance owns."""
        async with db.session(await _task_partition(assignee)) as session:
            tasks = (
                await session.exec(
                    select(Task)
                    .where(Task.assignee == assignee)
                    .order_by(Task.created_at)
                )
            ).all()
            return [self._task_record(task) for task in tasks]

    async def list_user_tasks(self, user_id: str) -> list[TaskRecord]:
        """This user's in-flight tasks, read through the `user_tasks`
        index (what GET /tasks does).

        The index says which tasks exist and which node holds each one; the
        rows themselves are on the instances those node names hash to, so
        they are grouped by instance and fetched one query per instance
        rather than one per task."""
        user_uuid = _uuid(user_id)
        async with db.session(await db.partition_for_user(user_id)) as session:
            indexed = (
                await session.exec(
                    select(UserTask).where(UserTask.user_id == user_uuid)
                )
            ).all()

        by_assignee: dict[str, list[uuid.UUID]] = {}
        for entry in indexed:
            by_assignee.setdefault(entry.assignee, []).append(entry.task_id)

        records: list[TaskRecord] = []
        for assignee, task_ids in by_assignee.items():
            async with db.session(await _task_partition(assignee)) as session:
                tasks = (
                    await session.exec(
                        select(Task).where(
                            Task.assignee == assignee,
                            Task.id.in_(task_ids),
                            Task.user_id == user_uuid,
                        )
                    )
                ).all()
            records.extend(self._task_record(task) for task in tasks)
        records.sort(key=lambda record: record.created_at)
        return records

    async def get_user_task(self, user_id: str, task_id: str) -> TaskRecord | None:
        """One of this user's tasks by id. The index row is what proves the
        task is theirs *and* what says which node to read it from — a task
        id alone addresses nothing."""
        try:
            user_uuid, task_uuid = _uuid(user_id), _uuid(task_id)
        except ValueError:
            return None
        async with db.session(await db.partition_for_user(user_id)) as session:
            indexed = await session.get(UserTask, (user_uuid, task_uuid))
        if indexed is None:
            return None
        task = await self.get_task(indexed.assignee, task_id)
        return task if task is not None and task.user_id == user_id else None

    async def set_task_state(
        self,
        assignee: str,
        task_id: str,
        state: str,
        payload: dict | None = None,
        error: str | None = None,
    ) -> None:
        async with db.session(await _task_partition(assignee)) as session:
            task = await session.get(
                Task, {"assignee": assignee, "id": _uuid(task_id)}
            )
            if task is None:
                return
            task.state = state
            if payload is not None:
                task.payload = payload
            task.error = error
            task.updated_at = datetime.now(timezone.utc)
            session.add(task)
            await session.commit()

    async def delete_task(self, assignee: str, task_id: str) -> None:
        """Remove a finished task and its index entry. The index goes
        first: an entry pointing at a task that is gone would be reported
        as in-flight forever, while a task with no entry is merely invisible
        to GET /tasks for the moment it takes to delete it."""
        task = await self.get_task(assignee, task_id)
        if task is None:
            return
        async with db.session(await db.partition_for_user(task.user_id)) as session:
            indexed = await session.get(
                UserTask, (_uuid(task.user_id), _uuid(task_id))
            )
            if indexed is not None:
                await session.delete(indexed)
                await session.commit()

        async with db.session(await _task_partition(assignee)) as session:
            row = await session.get(
                Task, {"assignee": assignee, "id": _uuid(task_id)}
            )
            if row is not None:
                await session.delete(row)
                await session.commit()

store = Store()
