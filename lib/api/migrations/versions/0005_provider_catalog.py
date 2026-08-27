"""location mappings and provider catalog

Revision ID: 0005_provider_catalog
Revises: 0004_tasks
Create Date: 2026-08-22

"""
from typing import Sequence, Union

import sqlalchemy as sa
import sqlmodel
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "0005_provider_catalog"
down_revision: Union[str, Sequence[str], None] = "0004_tasks"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.create_table(
        "location_mappings",
        sa.Column("code", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("provider", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("provider_location_code", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.PrimaryKeyConstraint("code"),
    )

    op.create_table(
        "provider_resources",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("provider", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("kind", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("code", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("data", sa.JSON(), nullable=False),
        sa.Column("updated_at", sa.DateTime(timezone=True), nullable=False),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("provider", "kind", "code", name="uq_provider_resources_kind_code"),
    )
    op.create_index("ix_provider_resources_provider", "provider_resources", ["provider"])
    op.create_index("ix_provider_resources_kind", "provider_resources", ["kind"])


def downgrade() -> None:
    op.drop_index("ix_provider_resources_kind", table_name="provider_resources")
    op.drop_index("ix_provider_resources_provider", table_name="provider_resources")
    op.drop_table("provider_resources")
    op.drop_table("location_mappings")
