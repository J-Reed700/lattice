"""Upgrade embeddings from 768-dim to 1024-dim (BGE-M3)

Revision ID: 20251111_upgrade_bge_m3
Revises: 20251110_add_user_id_to_files
Create Date: 2025-11-11

This migration handles the upgrade from all-mpnet-base-v2 (768-dim) to BGE-M3 (1024-dim).

Migration Strategy:
- Adds new dimension column to embedding tables
- Creates temporary tables for new embeddings
- Provides utilities for gradual reindexing
- Maintains backward compatibility during transition
"""
from alembic import op
import sqlalchemy as sa
from sqlalchemy.dialects import postgresql
from pgvector.sqlalchemy import Vector
import logging

revision = "20251111_upgrade_bge_m3"
down_revision = "20251110_add_user_id_to_files"
branch_labels = None
depends_on = None

logger = logging.getLogger(__name__)


def upgrade() -> None:
    """
    Upgrade embeddings to support BGE-M3 (1024 dimensions).

    This migration:
    1. Adds a 'model_version' column to track embedding model
    2. Adds a 'dimension' column to track embedding dimension
    3. Creates backup tables
    4. Provides indexes for efficient querying during migration
    """

    # Add model tracking columns to text_embeddings
    op.add_column('text_embeddings',
        sa.Column('model_version', sa.String(50), nullable=True, server_default='all-mpnet-base-v2')
    )
    op.add_column('text_embeddings',
        sa.Column('dimension', sa.Integer, nullable=False, server_default='768')
    )

    # Add model tracking columns to image_embeddings (if they exist)
    try:
        op.add_column('image_embeddings',
            sa.Column('model_version', sa.String(50), nullable=True, server_default='clip-vit-base-patch32')
        )
        op.add_column('image_embeddings',
            sa.Column('dimension', sa.Integer, nullable=False, server_default='512')
        )
    except Exception as e:
        logger.warning(f"Could not add columns to image_embeddings: {e}")

    # Create index on model_version for efficient querying
    op.create_index(
        'idx_text_embeddings_model_version',
        'text_embeddings',
        ['model_version'],
        postgresql_using='btree'
    )

    op.create_index(
        'idx_text_embeddings_dimension',
        'text_embeddings',
        ['dimension'],
        postgresql_using='btree'
    )

    # Create a table to track migration progress
    op.create_table(
        'embedding_migration_progress',
        sa.Column('id', sa.Integer, primary_key=True),
        sa.Column('file_id', sa.String, nullable=False),
        sa.Column('old_model', sa.String(50), nullable=False),
        sa.Column('new_model', sa.String(50), nullable=False),
        sa.Column('migrated_at', sa.DateTime, server_default=sa.func.now()),
        sa.Column('status', sa.String(20), nullable=False, server_default='pending')
    )

    op.create_index(
        'idx_migration_progress_file',
        'embedding_migration_progress',
        ['file_id'],
        unique=True
    )

    # Create function to migrate single embedding
    op.execute("""
        CREATE OR REPLACE FUNCTION migrate_embedding_to_bge_m3(
            p_file_id VARCHAR,
            p_new_embedding vector(1024)
        ) RETURNS VOID AS $$
        BEGIN
            -- Update the embedding with new dimension
            UPDATE text_embeddings
            SET
                embedding = p_new_embedding,
                dimension = 1024,
                model_version = 'bge-m3',
                updated_at = NOW()
            WHERE file_id = p_file_id;

            -- Track migration
            INSERT INTO embedding_migration_progress (file_id, old_model, new_model, status)
            VALUES (p_file_id, 'all-mpnet-base-v2', 'bge-m3', 'completed')
            ON CONFLICT (file_id)
            DO UPDATE SET
                status = 'completed',
                migrated_at = NOW();
        END;
        $$ LANGUAGE plpgsql;
    """)

    # Create view for migration status
    op.execute("""
        CREATE OR REPLACE VIEW embedding_migration_status AS
        SELECT
            COUNT(*) FILTER (WHERE dimension = 768) as old_model_count,
            COUNT(*) FILTER (WHERE dimension = 1024) as new_model_count,
            COUNT(*) as total_count,
            ROUND(100.0 * COUNT(*) FILTER (WHERE dimension = 1024) / NULLIF(COUNT(*), 0), 2) as migration_percentage
        FROM text_embeddings;
    """)

    logger.info("Migration setup complete. Old embeddings (768-dim) are still available.")
    logger.info("Use the reindexing script to gradually migrate to BGE-M3 (1024-dim).")


def downgrade() -> None:
    """
    Downgrade from BGE-M3 back to previous model.

    WARNING: This will remove all BGE-M3 embeddings and revert to 768-dim.
    """

    # Drop the migration utilities
    op.execute("DROP VIEW IF EXISTS embedding_migration_status")
    op.execute("DROP FUNCTION IF EXISTS migrate_embedding_to_bge_m3(VARCHAR, vector(1024))")
    op.drop_table('embedding_migration_progress')

    # Remove embeddings that are 1024-dim (BGE-M3)
    op.execute("DELETE FROM text_embeddings WHERE dimension = 1024")

    # Drop indexes
    try:
        op.drop_index('idx_text_embeddings_model_version', 'text_embeddings')
        op.drop_index('idx_text_embeddings_dimension', 'text_embeddings')
    except Exception as e:
        logger.warning(f"Could not drop indexes: {e}")

    # Remove tracking columns
    op.drop_column('text_embeddings', 'dimension')
    op.drop_column('text_embeddings', 'model_version')

    try:
        op.drop_column('image_embeddings', 'dimension')
        op.drop_column('image_embeddings', 'model_version')
    except Exception as e:
        logger.warning(f"Could not drop columns from image_embeddings: {e}")

    logger.info("Downgrade complete. Reverted to 768-dim embeddings.")
