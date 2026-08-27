"""shard mappings, user placement, task assignee

Splits rows across several PostgreSQL instances and several named API
instances. See db.py (partitioning) and tasks.py (assignees).

Every instance runs this same migration set — the schema is identical
everywhere — but only the main/first instance's `shard_mappings` rows are
ever read.

Revision ID: 0008_horizontal_scaling
Revises: 0007_location_mapping
Create Date: 2026-08-23

"""
from typing import Sequence, Union

import sqlalchemy as sa
import sqlmodel
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "0008_horizontal_scaling"
down_revision: Union[str, Sequence[str], None] = "0007_location_mapping"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None

# Tables whose rows are partitioned across database instances. Their owning
# `users` row lives on the main instance, which may be a different server
# entirely, so the foreign key to it cannot be enforced any more.
_PARTITIONED_TABLES = ("provider_credentials", "servers", "tasks")


def upgrade() -> None:
    op.create_table(
        "shard_mappings",
        sa.Column("partition_key", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("host", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("port", sa.Integer(), nullable=False),
        sa.Column("database", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.PrimaryKeyConstraint("partition_key"),
    )

    # Where a user's partitioned rows live; null means the main instance,
    # which is where every pre-existing user's rows already are.
    op.add_column(
        "users",
        sa.Column("partition_key", sqlmodel.sql.sqltypes.AutoString(), nullable=True),
    )
    op.create_index(
        op.f("ix_users_partition_key"), "users", ["partition_key"], unique=False
    )

    # Which API instance owns a task; null means the main/first one, which
    # is the correct reading for tasks created before instances were named.
    op.add_column(
        "tasks",
        sa.Column("assignee", sqlmodel.sql.sqltypes.AutoString(), nullable=True),
    )
    op.create_index(op.f("ix_tasks_assignee"), "tasks", ["assignee"], unique=False)

    for table in _PARTITIONED_TABLES:
        op.execute(f"ALTER TABLE {table} DROP CONSTRAINT IF EXISTS {table}_user_id_fkey")


def downgrade() -> None:
    for table in _PARTITIONED_TABLES:
        op.create_foreign_key(
            f"{table}_user_id_fkey",
            table,
            "users",
            ["user_id"],
            ["id"],
            ondelete="CASCADE",
        )
    op.drop_index(op.f("ix_tasks_assignee"), table_name="tasks")
    op.drop_column("tasks", "assignee")
    op.drop_index(op.f("ix_users_partition_key"), table_name="users")
    op.drop_column("users", "partition_key")
    op.drop_table("shard_mappings")
