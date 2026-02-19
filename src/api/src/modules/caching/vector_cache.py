from collections import OrderedDict
import logging
import threading
import time
from typing import Any

from .utils import CacheStats, make_cache_key

logger = logging.getLogger(__name__)


class VectorSearchCache:
    def __init__(self, ttl_seconds: int = 300, max_size: int = 500):
        self.ttl = ttl_seconds
        self.max_size = max_size

        self._cache: OrderedDict[str, tuple[list[Any], float]] = OrderedDict()
        self._lock = threading.RLock()

        self._hits = 0
        self._misses = 0
        self._evictions = 0
        self._expirations = 0

    def get(
        self,
        query_embedding: list[float],
        limit: int,
        threshold: float,
        include_metadata: bool,
        preview_length: int | None = None,
    ) -> list[Any] | None:
        key = self._make_key(query_embedding, limit, threshold, include_metadata, preview_length)

        with self._lock:
            if key in self._cache:
                results, timestamp = self._cache[key]
                current_time = time.time()

                if current_time - timestamp < self.ttl:
                    self._hits += 1
                    self._cache.move_to_end(key)
                    return results

                del self._cache[key]
                self._expirations += 1

            self._misses += 1
            return None

    def put(
        self,
        query_embedding: list[float],
        limit: int,
        threshold: float,
        include_metadata: bool,
        preview_length: int | None,
        results: list[Any],
    ) -> None:
        key = self._make_key(query_embedding, limit, threshold, include_metadata, preview_length)
        timestamp = time.time()

        with self._lock:
            if key in self._cache:
                self._cache.move_to_end(key)
            elif len(self._cache) >= self.max_size:
                self._evict_oldest()

            self._cache[key] = (results, timestamp)

    def _make_key(
        self,
        query_embedding: list[float],
        limit: int,
        threshold: float,
        include_metadata: bool,
        preview_length: int | None,
    ) -> str:
        embedding_hash = hash(tuple(query_embedding[:10]))
        return make_cache_key(
            embedding_hash,
            limit=limit,
            threshold=threshold,
            metadata=include_metadata,
            preview=preview_length,
        )

    def _evict_oldest(self) -> None:
        if self._cache:
            evicted_key = next(iter(self._cache))
            del self._cache[evicted_key]
            self._evictions += 1
            logger.debug(
                f"Evicted oldest vector search cache entry (total evictions: {self._evictions})"
            )

    def invalidate_all(self) -> None:
        with self._lock:
            count = len(self._cache)
            self._cache.clear()
            logger.info(f"Invalidated all vector search cache entries ({count} entries)")

    def cleanup_expired(self) -> int:
        with self._lock:
            current_time = time.time()
            expired_keys = [
                key
                for key, (_, timestamp) in self._cache.items()
                if current_time - timestamp >= self.ttl
            ]

            for key in expired_keys:
                del self._cache[key]
                self._expirations += 1

            if expired_keys:
                logger.debug(f"Cleaned up {len(expired_keys)} expired vector search cache entries")

            return len(expired_keys)

    def clear(self) -> None:
        with self._lock:
            self._cache.clear()
            self._hits = 0
            self._misses = 0
            self._evictions = 0
            self._expirations = 0
            logger.info("Vector search cache cleared")

    def get_stats(self) -> CacheStats:
        with self._lock:
            self.cleanup_expired()

            return CacheStats(
                cache_type="vector_search",
                hits=self._hits,
                misses=self._misses,
                size=len(self._cache),
                max_size=self.max_size,
                evictions=self._evictions + self._expirations,
                memory_bytes=0,
            )
