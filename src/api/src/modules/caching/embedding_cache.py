import asyncio
from collections import OrderedDict
import logging
from pathlib import Path
import pickle

import numpy as np

from .utils import CacheStats, make_embedding_key

logger = logging.getLogger(__name__)


class EmbeddingCache:
    """Async LRU cache for embeddings with thread-safe operations.

    Performance optimization: Uses asyncio.Lock instead of threading.Lock
    to prevent blocking the async event loop during cache operations.
    """

    def __init__(
        self,
        max_size: int = 10000,
        cache_file: Path | None = None,
        embedding_dim: int = 768,
    ):
        self.max_size = max_size
        self.cache_file = cache_file
        self.embedding_dim = embedding_dim

        self._cache: OrderedDict[str, np.ndarray] = OrderedDict()
        self._lock = asyncio.Lock()  # Performance: Async lock instead of thread lock

        self._hits = 0
        self._misses = 0
        self._evictions = 0

        if cache_file and cache_file.exists():
            self._load_from_disk_sync()

    async def get(self, text: str, model_name: str) -> np.ndarray | None:
        """Get embedding from cache (async)."""
        key = make_embedding_key(text, model_name)

        async with self._lock:
            if key in self._cache:
                self._hits += 1
                self._cache.move_to_end(key)
                return self._cache[key].copy()

            self._misses += 1
            return None

    async def put(self, text: str, model_name: str, embedding: np.ndarray) -> None:
        """Store embedding in cache (async)."""
        if embedding.shape[0] != self.embedding_dim:
            raise ValueError(
                f"Embedding dimension {embedding.shape[0]} doesn't match "
                f"cache dimension {self.embedding_dim}"
            )

        key = make_embedding_key(text, model_name)

        async with self._lock:
            if key in self._cache:
                self._cache.move_to_end(key)
            elif len(self._cache) >= self.max_size:
                await self._evict_lru()

            self._cache[key] = embedding.copy()

    async def _evict_lru(self) -> None:
        """Evict least recently used item (async)."""
        if self._cache:
            evicted_key = next(iter(self._cache))
            del self._cache[evicted_key]
            self._evictions += 1
            logger.debug(f"Evicted LRU embedding (total evictions: {self._evictions})")

    async def clear(self) -> None:
        """Clear all cache entries (async)."""
        async with self._lock:
            self._cache.clear()
            self._hits = 0
            self._misses = 0
            self._evictions = 0
            logger.info("Embedding cache cleared")

    async def get_stats(self) -> CacheStats:
        """Get cache statistics (async)."""
        async with self._lock:
            memory_bytes = sum(embedding.nbytes for embedding in self._cache.values())

            return CacheStats(
                cache_type="embedding",
                hits=self._hits,
                misses=self._misses,
                size=len(self._cache),
                max_size=self.max_size,
                evictions=self._evictions,
                memory_bytes=memory_bytes,
            )

    async def _save_to_disk(self) -> None:
        """Save cache to disk (async)."""
        if not self.cache_file:
            return

        async with self._lock:
            try:
                self.cache_file.parent.mkdir(parents=True, exist_ok=True)

                cache_data = {
                    "embeddings": dict(self._cache),
                    "stats": {
                        "hits": self._hits,
                        "misses": self._misses,
                        "evictions": self._evictions,
                    },
                }

                # Use asyncio.to_thread for I/O operation
                await asyncio.to_thread(self._write_cache_file, cache_data)

                logger.info(f"Saved {len(self._cache)} embeddings to disk: {self.cache_file}")

            except Exception as e:
                logger.error(f"Failed to save embedding cache to disk: {e}", exc_info=True)

    def _write_cache_file(self, cache_data: dict) -> None:
        """Write cache data to file (sync helper)."""
        with open(self.cache_file, "wb") as f:
            pickle.dump(cache_data, f, protocol=pickle.HIGHEST_PROTOCOL)

    def _load_from_disk_sync(self) -> None:
        """Load cache from disk during initialization (sync).

        Note: This is called from __init__ so it must be synchronous.
        The lock is not held during init since no concurrent access is possible.
        """
        if not self.cache_file or not self.cache_file.exists():
            return

        try:
            with open(self.cache_file, "rb") as f:
                cache_data = pickle.load(f)

            loaded_embeddings = cache_data.get("embeddings", {})
            stats = cache_data.get("stats", {})

            valid_count = 0
            for key, embedding in loaded_embeddings.items():
                if isinstance(embedding, np.ndarray) and embedding.shape[0] == self.embedding_dim:
                    self._cache[key] = embedding
                    valid_count += 1

                if len(self._cache) >= self.max_size:
                    break

            self._hits = stats.get("hits", 0)
            self._misses = stats.get("misses", 0)
            self._evictions = stats.get("evictions", 0)

            logger.info(f"Loaded {valid_count} embeddings from disk cache: {self.cache_file}")

        except Exception as e:
            logger.error(f"Failed to load embedding cache from disk: {e}", exc_info=True)
            self._cache.clear()

    async def persist(self) -> None:
        """Persist cache to disk (async)."""
        await self._save_to_disk()

    def _save_to_disk_unsafe(self) -> None:
        if not self.cache_file:
            return

        try:
            self.cache_file.parent.mkdir(parents=True, exist_ok=True)

            cache_data = {
                "embeddings": dict(self._cache),
                "stats": {
                    "hits": self._hits,
                    "misses": self._misses,
                    "evictions": self._evictions,
                },
            }

            with open(self.cache_file, "wb") as f:
                pickle.dump(cache_data, f, protocol=pickle.HIGHEST_PROTOCOL)

            logger.info(f"Saved {len(self._cache)} embeddings to disk: {self.cache_file}")

        except Exception as e:
            logger.error(f"Failed to save embedding cache to disk: {e}", exc_info=True)

    def __del__(self):
        if self.cache_file:
            try:
                self._save_to_disk_unsafe()
            except Exception:
                pass
