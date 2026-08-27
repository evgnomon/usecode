"""shard_ranges: one map per table

0009 gave the deployment a single bucket-to-instance map, implicitly the
`users` one, and called everything it didn't cover "global". That left two
tables — `user_directory` and `otps` — pinned to the main database by
convention rather than by any rule.

There is no such thing as a global table. A table with no foreign key is
partitioned on its own key; one with a foreign key inherits its parent's
placement. So `shard_ranges` grows a `table` column: every row says whose
map it belongs to, and every lookup names the map it is reading. Tables
whose map is a single whole-space range on the main database — the range
table itself, and the catalog tables that are read by scans rather than by
key — are that same mechanism with an answer that doesn't vary by bucket,
not an exception to it.

Existing rows are the `users` map. The other maps are not seeded here:
`db.seed_shard_ranges` fills in whatever is missing on the next boot, and
it is the thing that knows whether a table's rows are already sitting on
the main database and so must stay there.

Revision ID: 0010_shard_ranges_per_table
Revises: 0009_virtual_shards
Create Date: 2026-08-24

"""
from typing import Sequence, Union

import sqlalchemy as sa
import sqlmodel
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "0010_shard_ranges_per_table"
down_revision: Union[str, Sequence[str], None] = "0009_virtual_shards"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.add_column(
        "shard_ranges",
        sa.Column("table", sqlmodel.sql.sqltypes.AutoString(), nullable=True),
    )
    # Every range that exists today addresses user-owned rows.
    op.execute('UPDATE shard_ranges SET "table" = \'users\'')
    op.alter_column("shard_ranges", "table", nullable=False)

    op.drop_constraint("shard_ranges_pkey", "shard_ranges", type_="primary")
    op.create_primary_key(
        "shard_ranges_pkey", "shard_ranges", ["table", "start_bucket"]
    )


def downgrade() -> None:
    # Only the `users` map fits a table-less schema; the rest would collide
    # on start_bucket.
    op.execute("DELETE FROM shard_ranges WHERE \"table\" <> 'users'")
    op.drop_constraint("shard_ranges_pkey", "shard_ranges", type_="primary")
    op.create_primary_key("shard_ranges_pkey", "shard_ranges", ["start_bucket"])
    op.drop_column("shard_ranges", "table")
