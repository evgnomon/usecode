"""servers and server type mappings

Revision ID: 0003_servers
Revises: 0002_provider_credentials
Create Date: 2026-08-22

"""
from typing import Sequence, Union

import sqlalchemy as sa
import sqlmodel
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "0003_servers"
down_revision: Union[str, Sequence[str], None] = "0002_provider_credentials"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None

SERVER_TYPE_MAPPINGS = [
    {"series": "x1", "provider": "hetzner", "provider_server_type": "cx22"},
    {"series": "x2", "provider": "hetzner", "provider_server_type": "cx32"},
    {"series": "x4", "provider": "hetzner", "provider_server_type": "cx42"},
    {"series": "x8", "provider": "hetzner", "provider_server_type": "cx52"},
    {"series": "y1", "provider": "digitalocean", "provider_server_type": "s-1vcpu-1gb"},
    {"series": "y2", "provider": "digitalocean", "provider_server_type": "s-2vcpu-2gb"},
    {"series": "y4", "provider": "digitalocean", "provider_server_type": "s-4vcpu-8gb"},
    {"series": "y8", "provider": "digitalocean", "provider_server_type": "s-8vcpu-16gb"},
]


def upgrade() -> None:
    server_type_mappings = op.create_table(
        "server_type_mappings",
        sa.Column("series", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("provider", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("provider_server_type", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.PrimaryKeyConstraint("series"),
    )
    op.bulk_insert(server_type_mappings, SERVER_TYPE_MAPPINGS)

    op.create_table(
        "servers",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("user_id", sa.Uuid(), nullable=False),
        sa.Column("provider", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("provider_server_id", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("type", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("name", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("status", sqlmodel.sql.sqltypes.AutoString(), nullable=False),
        sa.Column("public_ip4", sqlmodel.sql.sqltypes.AutoString(), nullable=True),
        sa.Column("public_ip6", sqlmodel.sql.sqltypes.AutoString(), nullable=True),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.Column("updated_at", sa.DateTime(timezone=True), nullable=False),
        sa.ForeignKeyConstraint(["user_id"], ["users.id"], ondelete="CASCADE"),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("provider", "provider_server_id", name="uq_servers_provider_id"),
    )
    op.create_index("ix_servers_user_id", "servers", ["user_id"])
    op.create_index("ix_servers_provider_server_id", "servers", ["provider_server_id"])


def downgrade() -> None:
    op.drop_index("ix_servers_provider_server_id", table_name="servers")
    op.drop_index("ix_servers_user_id", table_name="servers")
    op.drop_table("servers")
    op.drop_table("server_type_mappings")
