"""Add composite indexes for sync pagination and embedding queries

Revision ID: 20251117_add_composite_indexes
Revises: 20251117_add_indexing_jobs_table
Create Date: 2025-11-17

This migration adds:
1. Composite index on documents(user_id, modified_at DESC) for efficient sync pagination
2. Composite index on text_embeddings(model_version, dimension) for efficient model version queries
"""
from alembic import op

revision = "20251117_add_composite_indexes"
down_revision = "20251117_add_indexing_jobs_table"
branch_labels = None
depends_on = None


def upgrade() -> None:
    """Add composite indexes for improved query performance."""
    
    # Index for sync pagination queries (documents by user, ordered by modification time)
    # This supports queries like: SELECT * FROM documents WHERE user_id = ? ORDER BY modified_at DESC LIMIT ? OFFSET ?
    op.execute("""
        CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_documents_user_modified 
        ON documents(user_id, modified_at DESC)
    """)
    
    # Composite index for embedding model version queries
    # This supports queries like: SELECT * FROM text_embeddings WHERE model_version = ? AND dimension = ?
    # Improves performance when filtering embeddings by model and dimension
    op.execute("""
        CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_text_embeddings_model_dimension 
        ON text_embeddings(model_version, dimension)
    """)


def downgrade() -> None:
    """Remove composite indexes."""
    
    op.execute("DROP INDEX CONCURRENTLY IF EXISTS idx_documents_user_modified")
    op.execute("DROP INDEX CONCURRENTLY IF EXISTS idx_text_embeddings_model_dimension")
