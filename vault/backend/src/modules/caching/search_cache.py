from collections import OrderedDict
import logging
import threading
import time
from typing import Any

from .utils import CacheStats, make_search_key

logger = logging.getLogger(__name__)


class SearchCache:
    def __init__(self, ttl_seconds: int = 300, max_size: int = 1000):
        self.ttl = ttl_seconds
        self.max_size = max_size

        self._cache: OrderedDict[str, tuple[list[Any], float]] = OrderedDict()
        self._lock = threading.RLock()

        self._hits = 0
        self._misses = 0
        self._evictions = 0
        self._expirations = 0

    def get(self, query: str, params: dict[str, Any]) -> list[Any] | None:
        key = make_search_key(query, params)

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
                logger.debug("Search cache entry expired for query")

            self._misses += 1
            return None

    def put(self, query: str, params: dict[str, Any], results: list[Any]) -> None:
        key = make_search_key(query, params)
        timestamp = time.time()

        with self._lock:
            if key in self._cache:
                self._cache.move_to_end(key)
            elif len(self._cache) >= self.max_size:
                self._evict_oldest()

            self._cache[key] = (results, timestamp)

    def _evict_oldest(self) -> None:
        if self._cache:
            evicted_key = next(iter(self._cache))
            del self._cache[evicted_key]
            self._evictions += 1
            logger.debug(f"Evicted oldest search cache entry (total evictions: {self._evictions})")

    def invalidate_all(self) -> None:
        with self._lock:
            count = len(self._cache)
            self._cache.clear()
            logger.info(f"Invalidated all search cache entries ({count} entries)")

    def invalidate_pattern(self, pattern: str) -> int:
        with self._lock:
            keys_to_remove = [key for key in self._cache.keys() if pattern.lower() in key.lower()]

            for key in keys_to_remove:
                del self._cache[key]

            if keys_to_remove:
                logger.info(
                    f"Invalidated {len(keys_to_remove)} search cache entries matching pattern: {pattern}"
                )

            return len(keys_to_remove)

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
                logger.debug(f"Cleaned up {len(expired_keys)} expired search cache entries")

            return len(expired_keys)

    def clear(self) -> None:
        with self._lock:
            self._cache.clear()
            self._hits = 0
            self._misses = 0
            self._evictions = 0
            self._expirations = 0
            logger.info("Search cache cleared")

    def get_stats(self) -> CacheStats:
        with self._lock:
            self.cleanup_expired()

            return CacheStats(
                cache_type="search",
                hits=self._hits,
                misses=self._misses,
                size=len(self._cache),
                max_size=self.max_size,
                evictions=self._evictions + self._expirations,
                memory_bytes=0,
            )
