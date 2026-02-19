"""Initial schema with pgvector support

Revision ID: 001_initial
Revises: 
Create Date: 2025-11-09
"""
from alembic import op
import sqlalchemy as sa
from sqlalchemy.dialects import postgresql
from pgvector.sqlalchemy import Vector

revision = "001_initial"
down_revision = None
branch_labels = None
depends_on = None


def upgrade() -> None:
    """Create all tables, indexes, triggers for Vault database."""
    # Read and execute the SQL schema
    import pathlib
    sql_file = pathlib.Path(__file__).parent / "001_initial_schema.sql"
    with open(sql_file, "r", encoding="utf-8") as f:
        sql_content = f.read()
    
    # Execute the complete SQL schema
    # Split by statement and execute
    op.execute(sql_content)


def downgrade() -> None:
    """Drop all tables and extensions."""
    tables = [
        "search_history", "file_tags", "tags",
        "image_embeddings", "images", "text_embeddings",
        "text_content", "files", "watch_folders"
    ]
    
    for table in tables:
        op.execute(f"DROP TABLE IF EXISTS {table} CASCADE")
    
    op.execute("DROP FUNCTION IF EXISTS update_text_search_vector()")
    op.execute("DROP FUNCTION IF EXISTS update_electric_timestamp()")
    op.execute("DROP EXTENSION IF EXISTS pg_trgm")
    op.execute("DROP EXTENSION IF EXISTS vector")
    op.execute("DROP EXTENSION IF EXISTS "uuid-ossp"")
