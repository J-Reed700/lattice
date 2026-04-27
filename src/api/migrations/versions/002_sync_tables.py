"""Add sync tables for multi-device synchronization

Revision ID: 002_sync_tables
Revises: 001_5_create_users_table
Create Date: 2025-11-15
"""
from alembic import op
import sqlalchemy as sa
from sqlalchemy.dialects import postgresql

revision = "002_sync_tables"
down_revision = "001_5_create_users_table"
branch_labels = None
depends_on = None


def upgrade() -> None:
    """Create sync tables: devices, documents, sync_logs, conflicts."""

    # Create devices table
    op.create_table(
        "devices",
        sa.Column("id", sa.Integer(), nullable=False),
        sa.Column("user_id", sa.Integer(), nullable=False),
        sa.Column("device_id", sa.String(length=36), nullable=False),
        sa.Column("device_name", sa.String(length=255), nullable=False),
        sa.Column("last_seen_at", sa.DateTime(), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
        sa.ForeignKeyConstraint(["user_id"], ["users.id"], ondelete="CASCADE"),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("device_id"),
    )
    op.create_index("ix_devices_id", "devices", ["id"])
    op.create_index("ix_devices_device_id", "devices", ["device_id"], unique=True)
    op.create_index("ix_devices_user_id", "devices", ["user_id"])
    op.create_index("idx_device_user", "devices", ["user_id", "device_id"])

    # Create documents table
    op.create_table(
        "documents",
        sa.Column("id", sa.Integer(), nullable=False),
        sa.Column("user_id", sa.Integer(), nullable=False),
        sa.Column("device_id", sa.Integer(), nullable=True),
        sa.Column("last_modified_device_id", sa.Integer(), nullable=True),
        sa.Column("path", sa.String(length=1024), nullable=False),
        sa.Column("title", sa.String(length=512), nullable=True),
        sa.Column("content", sa.Text(), nullable=True),
        sa.Column("content_hash", sa.String(length=64), nullable=True),
        sa.Column("created_at", sa.DateTime(), nullable=False),
        sa.Column("modified_at", sa.DateTime(), nullable=False),
        sa.Column("version", sa.Integer(), nullable=False),
        sa.ForeignKeyConstraint(["user_id"], ["users.id"], ondelete="CASCADE"),
        sa.ForeignKeyConstraint(["device_id"], ["devices.id"], ondelete="SET NULL"),
        sa.ForeignKeyConstraint(["last_modified_device_id"], ["devices.id"], ondelete="SET NULL"),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("user_id", "path", name="uq_document_user_path"),
    )
    op.create_index("ix_documents_id", "documents", ["id"])
    op.create_index("ix_documents_user_id", "documents", ["user_id"])
    op.create_index("ix_documents_device_id", "documents", ["device_id"])
    op.create_index("ix_documents_created_at", "documents", ["created_at"])
    op.create_index("ix_documents_modified_at", "documents", ["modified_at"])
    op.create_index("idx_document_modified", "documents", ["modified_at"])
    op.create_index("idx_document_user_modified", "documents", ["user_id", "modified_at"])
    op.create_index("idx_document_last_modifier", "documents", ["last_modified_device_id"])

    # Create sync_logs table
    op.create_table(
        "sync_logs",
        sa.Column("id", sa.Integer(), nullable=False),
        sa.Column("device_id", sa.Integer(), nullable=False),
        sa.Column("document_id", sa.Integer(), nullable=False),
        sa.Column(
            "action",
            sa.Enum("create", "update", "delete", name="syncaction"),
            nullable=False,
        ),
        sa.Column("timestamp", sa.DateTime(), nullable=False),
        sa.Column("version", sa.Integer(), nullable=False),
        sa.ForeignKeyConstraint(["device_id"], ["devices.id"], ondelete="CASCADE"),
        sa.ForeignKeyConstraint(["document_id"], ["documents.id"], ondelete="CASCADE"),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index("ix_sync_logs_id", "sync_logs", ["id"])
    op.create_index("ix_sync_logs_device_id", "sync_logs", ["device_id"])
    op.create_index("ix_sync_logs_document_id", "sync_logs", ["document_id"])
    op.create_index("ix_sync_logs_timestamp", "sync_logs", ["timestamp"])
    op.create_index("idx_sync_log_device_timestamp", "sync_logs", ["device_id", "timestamp"])
    op.create_index("idx_sync_log_document_timestamp", "sync_logs", ["document_id", "timestamp"])

    # Create conflicts table
    op.create_table(
        "conflicts",
        sa.Column("id", sa.Integer(), nullable=False),
        sa.Column("document_id", sa.Integer(), nullable=False),
        sa.Column("local_version", sa.Integer(), nullable=False),
        sa.Column("remote_version", sa.Integer(), nullable=False),
        sa.Column("local_modified_at", sa.DateTime(), nullable=False),
        sa.Column("remote_modified_at", sa.DateTime(), nullable=False),
        sa.Column("local_content_hash", sa.String(length=64), nullable=True),
        sa.Column("remote_content_hash", sa.String(length=64), nullable=True),
        sa.Column(
            "status",
            sa.Enum(
                "pending",
                "resolved_local",
                "resolved_remote",
                "resolved_merge",
                name="conflictstatus",
            ),
            nullable=False,
        ),
        sa.Column("resolved_at", sa.DateTime(), nullable=True),
        sa.Column("created_at", sa.DateTime(), nullable=False),
        sa.ForeignKeyConstraint(["document_id"], ["documents.id"], ondelete="CASCADE"),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index("ix_conflicts_id", "conflicts", ["id"])
    op.create_index("ix_conflicts_document_id", "conflicts", ["document_id"])
    op.create_index("idx_conflict_status", "conflicts", ["status"])
    op.create_index("idx_conflict_document_status", "conflicts", ["document_id", "status"])


def downgrade() -> None:
    """Drop sync tables."""
    # Drop indexes first
    op.drop_index("idx_conflict_document_status", "conflicts")
    op.drop_index("idx_conflict_status", "conflicts")
    op.drop_index("ix_conflicts_document_id", "conflicts")
    op.drop_index("ix_conflicts_id", "conflicts")
    
    op.drop_index("idx_sync_log_document_timestamp", "sync_logs")
    op.drop_index("idx_sync_log_device_timestamp", "sync_logs")
    op.drop_index("ix_sync_logs_timestamp", "sync_logs")
    op.drop_index("ix_sync_logs_document_id", "sync_logs")
    op.drop_index("ix_sync_logs_device_id", "sync_logs")
    op.drop_index("ix_sync_logs_id", "sync_logs")
    
    op.drop_index("idx_document_last_modifier", "documents")
    op.drop_index("idx_document_user_modified", "documents")
    op.drop_index("idx_document_modified", "documents")
    op.drop_index("ix_documents_modified_at", "documents")
    op.drop_index("ix_documents_created_at", "documents")
    op.drop_index("ix_documents_device_id", "documents")
    op.drop_index("ix_documents_user_id", "documents")
    op.drop_index("ix_documents_id", "documents")
    
    op.drop_index("idx_device_user", "devices")
    op.drop_index("ix_devices_user_id", "devices")
    op.drop_index("ix_devices_device_id", "devices")
    op.drop_index("ix_devices_id", "devices")
    
    # Drop tables
    op.drop_table("conflicts")
    op.drop_table("sync_logs")
    op.drop_table("documents")
    op.drop_table("devices")

    # Drop enums
    op.execute("DROP TYPE IF EXISTS syncaction")
    op.execute("DROP TYPE IF EXISTS conflictstatus")
