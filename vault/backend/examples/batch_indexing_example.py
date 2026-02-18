"""
Batch Indexing Usage Examples

This script demonstrates how to use the batch indexing services
for optimal performance when indexing files.

Examples:
1. Single file batch indexing
2. Multiple files sequential batch indexing
3. Multiple files parallel batch indexing
4. Progress tracking and monitoring
"""

import asyncio
from uuid import UUID
from pathlib import Path
from typing import List

from sqlalchemy.ext.asyncio import create_async_engine, AsyncSession
from sqlalchemy.orm import sessionmaker

from src.config.settings import get_settings
from src.services.indexing import IndexingService
from src.services.indexing_batch import BatchIndexingService
from src.services.indexing_parallel import ParallelIndexingService


async def example_1_single_file_batch():
    """Example 1: Index a single file using batch operations."""
    print("\n" + "="*60)
    print("EXAMPLE 1: Single File Batch Indexing")
    print("="*60)

    settings = get_settings()
    engine = create_async_engine(settings.database_url)
    async_session = sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)

    service = BatchIndexingService()
    file_id = UUID('12345678-1234-1234-1234-123456789abc')

    async with async_session() as session:
        print(f"Indexing file: {file_id}")

        success = await service.index_file_batch(file_id, session)

        if success:
            await session.commit()
            print("✓ File indexed successfully")
        else:
            await session.rollback()
            print("✗ File indexing failed")

    await engine.dispose()


async def example_2_multiple_files_sequential():
    """Example 2: Index multiple files sequentially with batch operations."""
    print("\n" + "="*60)
    print("EXAMPLE 2: Multiple Files Sequential Batch Indexing")
    print("="*60)

    settings = get_settings()
    engine = create_async_engine(settings.database_url)
    async_session = sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)

    file_ids = [
        UUID('12345678-1234-1234-1234-123456789001'),
        UUID('12345678-1234-1234-1234-123456789002'),
        UUID('12345678-1234-1234-1234-123456789003'),
    ]

    service = BatchIndexingService()

    def progress_callback(current: int, total: int):
        print(f"Progress: {current}/{total} files ({current/total*100:.1f}%)")

    async with async_session() as session:
        print(f"Indexing {len(file_ids)} files sequentially...")

        results = await service.index_multiple_files_batch(
            file_ids=file_ids,
            db_session=session,
            progress_callback=progress_callback
        )

        print(f"\nResults:")
        print(f"  Total: {results['total']}")
        print(f"  Succeeded: {results['succeeded']}")
        print(f"  Failed: {results['failed']}")

        if results['errors']:
            print(f"\nErrors:")
            for file_id, error in results['errors']:
                print(f"  {file_id}: {error}")

    await engine.dispose()


async def example_3_multiple_files_parallel():
    """Example 3: Index multiple files in parallel with batch operations."""
    print("\n" + "="*60)
    print("EXAMPLE 3: Multiple Files Parallel Batch Indexing")
    print("="*60)

    settings = get_settings()
    engine = create_async_engine(settings.database_url)
    async_session = sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)

    file_ids = [
        UUID(f'12345678-1234-1234-1234-12345678{i:04d}')
        for i in range(20)
    ]

    max_workers = 4
    service = ParallelIndexingService(max_workers=max_workers)

    def progress_callback(current: int, total: int, stats: dict):
        print(
            f"Progress: {current}/{total} files ({stats['percentage']:.1f}%) - "
            f"Rate: {stats['rate']:.2f} files/sec - "
            f"ETA: {stats['eta_seconds']:.0f}s"
        )

    async with async_session() as session:
        print(f"Indexing {len(file_ids)} files in parallel ({max_workers} workers)...")

        results = await service.index_files_parallel(
            file_ids=file_ids,
            db_session=session,
            progress_callback=progress_callback
        )

        print(f"\nResults:")
        print(f"  Total: {results['total']}")
        print(f"  Succeeded: {results['succeeded']}")
        print(f"  Failed: {results['failed']}")
        print(f"  Duration: {results['duration_seconds']:.2f}s")
        print(f"  Rate: {results['rate']:.2f} files/sec")

        if results['errors']:
            print(f"\nErrors:")
            for file_id, error in results['errors'][:5]:
                print(f"  {file_id}: {error}")
            if len(results['errors']) > 5:
                print(f"  ... and {len(results['errors']) - 5} more")

    await engine.dispose()


async def example_4_comparison():
    """Example 4: Compare original vs batch vs parallel indexing."""
    print("\n" + "="*60)
    print("EXAMPLE 4: Performance Comparison")
    print("="*60)

    settings = get_settings()
    engine = create_async_engine(settings.database_url)
    async_session = sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)

    file_ids = [
        UUID(f'12345678-1234-1234-1234-12345678{i:04d}')
        for i in range(10)
    ]

    import time

    original_service = IndexingService()
    batch_service = BatchIndexingService()
    parallel_service = ParallelIndexingService(max_workers=4)

    async with async_session() as session:
        print("\n1. Original Indexing (Individual Inserts):")
        start = time.time()
        for file_id in file_ids:
            try:
                await original_service.index_file(file_id, session)
            except:
                pass
        await session.commit()
        original_time = time.time() - start
        print(f"   Time: {original_time:.2f}s")

    async with async_session() as session:
        print("\n2. Batch Indexing:")
        start = time.time()
        await batch_service.index_multiple_files_batch(file_ids, session)
        batch_time = time.time() - start
        print(f"   Time: {batch_time:.2f}s")
        print(f"   Speedup: {original_time/batch_time:.1f}x")

    async with async_session() as session:
        print("\n3. Parallel Batch Indexing:")
        start = time.time()
        await parallel_service.index_files_parallel(file_ids, session)
        parallel_time = time.time() - start
        print(f"   Time: {parallel_time:.2f}s")
        print(f"   Speedup: {original_time/parallel_time:.1f}x")

    print("\nComparison:")
    print(f"  Original:  {original_time:.2f}s (baseline)")
    print(f"  Batch:     {batch_time:.2f}s ({original_time/batch_time:.1f}x faster)")
    print(f"  Parallel:  {parallel_time:.2f}s ({original_time/parallel_time:.1f}x faster)")

    await engine.dispose()


async def example_5_error_handling():
    """Example 5: Proper error handling with batch indexing."""
    print("\n" + "="*60)
    print("EXAMPLE 5: Error Handling")
    print("="*60)

    settings = get_settings()
    engine = create_async_engine(settings.database_url)
    async_session = sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)

    file_ids = [
        UUID('12345678-1234-1234-1234-123456789001'),
        UUID('invalid-uuid-will-fail'),
        UUID('12345678-1234-1234-1234-123456789003'),
    ]

    service = ParallelIndexingService(max_workers=2)

    async with async_session() as session:
        try:
            print("Indexing files with error handling...")

            results = await service.index_files_parallel(
                file_ids=file_ids,
                db_session=session
            )

            print(f"\nIndexing completed:")
            print(f"  Succeeded: {results['succeeded']}")
            print(f"  Failed: {results['failed']}")

            if results['errors']:
                print(f"\nErrors encountered:")
                for file_id, error in results['errors']:
                    print(f"  ✗ {file_id}")
                    print(f"    Error: {error[:100]}")

            if results['succeeded'] > 0:
                print(f"\n✓ Successfully indexed {results['succeeded']} files")

        except Exception as e:
            print(f"\n✗ Batch indexing failed: {e}")
            await session.rollback()

    await engine.dispose()


async def example_6_progress_monitoring():
    """Example 6: Advanced progress monitoring."""
    print("\n" + "="*60)
    print("EXAMPLE 6: Advanced Progress Monitoring")
    print("="*60)

    settings = get_settings()
    engine = create_async_engine(settings.database_url)
    async_session = sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)

    file_ids = [UUID(f'12345678-1234-1234-1234-12345678{i:04d}') for i in range(50)]

    service = ParallelIndexingService(max_workers=4)

    class ProgressMonitor:
        def __init__(self):
            self.last_update = 0

        def __call__(self, current: int, total: int, stats: dict):
            if current - self.last_update >= 5 or current == total:
                self.last_update = current

                bar_length = 30
                filled = int(bar_length * current / total)
                bar = '█' * filled + '░' * (bar_length - filled)

                print(
                    f"\r[{bar}] {current}/{total} files "
                    f"({stats['percentage']:.1f}%) - "
                    f"{stats['rate']:.2f} f/s - "
                    f"ETA: {stats['eta_seconds']:.0f}s",
                    end='', flush=True
                )

                if current == total:
                    print()

    monitor = ProgressMonitor()

    async with async_session() as session:
        print(f"Indexing {len(file_ids)} files with progress bar...\n")

        results = await service.index_files_parallel(
            file_ids=file_ids,
            db_session=session,
            progress_callback=monitor
        )

        print(f"\n✓ Completed in {results['duration_seconds']:.2f}s")

    await engine.dispose()


async def main():
    """Run all examples."""
    print("\n" + "="*80)
    print("BATCH INDEXING EXAMPLES")
    print("="*80)

    print("\nThese examples demonstrate batch indexing capabilities.")
    print("Note: Examples use placeholder UUIDs - replace with real file IDs.")

    examples = [
        ("Single File Batch", example_1_single_file_batch),
        ("Multiple Files Sequential", example_2_multiple_files_sequential),
        ("Multiple Files Parallel", example_3_multiple_files_parallel),
        ("Performance Comparison", example_4_comparison),
        ("Error Handling", example_5_error_handling),
        ("Progress Monitoring", example_6_progress_monitoring),
    ]

    for i, (name, func) in enumerate(examples, 1):
        print(f"\n{'='*80}")
        print(f"Running Example {i}: {name}")
        print(f"{'='*80}")

        try:
            await func()
        except Exception as e:
            print(f"\n✗ Example failed: {e}")
            import traceback
            traceback.print_exc()

    print("\n" + "="*80)
    print("All examples completed!")
    print("="*80)


if __name__ == "__main__":
    asyncio.run(main())
