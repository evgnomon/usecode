"""server type mapping cities

Revision ID: 0006_server_type_cities
Revises: 0005_provider_catalog
Create Date: 2026-08-22

"""
from typing import Sequence, Union

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "0006_server_type_cities"
down_revision: Union[str, Sequence[str], None] = "0005_provider_catalog"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.add_column(
        "server_type_mappings",
        sa.Column("cities", sa.JSON(), nullable=False, server_default="[]"),
    )
    op.alter_column("server_type_mappings", "cities", server_default=None)


def downgrade() -> None:
    op.drop_column("server_type_mappings", "cities")
