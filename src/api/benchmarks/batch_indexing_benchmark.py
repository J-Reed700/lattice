"""
Batch Indexing Performance Benchmark

This script measures the performance improvements from batch indexing
optimizations and generates a detailed comparison report.

It compares:
1. Original indexing (individual inserts)
2. Batch indexing (batched inserts per file)
3. Parallel batch indexing (concurrent + batched)

Expected Results:
- Batch indexing: 5-10x faster than original
- Parallel batch: 10-20x faster than original (with 4 workers)

Usage:
    python benchmarks/batch_indexing_benchmark.py
"""

import asyncio
import sys
import time
import random
import string
from pathlib import Path
from typing import List, Dict, Any
from uuid import uuid4
from datetime import datetime

sys.path.insert(0, str(Path(__file__).parent.parent))

from sqlalchemy.ext.asyncio import create_async_engine, AsyncSession
from sqlalchemy.orm import sessionmaker

from src.config.settings import get_settings
from src.models import Base, File
from src.services.indexing import IndexingService
from src.services.indexing_batch import BatchIndexingService
from src.services.indexing_parallel import ParallelIndexingService


class BenchmarkData:
    """Generate test data for benchmarking."""

    @staticmethod
    def generate_test_text(size: int = 5000) -> str:
        """Generate random text content.

        Args:
            size: Number of words to generate

        Returns:
            Random text string
        """
        words = [''.join(random.choices(string.ascii_lowercase, k=random.randint(3, 10)))
                 for _ in range(size)]
        return ' '.join(words)

    @staticmethod
    async def create_test_files(
        session: AsyncSession,
        num_files: int = 10,
        base_path: str = "/tmp/benchmark"
    ) -> List[File]:
        """Create test file records in database.

        Args:
            session: Database session
            num_files: Number of test files to create
            base_path: Base path for test files

        Returns:
            List of created File objects
        """
        files = []

        for i in range(num_files):
            file_path = Path(base_path) / f"test_document_{i}.txt"
            file_path.parent.mkdir(parents=True, exist_ok=True)

            content = BenchmarkData.generate_test_text(5000)
            file_path.write_text(content)

            file = File(
                id=uuid4(),
                path=str(file_path),
                name=f"test_document_{i}.txt",
                size=len(content),
                mime_type="text/plain",
                processing_status="pending"
            )
            session.add(file)
            files.append(file)

        await session.commit()
        return files


class IndexingBenchmark:
    """Benchmark indexing performance."""

    def __init__(self):
        """Initialize benchmark."""
        self.settings = get_settings()
        self.engine = create_async_engine(
            self.settings.database_url,
            echo=False,
            pool_pre_ping=True,
        )
        self.async_session = sessionmaker(
            self.engine,
            class_=AsyncSession,
            expire_on_commit=False,
        )

    async def setup_database(self):
        """Set up test database."""
        async with self.engine.begin() as conn:
            await conn.run_sync(Base.metadata.create_all)

    async def cleanup_test_files(self, files: List[File]):
        """Clean up test files.

        Args:
            files: List of file records to clean up
        """
        for file in files:
            try:
                Path(file.path).unlink(missing_ok=True)
            except Exception:
                pass

    async def benchmark_original_indexing(
        self,
        file_ids: List[str],
        num_files: int
    ) -> Dict[str, Any]:
        """Benchmark original indexing service.

        Args:
            file_ids: List of file IDs to index
            num_files: Number of files

        Returns:
            Benchmark results
        """
        print(f"\n{'='*60}")
        print(f"BENCHMARK: Original Indexing (Individual Inserts)")
        print(f"{'='*60}")
        print(f"Files: {num_files}")

        service = IndexingService()
        start_time = time.time()

        async with self.async_session() as session:
            succeeded = 0
            failed = 0

            for i, file_id in enumerate(file_ids):
                try:
                    success = await service.index_file(file_id, session)
                    if success:
                        succeeded += 1
                    else:
                        failed += 1
                except Exception as e:
                    failed += 1
                    print(f"Error indexing file {file_id}: {e}")

                if (i + 1) % 5 == 0:
                    print(f"  Progress: {i+1}/{num_files} files")

            await session.commit()

        duration = time.time() - start_time
        rate = num_files / duration if duration > 0 else 0

        results = {
            'method': 'Original Indexing',
            'files': num_files,
            'succeeded': succeeded,
            'failed': failed,
            'duration_seconds': duration,
            'rate': rate
        }

        print(f"\nResults:")
        print(f"  Duration: {duration:.2f}s")
        print(f"  Rate: {rate:.2f} files/sec")
        print(f"  Succeeded: {succeeded}, Failed: {failed}")

        return results

    async def benchmark_batch_indexing(
        self,
        file_ids: List[str],
        num_files: int
    ) -> Dict[str, Any]:
        """Benchmark batch indexing service.

        Args:
            file_ids: List of file IDs to index
            num_files: Number of files

        Returns:
            Benchmark results
        """
        print(f"\n{'='*60}")
        print(f"BENCHMARK: Batch Indexing (Batched Inserts)")
        print(f"{'='*60}")
        print(f"Files: {num_files}")

        service = BatchIndexingService()
        start_time = time.time()

        async with self.async_session() as session:
            results = await service.index_multiple_files_batch(
                file_ids,
                session,
                progress_callback=lambda cur, total: print(f"  Progress: {cur}/{total} files") if cur % 5 == 0 else None
            )

        duration = time.time() - start_time
        rate = num_files / duration if duration > 0 else 0

        results['method'] = 'Batch Indexing'
        results['duration_seconds'] = duration
        results['rate'] = rate

        print(f"\nResults:")
        print(f"  Duration: {duration:.2f}s")
        print(f"  Rate: {rate:.2f} files/sec")
        print(f"  Succeeded: {results['succeeded']}, Failed: {results['failed']}")

        return results

    async def benchmark_parallel_indexing(
        self,
        file_ids: List[str],
        num_files: int,
        max_workers: int = 4
    ) -> Dict[str, Any]:
        """Benchmark parallel batch indexing service.

        Args:
            file_ids: List of file IDs to index
            num_files: Number of files
            max_workers: Number of parallel workers

        Returns:
            Benchmark results
        """
        print(f"\n{'='*60}")
        print(f"BENCHMARK: Parallel Batch Indexing (Concurrent + Batched)")
        print(f"{'='*60}")
        print(f"Files: {num_files}")
        print(f"Workers: {max_workers}")

        service = ParallelIndexingService(max_workers=max_workers)
        start_time = time.time()

        async with self.async_session() as session:
            results = await service.index_files_parallel(
                file_ids,
                session,
                progress_callback=lambda cur, total, stats: print(
                    f"  Progress: {cur}/{total} files ({stats['rate']:.2f} files/sec)"
                ) if cur % 5 == 0 else None
            )

        duration = time.time() - start_time

        results['method'] = f'Parallel Batch Indexing ({max_workers} workers)'

        print(f"\nResults:")
        print(f"  Duration: {results['duration_seconds']:.2f}s")
        print(f"  Rate: {results['rate']:.2f} files/sec")
        print(f"  Succeeded: {results['succeeded']}, Failed: {results['failed']}")

        return results

    def print_comparison(self, results: List[Dict[str, Any]]):
        """Print comparison table.

        Args:
            results: List of benchmark results
        """
        print(f"\n{'='*80}")
        print(f"PERFORMANCE COMPARISON")
        print(f"{'='*80}")

        baseline = results[0]
        baseline_rate = baseline['rate']

        print(f"\n{'Method':<40} {'Duration':<12} {'Rate':<15} {'Speedup':<10}")
        print(f"{'-'*80}")

        for result in results:
            speedup = result['rate'] / baseline_rate if baseline_rate > 0 else 0
            print(
                f"{result['method']:<40} "
                f"{result['duration_seconds']:>10.2f}s  "
                f"{result['rate']:>10.2f} f/s  "
                f"{speedup:>8.2f}x"
            )

        print(f"\n{'='*80}")
        print("KEY FINDINGS:")
        print(f"{'='*80}")

        if len(results) >= 2:
            batch_speedup = results[1]['rate'] / baseline_rate
            print(f"✓ Batch indexing is {batch_speedup:.1f}x faster than original")

        if len(results) >= 3:
            parallel_speedup = results[2]['rate'] / baseline_rate
            print(f"✓ Parallel batch indexing is {parallel_speedup:.1f}x faster than original")

            parallel_vs_batch = results[2]['rate'] / results[1]['rate']
            print(f"✓ Parallel provides {parallel_vs_batch:.1f}x additional speedup over batch alone")

        print(f"\n{'='*80}")

    async def run_benchmark(self, num_files: int = 20):
        """Run complete benchmark suite.

        Args:
            num_files: Number of test files to use
        """
        print(f"\n{'='*80}")
        print(f"BATCH INDEXING PERFORMANCE BENCHMARK")
        print(f"{'='*80}")
        print(f"Timestamp: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
        print(f"Files to index: {num_files}")
        print(f"{'='*80}")

        await self.setup_database()

        async with self.async_session() as session:
            test_files = await BenchmarkData.create_test_files(
                session,
                num_files=num_files
            )
            file_ids = [file.id for file in test_files]

        all_results = []

        try:
            result1 = await self.benchmark_original_indexing(file_ids[:num_files], num_files)
            all_results.append(result1)

            result2 = await self.benchmark_batch_indexing(file_ids[:num_files], num_files)
            all_results.append(result2)

            result3 = await self.benchmark_parallel_indexing(file_ids[:num_files], num_files, max_workers=4)
            all_results.append(result3)

            self.print_comparison(all_results)

        finally:
            async with self.async_session() as session:
                await self.cleanup_test_files(test_files)

        await self.engine.dispose()


async def main():
    """Main benchmark entry point."""
    benchmark = IndexingBenchmark()

    print("\nRunning benchmark with 20 files...")
    await benchmark.run_benchmark(num_files=20)


if __name__ == "__main__":
    asyncio.run(main())
