"""api_keys hash their own key

An API key arrives with nothing else: the request that carries it has no
user id yet — the key is what produces one — so the key hash is the only
address it can have. `api_keys` therefore stops inheriting the owner's
placement and becomes a root table partitioned on `key_hash`, which
`db.seed_shard_ranges` gives a map of its own on the next boot (it is
derived from the schema: the table now has no foreign key).

Two references become plain columns, because the row each names is now on
an unrelated instance and no database can enforce them: `api_keys.user_id`,
and `web_sessions.api_key_id` — which becomes `api_key_hash`, since the
row's identity is its hash now that the surrogate `id` is gone.

`user_api_keys` is the index that answers the reverse question. A user id
predicts nothing about where their keys hashed to, so listing and revoking
would otherwise have to scan every instance; this table lives with the
`users` row (real foreign key, cascading) and records which hashes belong
to it.

Existing keys are still co-located with their owners, which is why the
index can be filled from `api_keys` on this same instance. Their rows are
left where they are: the `api_keys` map is seeded whole-space on the main
database in a deployment that already holds rows there, and spreading it
later means moving the buckets that change hands.

Revision ID: 0011_api_keys_own_shard
Revises: 0010_shard_ranges_per_table
Create Date: 2026-08-24

"""
from typing import Sequence, Union

import sqlalchemy as sa
import sqlmodel
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "0011_api_keys_own_shard"
down_revision: Union[str, Sequence[str], None] = "0010_shard_ranges_per_table"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    # -- web_sessions: point at the key by its hash ----------------------
    op.add_column(
        "web_sessions",
        sa.Column("api_key_hash", sqlmodel.sql.sqltypes.AutoString(), nullable=True),
    )
    op.execute(
        "UPDATE web_sessions SET api_key_hash = api_keys.key_hash "
        "FROM api_keys WHERE api_keys.id = web_sessions.api_key_id"
    )
    # A session whose key row is missing could not be logged out of anyway.
    op.execute("DELETE FROM web_sessions WHERE api_key_hash IS NULL")
    op.alter_column("web_sessions", "api_key_hash", nullable=False)
    op.execute(
        "ALTER TABLE web_sessions DROP CONSTRAINT IF EXISTS web_sessions_api_key_id_fkey"
    )
    op.drop_column("web_sessions", "api_key_id")

    # -- the owner's index of their keys ---------------------------------
    op.create_table(
        "user_api_keys",
        sa.Column("user_id", sa.Uuid(), nullable=False),
        sa.Column("key_hash", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.ForeignKeyConstraint(["user_id"], ["users.id"], ondelete="CASCADE"),
        sa.PrimaryKeyConstraint("user_id", "key_hash"),
    )
    op.execute(
        "INSERT INTO user_api_keys (user_id, key_hash, created_at) "
        "SELECT user_id, key_hash, created_at FROM api_keys "
        "ON CONFLICT DO NOTHING"
    )

    # -- api_keys: keyed and placed by the hash ---------------------------
    op.execute("ALTER TABLE api_keys DROP CONSTRAINT IF EXISTS api_keys_user_id_fkey")
    op.execute("ALTER TABLE api_keys DROP CONSTRAINT IF EXISTS api_keys_key_hash_key")
    op.drop_constraint("api_keys_pkey", "api_keys", type_="primary")
    op.create_primary_key("api_keys_pkey", "api_keys", ["key_hash"])
    op.drop_column("api_keys", "id")


def downgrade() -> None:
    op.add_column(
        "api_keys",
        sa.Column(
            "id", sa.Uuid(), nullable=False, server_default=sa.text("gen_random_uuid()")
        ),
    )
    op.alter_column("api_keys", "id", server_default=None)
    op.drop_constraint("api_keys_pkey", "api_keys", type_="primary")
    op.create_primary_key("api_keys_pkey", "api_keys", ["id"])
    op.create_unique_constraint("api_keys_key_hash_key", "api_keys", ["key_hash"])
    op.create_foreign_key(
        "api_keys_user_id_fkey",
        "api_keys",
        "users",
        ["user_id"],
        ["id"],
        ondelete="CASCADE",
    )

    op.add_column("web_sessions", sa.Column("api_key_id", sa.Uuid(), nullable=True))
    op.execute(
        "UPDATE web_sessions SET api_key_id = api_keys.id "
        "FROM api_keys WHERE api_keys.key_hash = web_sessions.api_key_hash"
    )
    op.execute("DELETE FROM web_sessions WHERE api_key_id IS NULL")
    op.alter_column("web_sessions", "api_key_id", nullable=False)
    op.create_foreign_key(
        "web_sessions_api_key_id_fkey",
        "web_sessions",
        "api_keys",
        ["api_key_id"],
        ["id"],
        ondelete="CASCADE",
    )
    op.drop_column("web_sessions", "api_key_hash")

    op.drop_table("user_api_keys")

    # The map only ever addressed a table that no longer hashes its own key.
    op.execute("DELETE FROM shard_ranges WHERE \"table\" = 'api_keys'")
