"""tasks are partitioned on their assignee

A task is never fetched by someone asking for it by id: each API instance
sweeps its own outstanding work on a timer, so the read that decides
placement is "this node's tasks". `tasks` therefore stops inheriting the
owner's placement and becomes a root table partitioned on `assignee`,
which `db.seed_shard_ranges` gives a map of its own on the next boot (it
is derived from the schema: the table now has no foreign key). One node's
whole backlog is a single query on a single instance, where before the
sweep walked every database looking for its own rows.

`assignee` becomes part of the primary key and stops being nullable: it is
half of a task's address — a row is read as (assignee, id) — so a task
with no assignee would have nowhere to be. Existing rows carrying a null
assignee belonged to the main API instance by the old convention; there is
no node name to give them here, so they are handed to the deployment's
first node by name via `USECODE_AGENT_MAIN_NODE_NAME`/`USECODE_AGENT_NODE_NAME` if the
environment says who that is, and deleted otherwise — an unaddressable
task is one nothing would ever advance again.

`user_id` becomes a plain column, because the owning `users` row is now on
an unrelated instance and no database can enforce the reference — the same
situation as `api_keys.user_id`. `user_tasks` is the index that answers the
reverse question, "what is in flight for me?" (GET /tasks), which would
otherwise have to scan every instance. Existing tasks are still co-located
with their owners, which is why the index can be filled from `tasks` on
this same instance; the task rows themselves are left where they are, since
the `tasks` map is seeded whole-space on the main database in a deployment
that already holds rows there.

Revision ID: 0012_tasks_partition_on_assignee
Revises: 0011_api_keys_own_shard
Create Date: 2026-08-24

"""
import os
from typing import Sequence, Union

import sqlalchemy as sa
import sqlmodel
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "0012_tasks_partition_on_assignee"
down_revision: Union[str, Sequence[str], None] = "0011_api_keys_own_shard"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def _main_node_name() -> str:
    """The node an unassigned task used to belong to, if this process can
    still tell. `USECODE_AGENT_MAIN_NODE_NAME` named it explicitly; failing that,
    a single-instance deployment's own name is the same answer."""
    for variable in ("USECODE_AGENT_MAIN_NODE_NAME", "USECODE_AGENT_NODE_NAME"):
        name = os.environ.get(variable, "").strip()
        if name:
            return name
    return ""


def upgrade() -> None:
    # -- the owner's index of their tasks --------------------------------
    op.create_table(
        "user_tasks",
        sa.Column("user_id", sa.Uuid(), nullable=False),
        sa.Column("task_id", sa.Uuid(), nullable=False),
        sa.Column("assignee", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.ForeignKeyConstraint(["user_id"], ["users.id"], ondelete="CASCADE"),
        sa.PrimaryKeyConstraint("user_id", "task_id"),
    )

    # -- every task gets an assignee, since that is its address ----------
    main_node = _main_node_name()
    if main_node:
        op.execute(
            sa.text("UPDATE tasks SET assignee = :node WHERE assignee IS NULL").bindparams(
                node=main_node
            )
        )
    op.execute("DELETE FROM tasks WHERE assignee IS NULL")
    op.alter_column("tasks", "assignee", nullable=False)

    op.execute(
        "INSERT INTO user_tasks (user_id, task_id, assignee, created_at) "
        "SELECT user_id, id, assignee, created_at FROM tasks "
        "ON CONFLICT DO NOTHING"
    )

    # -- tasks: keyed and placed by the assignee -------------------------
    op.execute("ALTER TABLE tasks DROP CONSTRAINT IF EXISTS tasks_user_id_fkey")
    op.drop_constraint("tasks_pkey", "tasks", type_="primary")
    # Assignee first: a sweep reads one contiguous run of the key.
    op.create_primary_key("tasks_pkey", "tasks", ["assignee", "id"])
    # The primary key now leads with it, so the standalone index is dead
    # weight on every write.
    op.drop_index(op.f("ix_tasks_assignee"), table_name="tasks")


def downgrade() -> None:
    op.create_index(op.f("ix_tasks_assignee"), "tasks", ["assignee"], unique=False)
    op.drop_constraint("tasks_pkey", "tasks", type_="primary")
    op.create_primary_key("tasks_pkey", "tasks", ["id"])
    op.alter_column("tasks", "assignee", nullable=True)
    op.create_foreign_key(
        "tasks_user_id_fkey", "tasks", "users", ["user_id"], ["id"], ondelete="CASCADE"
    )
    op.drop_table("user_tasks")

    # The map only ever addressed a table that no longer hashes its own key.
    op.execute("DELETE FROM shard_ranges WHERE \"table\" = 'tasks'")
