#!/usr/bin/env python3
"""
Reindex embeddings from old model (768-dim) to BGE-M3 (1024-dim).

This script gradually reindexes all embeddings to use the new BGE-M3 model.
It can run incrementally and resume from where it left off.

Usage:
    python reindex_bge_m3.py [--batch-size 100] [--max-files 1000] [--dry-run]

Features:
    - Incremental reindexing (can resume)
    - Progress tracking
    - Batch processing
    - Dry-run mode
    - Detailed logging
"""

import argparse
import asyncio
import logging
from datetime import datetime
from pathlib import Path
import sys
from typing import List, Dict, Any, Optional

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)


class BGEMigrationReindexer:
    """Handles reindexing embeddings from old model to BGE-M3."""

    def __init__(
        self,
        batch_size: int = 100,
        max_files: Optional[int] = None,
        dry_run: bool = False
    ):
        self.batch_size = batch_size
        self.max_files = max_files
        self.dry_run = dry_run
        self.stats = {
            'total': 0,
            'migrated': 0,
            'failed': 0,
            'skipped': 0
        }

    async def connect_db(self):
        """Connect to database."""
        from sqlalchemy.ext.asyncio import create_async_engine, AsyncSession
        from sqlalchemy.orm import sessionmaker
        from sqlalchemy import text
        import os

        db_url = os.getenv(
            'DATABASE_URL',
            'postgresql+asyncpg://localhost/recall_vault'
        )

        self.engine = create_async_engine(db_url, echo=False)
        self.Session = sessionmaker(
            self.engine,
            class_=AsyncSession,
            expire_on_commit=False
        )

        logger.info(f"Connected to database: {db_url}")

    async def get_migration_status(self) -> Dict[str, Any]:
        """Get current migration status."""
        from sqlalchemy import text
        
        async with self.Session() as session:
            result = await session.execute(text("""
                SELECT
                    COUNT(*) FILTER (WHERE dimension = 768) as old_count,
                    COUNT(*) FILTER (WHERE dimension = 1024) as new_count,
                    COUNT(*) as total_count
                FROM text_embeddings
            """))

            row = result.fetchone()

            return {
                'old_model_count': row[0],
                'new_model_count': row[1],
                'total_count': row[2],
                'migration_percentage': (row[1] / row[2] * 100) if row[2] > 0 else 0
            }

    async def get_files_to_migrate(self, limit: Optional[int] = None) -> List[Dict[str, Any]]:
        """Get files that need to be migrated."""
        from sqlalchemy import text
        
        async with self.Session() as session:
            query_str = """
                SELECT
                    f.id as file_id,
                    f.file_path,
                    f.mime_type,
                    tc.content,
                    te.id as embedding_id,
                    te.dimension as current_dimension
                FROM files f
                JOIN text_embeddings te ON te.file_id = f.id
                LEFT JOIN text_content tc ON tc.file_id = f.id
                WHERE te.dimension = 768
                    AND te.model_version = 'all-mpnet-base-v2'
                ORDER BY f.created_at
            """

            if limit:
                query_str += " LIMIT :limit"
                result = await session.execute(text(query_str), {"limit": limit})
            else:
                result = await session.execute(text(query_str))
                
            rows = result.fetchall()

            return [
                {
                    'file_id': row[0],
                    'file_path': row[1],
                    'mime_type': row[2],
                    'content': row[3],
                    'embedding_id': row[4],
                    'current_dimension': row[5]
                }
                for row in rows
            ]

    async def reindex_batch(self, files: List[Dict[str, Any]]) -> None:
        """Reindex a batch of files."""
        from sqlalchemy import text
        from services.embeddings.service import EmbeddingService

        embedding_service = EmbeddingService()

        for file_info in files:
            try:
                file_path = file_info['file_path']
                file_id = file_info['file_id']

                logger.info(f"Reindexing: {file_path}")

                if self.dry_run:
                    logger.info(f"[DRY RUN] Would reindex {file_path}")
                    self.stats['migrated'] += 1
                    continue

                # Check if file exists
                if not Path(file_path).exists():
                    logger.warning(f"File not found: {file_path}")
                    self.stats['skipped'] += 1
                    continue

                # Generate new embedding with BGE-M3
                embedding, text_content = await embedding_service.generate_embedding(
                    file_path,
                    file_info['mime_type']
                )

                # Verify dimension
                if len(embedding) != 1024:
                    logger.error(f"Invalid embedding dimension: {len(embedding)}")
                    self.stats['failed'] += 1
                    continue

                # Update database
                async with self.Session() as session:
                    await session.execute(text("""
                        UPDATE text_embeddings
                        SET
                            embedding = :embedding,
                            dimension = 1024,
                            model_version = 'bge-m3',
                            updated_at = NOW()
                        WHERE id = :embedding_id
                    """), {
                        'embedding': embedding,
                        'embedding_id': file_info['embedding_id']
                    })

                    # Track progress
                    await session.execute(text("""
                        INSERT INTO embedding_migration_progress
                            (file_id, old_model, new_model, status)
                        VALUES (:file_id, 'all-mpnet-base-v2', 'bge-m3', 'completed')
                        ON CONFLICT (file_id)
                        DO UPDATE SET
                            status = 'completed',
                            migrated_at = NOW()
                    """), {'file_id': file_id})

                    await session.commit()

                self.stats['migrated'] += 1
                logger.info(f"✓ Migrated {file_path} ({self.stats['migrated']}/{self.stats['total']})")

            except Exception as e:
                logger.error(f"Failed to reindex {file_info.get('file_path', 'unknown')}: {e}")
                self.stats['failed'] += 1

    async def run(self) -> None:
        """Run the reindexing process."""
        logger.info("=" * 70)
        logger.info("BGE-M3 Reindexing Tool")
        logger.info("=" * 70)

        if self.dry_run:
            logger.info("Running in DRY RUN mode - no changes will be made")

        await self.connect_db()

        # Get current status
        status = await self.get_migration_status()
        logger.info(f"\nCurrent status:")
        logger.info(f"  Old model (768-dim): {status['old_model_count']}")
        logger.info(f"  New model (1024-dim): {status['new_model_count']}")
        logger.info(f"  Migration progress: {status['migration_percentage']:.1f}%")

        if status['old_model_count'] == 0:
            logger.info("\n✓ All embeddings already migrated to BGE-M3!")
            return

        # Get files to migrate
        limit = min(self.max_files, status['old_model_count']) if self.max_files else status['old_model_count']

        logger.info(f"\nFetching files to migrate (limit: {limit})...")
        files = await self.get_files_to_migrate(limit)

        if not files:
            logger.info("No files to migrate")
            return

        self.stats['total'] = len(files)
        logger.info(f"Found {len(files)} files to reindex")

        # Process in batches
        for i in range(0, len(files), self.batch_size):
            batch = files[i:i + self.batch_size]
            batch_num = i // self.batch_size + 1
            total_batches = (len(files) + self.batch_size - 1) // self.batch_size

            logger.info(f"\n--- Batch {batch_num}/{total_batches} ---")
            await self.reindex_batch(batch)

            # Show progress
            if self.stats['total'] > 0:
                progress = (self.stats['migrated'] + self.stats['failed']) / self.stats['total'] * 100
                logger.info(f"Progress: {progress:.1f}%")

        # Final stats
        logger.info("\n" + "=" * 70)
        logger.info("Reindexing Complete")
        logger.info("=" * 70)
        logger.info(f"Total processed: {self.stats['total']}")
        logger.info(f"Migrated: {self.stats['migrated']}")
        logger.info(f"Failed: {self.stats['failed']}")
        logger.info(f"Skipped: {self.stats['skipped']}")

        # Get final status
        final_status = await self.get_migration_status()
        logger.info(f"\nFinal migration progress: {final_status['migration_percentage']:.1f}%")


def main():
    parser = argparse.ArgumentParser(
        description="Reindex embeddings to BGE-M3 (1024-dim)",
        formatter_class=argparse.RawDescriptionHelpFormatter
    )

    parser.add_argument(
        "--batch-size",
        type=int,
        default=100,
        help="Number of files to process in each batch (default: 100)"
    )

    parser.add_argument(
        "--max-files",
        type=int,
        default=None,
        help="Maximum number of files to reindex (default: all)"
    )

    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Simulate reindexing without making changes"
    )

    args = parser.parse_args()

    reindexer = BGEMigrationReindexer(
        batch_size=args.batch_size,
        max_files=args.max_files,
        dry_run=args.dry_run
    )

    asyncio.run(reindexer.run())


if __name__ == "__main__":
    main()
