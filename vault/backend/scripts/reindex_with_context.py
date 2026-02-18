#!/usr/bin/env python3
"""
Script: Re-index Documents with Contextual Retrieval

Re-indexes all existing documents in the vault with contextual retrieval enabled.
This applies Anthropic's Contextual Retrieval technique to improve RAG accuracy.

Usage:
    python reindex_with_context.py [options]

Options:
    --dry-run           Show what would be re-indexed without actually doing it
    --file-id UUID      Re-index only a specific file
    --limit N           Limit to N files (useful for testing)
    --status STATUS     Only re-index files with specific status (e.g., 'indexed')
    --force             Force re-index even if already has context

Example:
    # Re-index all documents
    python reindex_with_context.py

    # Dry run to see what would happen
    python reindex_with_context.py --dry-run

    # Re-index only 10 files for testing
    python reindex_with_context.py --limit 10

    # Re-index a specific file
    python reindex_with_context.py --file-id <uuid>
"""

import asyncio
import argparse
import sys
import logging
from pathlib import Path
from typing import Optional, List
from uuid import UUID

sys.path.insert(0, str(Path(__file__).parent.parent))

from sqlalchemy import select, func
from sqlalchemy.ext.asyncio import AsyncSession

from src.db.session import get_async_session
from src.models import File
from src.services.indexing import IndexingService
from src.config.settings import get_settings

logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)


class ReindexProgress:
    """Track and display re-indexing progress."""

    def __init__(self, total: int):
        self.total = total
        self.processed = 0
        self.successful = 0
        self.failed = 0
        self.skipped = 0

    def update(self, success: bool, skipped: bool = False):
        """Update progress counters."""
        self.processed += 1
        if skipped:
            self.skipped += 1
        elif success:
            self.successful += 1
        else:
            self.failed += 1

    def print_progress(self):
        """Print current progress."""
        pct = (self.processed / self.total * 100) if self.total > 0 else 0
        logger.info(
            f"Progress: {self.processed}/{self.total} ({pct:.1f}%) | "
            f"Success: {self.successful} | Failed: {self.failed} | Skipped: {self.skipped}"
        )

    def print_summary(self):
        """Print final summary."""
        logger.info("=" * 60)
        logger.info("Re-indexing Summary:")
        logger.info(f"  Total files: {self.total}")
        logger.info(f"  Successfully re-indexed: {self.successful}")
        logger.info(f"  Failed: {self.failed}")
        logger.info(f"  Skipped: {self.skipped}")
        logger.info("=" * 60)


async def get_files_to_reindex(
    db_session: AsyncSession,
    file_id: Optional[UUID] = None,
    limit: Optional[int] = None,
    status: Optional[str] = None
) -> List[File]:
    """Get list of files to re-index.

    Args:
        db_session: Database session
        file_id: Optional specific file ID to re-index
        limit: Optional limit on number of files
        status: Optional filter by processing status

    Returns:
        List of File objects to re-index
    """
    query = select(File)

    if file_id:
        query = query.where(File.id == file_id)

    if status:
        query = query.where(File.processing_status == status)

    query = query.order_by(File.indexed_at.desc())

    if limit:
        query = query.limit(limit)

    result = await db_session.execute(query)
    files = result.scalars().all()

    return list(files)


async def reindex_file(
    file: File,
    indexing_service: IndexingService,
    db_session: AsyncSession,
    dry_run: bool = False,
    force: bool = False
) -> tuple[bool, str]:
    """Re-index a single file with contextual retrieval.

    Args:
        file: File object to re-index
        indexing_service: IndexingService instance
        db_session: Database session
        dry_run: If True, don't actually re-index
        force: If True, re-index even if already has context

    Returns:
        Tuple of (success: bool, message: str)
    """
    try:
        if dry_run:
            return True, f"Would re-index: {file.filename}"

        logger.info(f"Re-indexing file: {file.filename} (ID: {file.id})")

        success = await indexing_service.reindex_file(file.id, db_session)

        if success:
            return True, f"Successfully re-indexed: {file.filename}"
        else:
            return False, f"Failed to re-index: {file.filename}"

    except Exception as e:
        error_msg = f"Error re-indexing {file.filename}: {str(e)}"
        logger.error(error_msg, exc_info=True)
        return False, error_msg


async def reindex_all(
    file_id: Optional[str] = None,
    limit: Optional[int] = None,
    status: Optional[str] = "indexed",
    dry_run: bool = False,
    force: bool = False
) -> int:
    """Re-index all documents with contextual retrieval.

    Args:
        file_id: Optional specific file UUID to re-index
        limit: Optional limit on number of files
        status: Filter by processing status
        dry_run: If True, show what would be done without doing it
        force: If True, re-index even if already has context

    Returns:
        Exit code (0 = success, 1 = failure)
    """
    settings = get_settings()

    if not settings.enable_contextual_retrieval:
        logger.warning(
            "Contextual retrieval is disabled in settings. "
            "Set ENABLE_CONTEXTUAL_RETRIEVAL=true to enable."
        )
        return 1

    logger.info("Starting re-indexing with contextual retrieval...")
    logger.info(f"Settings: dry_run={dry_run}, limit={limit}, status={status}")

    indexing_service = IndexingService()

    async for db_session in get_async_session():
        try:
            file_uuid = UUID(file_id) if file_id else None

            files = await get_files_to_reindex(
                db_session,
                file_id=file_uuid,
                limit=limit,
                status=status
            )

            if not files:
                logger.info("No files found to re-index.")
                return 0

            logger.info(f"Found {len(files)} files to re-index.")

            if dry_run:
                logger.info("DRY RUN - No actual changes will be made.")

            progress = ReindexProgress(total=len(files))

            for file in files:
                success, message = await reindex_file(
                    file,
                    indexing_service,
                    db_session,
                    dry_run=dry_run,
                    force=force
                )

                logger.info(message)
                progress.update(success=success)

                if progress.processed % 10 == 0:
                    progress.print_progress()

            progress.print_summary()

            if progress.failed > 0:
                logger.warning(f"{progress.failed} files failed to re-index.")
                return 1

            logger.info("Re-indexing completed successfully!")
            return 0

        except Exception as e:
            logger.error(f"Fatal error during re-indexing: {e}", exc_info=True)
            return 1


def main():
    """Main entry point for the re-indexing script."""
    parser = argparse.ArgumentParser(
        description="Re-index documents with contextual retrieval",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )

    parser.add_argument(
        '--dry-run',
        action='store_true',
        help='Show what would be re-indexed without actually doing it'
    )

    parser.add_argument(
        '--file-id',
        type=str,
        help='Re-index only a specific file (UUID)'
    )

    parser.add_argument(
        '--limit',
        type=int,
        help='Limit to N files (useful for testing)'
    )

    parser.add_argument(
        '--status',
        type=str,
        default='indexed',
        help='Only re-index files with specific status (default: indexed)'
    )

    parser.add_argument(
        '--force',
        action='store_true',
        help='Force re-index even if already has context'
    )

    parser.add_argument(
        '--verbose',
        action='store_true',
        help='Enable verbose logging'
    )

    args = parser.parse_args()

    if args.verbose:
        logging.getLogger().setLevel(logging.DEBUG)

    exit_code = asyncio.run(
        reindex_all(
            file_id=args.file_id,
            limit=args.limit,
            status=args.status,
            dry_run=args.dry_run,
            force=args.force
        )
    )

    sys.exit(exit_code)


if __name__ == '__main__':
    main()
