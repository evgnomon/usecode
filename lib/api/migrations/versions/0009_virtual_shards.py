"""virtual shards: user-id placement, user directory, real user foreign keys

Replaces the stored-partition-key scheme of 0008 with a computed one. A
user id hashes into one of 65536 virtual shards; `shard_ranges` maps a
contiguous run of those buckets to the instance holding them. Because every
user-owned table is keyed by the user id, they all hash to the same
instance as the `users` row — so the foreign keys 0008 had to drop come
back as real constraints. See db.py.

`user_directory` is the one remaining lookup that isn't by user id: a phone
number arriving at the login endpoint has to turn into a user id before
anything can be placed.

Revision ID: 0009_virtual_shards
Revises: 0008_horizontal_scaling
Create Date: 2026-08-24

"""
from typing import Sequence, Union

import sqlalchemy as sa
import sqlmodel
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "0009_virtual_shards"
down_revision: Union[str, Sequence[str], None] = "0008_horizontal_scaling"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None

# Tables keyed by a user id, and therefore co-located with their owner.
_USER_OWNED_TABLES = ("provider_credentials", "servers", "tasks")


def upgrade() -> None:
    op.create_table(
        "shard_ranges",
        sa.Column("start_bucket", sa.Integer(), nullable=False),
        sa.Column("end_bucket", sa.Integer(), nullable=False),
        sa.Column("partition_key", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("host", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("port", sa.Integer(), nullable=False),
        sa.Column("database", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.PrimaryKeyConstraint("start_bucket"),
    )

    op.create_table(
        "user_directory",
        sa.Column("phone", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("user_id", sa.Uuid(), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.PrimaryKeyConstraint("phone"),
    )
    op.create_index(
        op.f("ix_user_directory_user_id"), "user_directory", ["user_id"], unique=False
    )

    # Every user that exists today is on the database this migration is
    # running against, so its own `users` table is exactly the directory it
    # needs. On a shard that table is empty and this inserts nothing; only
    # the main database's copy is ever read.
    op.execute(
        "INSERT INTO user_directory (phone, user_id, created_at) "
        "SELECT phone, id, created_at FROM users "
        "ON CONFLICT (phone) DO NOTHING"
    )

    # Placement is computed from the user id now, so the recorded key would
    # only be a second, drifting answer to the same question.
    op.drop_index(op.f("ix_users_partition_key"), table_name="users")
    op.drop_column("users", "partition_key")
    op.drop_table("shard_mappings")

    # The owning `users` row is on this same instance again, so these can be
    # enforced by the database rather than by convention.
    for table in _USER_OWNED_TABLES:
        op.create_foreign_key(
            f"{table}_user_id_fkey",
            table,
            "users",
            ["user_id"],
            ["id"],
            ondelete="CASCADE",
        )


def downgrade() -> None:
    for table in _USER_OWNED_TABLES:
        op.drop_constraint(f"{table}_user_id_fkey", table, type_="foreignkey")

    op.create_table(
        "shard_mappings",
        sa.Column("partition_key", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("host", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("port", sa.Integer(), nullable=False),
        sa.Column("database", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.PrimaryKeyConstraint("partition_key"),
    )
    op.add_column(
        "users",
        sa.Column("partition_key", sqlmodel.sql.sqltypes.AutoString(), nullable=True),
    )
    op.create_index(
        op.f("ix_users_partition_key"), "users", ["partition_key"], unique=False
    )
    op.drop_index(op.f("ix_user_directory_user_id"), table_name="user_directory")
    op.drop_table("user_directory")
    op.drop_table("shard_ranges")
