"""Add sync_state and soft delete fields to documents

Revision ID: 003_add_sync_state
Revises: 002_sync_tables
Create Date: 2025-11-17
"""
from alembic import op
import sqlalchemy as sa
from sqlalchemy.dialects import postgresql

revision = "003_add_sync_state"
down_revision = "002_sync_tables"
branch_labels = None
depends_on = None


def upgrade() -> None:
    """Add sync_state enum and soft delete fields to documents table.
    
    This migration implements explicit conflict resolution by adding:
    - sync_state: Tracks if document is synced, pending, or in conflict
    - deleted_at: Timestamp when document was soft-deleted
    - deleted_by_device_id: Which device performed the deletion
    - cleanup_after: When to permanently delete (30 days after soft delete)
    
    Also creates a partial unique index to allow path reuse after deletion.
    """
    
    # Create SyncState enum type
    op.execute(
        "CREATE TYPE syncstate AS ENUM ('synced', 'pending', 'conflict')"
    )
    
    # Add sync_state column with default value
    op.add_column(
        "documents",
        sa.Column(
            "sync_state",
            sa.Enum("synced", "pending", "conflict", name="syncstate"),
            nullable=False,
            server_default="synced",
            comment="Synchronization state for conflict tracking"
        )
    )
    
    # Create index on sync_state
    op.create_index(
        "ix_documents_sync_state",
        "documents",
        ["sync_state"]
    )
    
    # Add soft delete fields
    op.add_column(
        "documents",
        sa.Column(
            "deleted_at",
            sa.DateTime(),
            nullable=True,
            comment="When document was soft deleted (NULL if active)"
        )
    )
    
    op.add_column(
        "documents",
        sa.Column(
            "deleted_by_device_id",
            sa.Integer(),
            nullable=True,
            comment="Which device deleted this document"
        )
    )
    
    op.add_column(
        "documents",
        sa.Column(
            "cleanup_after",
            sa.DateTime(),
            nullable=True,
            comment="When to hard delete (30 days after soft delete)"
        )
    )
    
    # Add foreign key constraint for deleted_by_device_id
    op.create_foreign_key(
        "fk_documents_deleted_by_device",
        "documents",
        "devices",
        ["deleted_by_device_id"],
        ["id"],
        ondelete="SET NULL"
    )
    
    # Create indexes for soft delete queries
    op.create_index(
        "ix_documents_deleted_at",
        "documents",
        ["deleted_at"]
    )
    
    op.create_index(
        "ix_documents_cleanup_after",
        "documents",
        ["cleanup_after"]
    )
    
    # Drop the old unique constraint
    op.drop_constraint("uq_document_user_path", "documents", type_="unique")
    
    # Create partial unique index (allows path reuse after soft delete)
    # This is PostgreSQL-specific and allows immediate path reuse
    op.execute(
        """
        CREATE UNIQUE INDEX uq_document_user_path_active 
        ON documents (user_id, path) 
        WHERE deleted_at IS NULL
        """
    )
    
    # Update sync_logs to allow NULL document_id (for orphaned logs after hard delete)
    # First, need to drop the foreign key and recreate it
    op.drop_constraint("sync_logs_document_id_fkey", "sync_logs", type_="foreignkey")
    
    # Add document_path column to sync_logs (to preserve path after hard delete)
    op.add_column(
        "sync_logs",
        sa.Column(
            "document_path",
            sa.String(length=1024),
            nullable=True,  # Will be populated, then made NOT NULL
            comment="Preserve path even after document is hard deleted"
        )
    )
    
    # Populate document_path from existing documents
    op.execute(
        """
        UPDATE sync_logs sl
        SET document_path = d.path
        FROM documents d
        WHERE sl.document_id = d.id
        """
    )
    
    # Make document_path NOT NULL after populating
    op.alter_column("sync_logs", "document_path", nullable=False)
    
    # Recreate foreign key with ondelete SET NULL
    op.create_foreign_key(
        "fk_sync_logs_document",
        "sync_logs",
        "documents",
        ["document_id"],
        ["id"],
        ondelete="SET NULL"
    )
    
    # Make document_id nullable
    op.alter_column("sync_logs", "document_id", nullable=True)


def downgrade() -> None:
    """Remove sync_state and soft delete fields.
    
    WARNING: This will lose soft delete and sync state information.
    Test thoroughly before running in production.
    """
    
    # Drop partial unique index
    op.execute("DROP INDEX IF EXISTS uq_document_user_path_active")
    
    # Recreate the old unique constraint
    op.create_unique_constraint(
        "uq_document_user_path",
        "documents",
        ["user_id", "path"]
    )
    
    # Drop indexes
    op.drop_index("ix_documents_cleanup_after", "documents")
    op.drop_index("ix_documents_deleted_at", "documents")
    op.drop_index("ix_documents_sync_state", "documents")
    
    # Drop foreign key for deleted_by_device_id
    op.drop_constraint("fk_documents_deleted_by_device", "documents", type_="foreignkey")
    
    # Drop soft delete columns
    op.drop_column("documents", "cleanup_after")
    op.drop_column("documents", "deleted_by_device_id")
    op.drop_column("documents", "deleted_at")
    
    # Drop sync_state column
    op.drop_column("documents", "sync_state")
    
    # Drop SyncState enum
    op.execute("DROP TYPE IF EXISTS syncstate")
    
    # Revert sync_logs changes
    op.drop_constraint("fk_sync_logs_document", "sync_logs", type_="foreignkey")
    
    # Make document_id NOT NULL again
    op.alter_column("sync_logs", "document_id", nullable=False)
    
    # Recreate original foreign key
    op.create_foreign_key(
        "sync_logs_document_id_fkey",
        "sync_logs",
        "documents",
        ["document_id"],
        ["id"],
        ondelete="CASCADE"
    )
    
    # Drop document_path column
    op.drop_column("sync_logs", "document_path")
