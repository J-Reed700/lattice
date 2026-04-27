import asyncio
from pathlib import Path
import tempfile
import time

import numpy as np
import pytest

from src.modules.caching import (
    EmbeddingCache,
    QueryDeduplicator,
    SearchCache,
    VectorSearchCache,
)


class TestEmbeddingCacheBenchmark:
    def test_embedding_cache_performance(self):
        cache_file = Path(tempfile.gettempdir()) / "test_embedding_cache.pkl"
        cache = EmbeddingCache(max_size=10000, cache_file=cache_file, embedding_dim=768)

        test_texts = [f"This is test text number {i}" for i in range(1000)]
        embeddings = [np.random.rand(768).astype(np.float32) for _ in range(1000)]

        start_time = time.time()
        for text, embedding in zip(test_texts, embeddings, strict=False):
            cache.put(text, "test-model", embedding)
        write_time = time.time() - start_time

        start_time = time.time()
        hits = 0
        for text in test_texts:
            result = cache.get(text, "test-model")
            if result is not None:
                hits += 1
        read_time = time.time() - start_time

        stats = cache.get_stats()

        print("\n=== Embedding Cache Benchmark ===")
        print(f"Write time for 1000 embeddings: {write_time:.3f}s ({1000/write_time:.0f} ops/sec)")
        print(f"Read time for 1000 lookups: {read_time:.3f}s ({1000/read_time:.0f} ops/sec)")
        print(f"Hit rate: {stats.hit_rate:.2%}")
        print(f"Cache size: {stats.size}")
        print(f"Memory usage: {stats.memory_bytes / (1024 * 1024):.2f} MB")

        assert hits == 1000
        assert stats.hit_rate == 1.0
        assert write_time < 1.0
        assert read_time < 0.5

        if cache_file.exists():
            cache_file.unlink()

    def test_embedding_cache_lru_eviction(self):
        cache = EmbeddingCache(max_size=100, embedding_dim=768)

        for i in range(150):
            text = f"Text {i}"
            embedding = np.random.rand(768).astype(np.float32)
            cache.put(text, "test-model", embedding)

        stats = cache.get_stats()

        print("\n=== LRU Eviction Test ===")
        print(f"Cache size: {stats.size}")
        print(f"Evictions: {stats.evictions}")
        print(f"Max size: {stats.max_size}")

        assert stats.size == 100
        assert stats.evictions == 50

    def test_embedding_cache_persistence(self):
        cache_file = Path(tempfile.gettempdir()) / "test_persist_cache.pkl"

        cache1 = EmbeddingCache(max_size=100, cache_file=cache_file, embedding_dim=768)
        for i in range(50):
            text = f"Persistent text {i}"
            embedding = np.random.rand(768).astype(np.float32)
            cache1.put(text, "test-model", embedding)

        cache1.persist()

        cache2 = EmbeddingCache(max_size=100, cache_file=cache_file, embedding_dim=768)

        hits = 0
        for i in range(50):
            text = f"Persistent text {i}"
            result = cache2.get(text, "test-model")
            if result is not None:
                hits += 1

        print("\n=== Persistence Test ===")
        print(f"Loaded embeddings: {hits}/50")

        assert hits == 50

        if cache_file.exists():
            cache_file.unlink()


class TestSearchCacheBenchmark:
    @pytest.mark.asyncio()
    async def test_search_cache_performance(self):
        cache = SearchCache(ttl_seconds=300, max_size=1000)

        queries = [f"search query {i}" for i in range(500)]
        results = [[{"id": j, "score": 0.9} for j in range(10)] for _ in range(500)]

        start_time = time.time()
        for query, result in zip(queries, results, strict=False):
            cache.put(query, {"limit": 10}, result)
        write_time = time.time() - start_time

        start_time = time.time()
        hits = 0
        for query in queries:
            result = cache.get(query, {"limit": 10})
            if result is not None:
                hits += 1
        read_time = time.time() - start_time

        stats = cache.get_stats()

        print("\n=== Search Cache Benchmark ===")
        print(f"Write time for 500 searches: {write_time:.3f}s ({500/write_time:.0f} ops/sec)")
        print(f"Read time for 500 lookups: {read_time:.3f}s ({500/read_time:.0f} ops/sec)")
        print(f"Hit rate: {stats.hit_rate:.2%}")
        print(f"Cache size: {stats.size}")

        assert hits == 500
        assert stats.hit_rate == 1.0

    @pytest.mark.asyncio()
    async def test_search_cache_ttl(self):
        cache = SearchCache(ttl_seconds=1, max_size=100)

        cache.put("test query", {"limit": 10}, [{"id": 1}])

        result1 = cache.get("test query", {"limit": 10})
        assert result1 is not None

        await asyncio.sleep(1.5)

        result2 = cache.get("test query", {"limit": 10})
        assert result2 is None

        print("\n=== TTL Test ===")
        print("TTL expiration working correctly")


class TestVectorSearchCacheBenchmark:
    @pytest.mark.asyncio()
    async def test_vector_search_cache_performance(self):
        cache = VectorSearchCache(ttl_seconds=300, max_size=500)

        query_embeddings = [np.random.rand(768).tolist() for _ in range(200)]
        results = [[{"id": j, "score": 0.9} for j in range(10)] for _ in range(200)]

        start_time = time.time()
        for embedding, result in zip(query_embeddings, results, strict=False):
            cache.put(
                embedding,
                limit=10,
                threshold=0.7,
                include_metadata=True,
                preview_length=None,
                results=result,
            )
        write_time = time.time() - start_time

        start_time = time.time()
        hits = 0
        for embedding in query_embeddings:
            result = cache.get(
                embedding, limit=10, threshold=0.7, include_metadata=True, preview_length=None
            )
            if result is not None:
                hits += 1
        read_time = time.time() - start_time

        stats = cache.get_stats()

        print("\n=== Vector Search Cache Benchmark ===")
        print(
            f"Write time for 200 vector searches: {write_time:.3f}s ({200/write_time:.0f} ops/sec)"
        )
        print(f"Read time for 200 lookups: {read_time:.3f}s ({200/read_time:.0f} ops/sec)")
        print(f"Hit rate: {stats.hit_rate:.2%}")
        print(f"Cache size: {stats.size}")

        assert hits == 200
        assert stats.hit_rate == 1.0


class TestQueryDeduplicatorBenchmark:
    @pytest.mark.asyncio()
    async def test_query_deduplication(self):
        deduplicator = QueryDeduplicator()

        async def slow_query():
            await asyncio.sleep(0.1)
            return {"result": "data"}

        start_time = time.time()
        tasks = [
            deduplicator.execute_with_dedup(slow_query, "query_1"),
            deduplicator.execute_with_dedup(slow_query, "query_1"),
            deduplicator.execute_with_dedup(slow_query, "query_1"),
            deduplicator.execute_with_dedup(slow_query, "query_2"),
            deduplicator.execute_with_dedup(slow_query, "query_2"),
        ]
        results = await asyncio.gather(*tasks)
        total_time = time.time() - start_time

        stats = deduplicator.get_stats()

        print("\n=== Query Deduplication Benchmark ===")
        print(f"Total time for 5 queries (3 duplicate + 2 unique): {total_time:.3f}s")
        print("Expected time without dedup: ~0.5s, with dedup: ~0.2s")
        print(f"Total queries: {stats['total_queries']}")
        print(f"Dedup hits: {stats['dedup_hits']}")
        print(f"Dedup rate: {stats['dedup_rate']:.2%}")

        assert len(results) == 5
        assert total_time < 0.3
        assert stats["dedup_hits"] == 3
        assert stats["dedup_rate"] == 0.6


class TestIntegratedCacheBenchmark:
    @pytest.mark.asyncio()
    async def test_full_search_pipeline_with_cache(self):
        from src.modules.caching import CacheMonitor
        from src.modules.embedding_generator.text_embedder import TextEmbedder

        embedder = TextEmbedder(enable_cache=True, cache_size=1000)
        monitor = CacheMonitor(embedder=embedder)

        test_texts = [
            "Machine learning is transforming technology",
            "Deep learning powers modern AI systems",
            "Neural networks learn from data",
        ] * 10

        print("\n=== Full Pipeline Benchmark (with cache) ===")

        start_time = time.time()
        for i, text in enumerate(test_texts):
            embedding, chunks = await embedder.embed(text)
            if i % 10 == 0:
                stats = monitor.collect_stats()
                if stats.embedding_cache:
                    print(
                        f"After {i} embeddings: Hit rate = {stats.embedding_cache.get('hit_rate', 0):.2%}"
                    )

        total_time = time.time() - start_time

        final_stats = monitor.get_summary()

        print(f"Total time: {total_time:.3f}s ({len(test_texts)/total_time:.0f} ops/sec)")
        print(f"Overall hit rate: {final_stats['overall_hit_rate']:.2%}")
        print(f"Total memory: {final_stats['total_memory_mb']:.2f} MB")

        assert final_stats["overall_hit_rate"] > 0.6


if __name__ == "__main__":
    pytest.main([__file__, "-v", "-s"])
