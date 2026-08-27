"""location mapping keyed per provider

Revision ID: 0007_location_mapping
Revises: 0006_server_type_cities
Create Date: 2026-08-23

"""
from typing import Sequence, Union

from alembic import op

# revision identifiers, used by Alembic.
revision: str = "0007_location_mapping"
down_revision: Union[str, Sequence[str], None] = "0006_server_type_cities"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.drop_constraint("location_mappings_pkey", "location_mappings", type_="primary")
    op.create_primary_key(
        "location_mappings_pkey", "location_mappings", ["code", "provider"]
    )


def downgrade() -> None:
    op.drop_constraint("location_mappings_pkey", "location_mappings", type_="primary")
    op.create_primary_key("location_mappings_pkey", "location_mappings", ["code"])
