"""
Migration: Upgrade to stella_en_1.5B_v5 embeddings

This migration upgrades text embeddings from all-MiniLM-L6-v2 (384-dim)
to stella_en_1.5B_v5 (768-dim via MRL truncation) for improved search quality.

Steps:
1. Backup current embeddings (optional, for rollback)
2. Update embedding dimension in text_embeddings table
3. Re-generate embeddings for all text content using new model
4. Update pgvector HNSW indexes with optimal parameters for 768-dim
5. Verify migration success

Usage:
    python -m vault.backend.migrations.upgrade_embeddings_stella upgrade
    python -m vault.backend.migrations.upgrade_embeddings_stella downgrade
"""

import asyncio
import logging
from datetime import datetime
from pathlib import Path
from typing import Optional, List, Dict, Any
import sys

from sqlalchemy import text
from sqlalchemy.ext.asyncio import AsyncSession, create_async_engine
from sqlalchemy.orm import sessionmaker

from ..src.modules.embedding_generator.text_embedder import TextEmbedder
from ..src.modules.embedding_generator.types import (
    TEXT_MODEL_NAME,
    TEXT_EMBEDDING_DIM,
    OLD_TEXT_EMBEDDING_DIM,
)

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

BATCH_SIZE = 50
BACKUP_TABLE = "text_embeddings_backup_384"


class MigrationStats:
    """Track migration statistics."""

    def __init__(self):
        self.total_embeddings = 0
        self.processed_embeddings = 0
        self.failed_embeddings = 0
        self.start_time: Optional[datetime] = None
        self.end_time: Optional[datetime] = None

    def __str__(self) -> str:
        duration = "N/A"
        if self.start_time and self.end_time:
            duration = str(self.end_time - self.start_time)
        return (
            f"Migration Stats:\n"
            f"  Total: {self.total_embeddings}\n"
            f"  Processed: {self.processed_embeddings}\n"
            f"  Failed: {self.failed_embeddings}\n"
            f"  Duration: {duration}"
        )


async def check_migration_needed(session: AsyncSession) -> bool:
    """
    Check if migration is needed by examining existing embeddings.

    Returns:
        True if migration needed, False otherwise
    """
    query = text("""
        SELECT
            vector_dims(embedding) as dim,
            COUNT(*) as count
        FROM text_embeddings
        GROUP BY vector_dims(embedding)
        LIMIT 1
    """)

    result = await session.execute(query)
    row = result.fetchone()

    if not row:
        logger.info("No embeddings found - migration not needed")
        return False

    current_dim = row.dim
    count = row.count

    logger.info(f"Found {count} embeddings with dimension {current_dim}")

    if current_dim == OLD_TEXT_EMBEDDING_DIM:
        logger.info("Migration needed: embeddings are 384-dim")
        return True
    elif current_dim == TEXT_EMBEDDING_DIM:
        logger.info("Migration not needed: already using 768-dim")
        return False
    else:
        logger.warning(f"Unexpected embedding dimension: {current_dim}")
        return False


async def create_backup(session: AsyncSession) -> None:
    """
    Create backup of current embeddings for rollback capability.

    Args:
        session: Database session
    """
    logger.info(f"Creating backup table: {BACKUP_TABLE}")

    drop_query = text(f"DROP TABLE IF EXISTS {BACKUP_TABLE}")
    await session.execute(drop_query)

    backup_query = text(f"""
        CREATE TABLE {BACKUP_TABLE} AS
        SELECT * FROM text_embeddings
    """)
    await session.execute(backup_query)
    await session.commit()

    count_query = text(f"SELECT COUNT(*) FROM {BACKUP_TABLE}")
    result = await session.execute(count_query)
    count = result.scalar()

    logger.info(f"Backed up {count} embeddings to {BACKUP_TABLE}")


async def get_text_contents(
    session: AsyncSession,
    batch_size: int = BATCH_SIZE,
    offset: int = 0,
) -> List[Dict[str, Any]]:
    """
    Fetch batch of text contents with their embeddings.

    Args:
        session: Database session
        batch_size: Number of records to fetch
        offset: Offset for pagination

    Returns:
        List of text content records
    """
    query = text("""
        SELECT
            tc.id as text_content_id,
            tc.content,
            tc.content_length,
            tc.language,
            f.id as file_id,
            f.file_path,
            f.filename,
            te.id as embedding_id
        FROM text_contents tc
        JOIN files f ON tc.file_id = f.id
        LEFT JOIN text_embeddings te ON tc.id = te.text_content_id
        WHERE tc.content IS NOT NULL
        ORDER BY tc.id
        LIMIT :limit OFFSET :offset
    """)

    result = await session.execute(
        query,
        {"limit": batch_size, "offset": offset}
    )

    return [
        {
            'text_content_id': row.text_content_id,
            'content': row.content,
            'content_length': row.content_length,
            'language': row.language,
            'file_id': row.file_id,
            'file_path': row.file_path,
            'filename': row.filename,
            'embedding_id': row.embedding_id,
        }
        for row in result
    ]


async def regenerate_embedding(
    embedder: TextEmbedder,
    content: str,
    metadata: Dict[str, Any],
) -> List[float]:
    """
    Generate new embedding using stella model.

    Args:
        embedder: Text embedder instance
        content: Text content to embed
        metadata: Document metadata for context

    Returns:
        Embedding vector as list of floats
    """
    embedding, _ = await embedder.embed(content, metadata)
    return embedding.tolist()


async def update_embedding(
    session: AsyncSession,
    text_content_id: str,
    embedding: List[float],
    embedding_id: Optional[str] = None,
) -> None:
    """
    Update or insert embedding in database.

    Args:
        session: Database session
        text_content_id: Text content ID
        embedding: New embedding vector
        embedding_id: Existing embedding ID (if updating)
    """
    if embedding_id:
        update_query = text("""
            UPDATE text_embeddings
            SET
                embedding = :embedding::vector,
                model_name = :model_name,
                updated_at = NOW()
            WHERE id = :embedding_id
        """)
        await session.execute(
            update_query,
            {
                'embedding': embedding,
                'model_name': TEXT_MODEL_NAME,
                'embedding_id': embedding_id,
            }
        )
    else:
        insert_query = text("""
            INSERT INTO text_embeddings
                (text_content_id, embedding, model_name, model_version, created_at, updated_at)
            VALUES
                (:text_content_id, :embedding::vector, :model_name, '1.0', NOW(), NOW())
        """)
        await session.execute(
            insert_query,
            {
                'text_content_id': text_content_id,
                'embedding': embedding,
                'model_name': TEXT_MODEL_NAME,
            }
        )


async def rebuild_indexes(session: AsyncSession) -> None:
    """
    Rebuild pgvector HNSW indexes with optimal parameters for 768-dim.

    For 768-dimensional vectors, we use:
    - m=24: Optimal connections for this dimensionality
    - ef_construction=200: Good balance of index quality and build time

    Args:
        session: Database session
    """
    logger.info("Rebuilding HNSW indexes with optimized parameters")

    drop_index = text("DROP INDEX IF EXISTS text_embeddings_hnsw_idx")
    await session.execute(drop_index)

    create_index = text("""
        CREATE INDEX text_embeddings_hnsw_idx
        ON text_embeddings
        USING hnsw (embedding vector_cosine_ops)
        WITH (m = 24, ef_construction = 200)
    """)
    await session.execute(create_index)
    await session.commit()

    logger.info("HNSW indexes rebuilt successfully")


async def upgrade(database_url: str) -> None:
    """
    Run the upgrade migration.

    Args:
        database_url: Database connection URL
    """
    engine = create_async_engine(database_url, echo=False)
    async_session = sessionmaker(
        engine, class_=AsyncSession, expire_on_commit=False
    )

    stats = MigrationStats()
    stats.start_time = datetime.now()

    async with async_session() as session:
        logger.info("Starting embedding migration to stella_en_1.5B_v5")

        if not await check_migration_needed(session):
            logger.info("Migration not needed, exiting")
            return

        await create_backup(session)

        embedder = TextEmbedder()

        count_query = text("SELECT COUNT(*) FROM text_contents WHERE content IS NOT NULL")
        result = await session.execute(count_query)
        stats.total_embeddings = result.scalar()

        logger.info(f"Processing {stats.total_embeddings} text contents")

        offset = 0
        while True:
            batch = await get_text_contents(session, BATCH_SIZE, offset)
            if not batch:
                break

            for item in batch:
                try:
                    metadata = {
                        'file_path': item['file_path'],
                        'file_name': item['filename'],
                    }

                    embedding = await regenerate_embedding(
                        embedder,
                        item['content'],
                        metadata,
                    )

                    await update_embedding(
                        session,
                        item['text_content_id'],
                        embedding,
                        item['embedding_id'],
                    )

                    stats.processed_embeddings += 1

                    if stats.processed_embeddings % 10 == 0:
                        progress = (stats.processed_embeddings / stats.total_embeddings) * 100
                        logger.info(
                            f"Progress: {stats.processed_embeddings}/{stats.total_embeddings} "
                            f"({progress:.1f}%)"
                        )

                except Exception as e:
                    logger.error(f"Failed to process {item['text_content_id']}: {e}")
                    stats.failed_embeddings += 1

            await session.commit()
            offset += BATCH_SIZE

        await rebuild_indexes(session)

    stats.end_time = datetime.now()
    logger.info("Migration completed!")
    logger.info(str(stats))

    await engine.dispose()


async def downgrade(database_url: str) -> None:
    """
    Rollback the migration using backup.

    Args:
        database_url: Database connection URL
    """
    engine = create_async_engine(database_url, echo=False)
    async_session = sessionmaker(
        engine, class_=AsyncSession, expire_on_commit=False
    )

    async with async_session() as session:
        logger.info("Rolling back to 384-dim embeddings")

        check_backup = text(f"""
            SELECT COUNT(*)
            FROM information_schema.tables
            WHERE table_name = '{BACKUP_TABLE}'
        """)
        result = await session.execute(check_backup)
        has_backup = result.scalar() > 0

        if not has_backup:
            logger.error(f"Backup table {BACKUP_TABLE} not found, cannot rollback")
            return

        restore_query = text(f"""
            TRUNCATE text_embeddings;
            INSERT INTO text_embeddings SELECT * FROM {BACKUP_TABLE};
        """)
        await session.execute(restore_query)
        await session.commit()

        logger.info("Rebuilding indexes for 384-dim")
        drop_index = text("DROP INDEX IF EXISTS text_embeddings_hnsw_idx")
        await session.execute(drop_index)

        create_index = text("""
            CREATE INDEX text_embeddings_hnsw_idx
            ON text_embeddings
            USING hnsw (embedding vector_cosine_ops)
            WITH (m = 24, ef_construction = 200)
        """)
        await session.execute(create_index)
        await session.commit()

        logger.info("Rollback completed successfully")

    await engine.dispose()


def main():
    """CLI entry point for migration."""
    import os

    if len(sys.argv) < 2:
        print("Usage: python upgrade_embeddings_stella.py [upgrade|downgrade]")
        sys.exit(1)

    command = sys.argv[1]
    database_url = os.getenv("DATABASE_URL")

    if not database_url:
        print("Error: DATABASE_URL environment variable not set")
        sys.exit(1)

    if command == "upgrade":
        asyncio.run(upgrade(database_url))
    elif command == "downgrade":
        asyncio.run(downgrade(database_url))
    else:
        print(f"Unknown command: {command}")
        print("Usage: python upgrade_embeddings_stella.py [upgrade|downgrade]")
        sys.exit(1)


if __name__ == "__main__":
    main()
