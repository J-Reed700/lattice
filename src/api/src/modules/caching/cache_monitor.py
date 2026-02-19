from dataclasses import asdict, dataclass
from datetime import UTC, datetime
import logging
from typing import Any

logger = logging.getLogger(__name__)


@dataclass
class CacheMonitorStats:
    timestamp: str
    embedding_cache: dict[str, Any] | None = None
    search_cache: dict[str, Any] | None = None
    vector_text_cache: dict[str, Any] | None = None
    vector_image_cache: dict[str, Any] | None = None
    deduplicator: dict[str, Any] | None = None

    def to_dict(self) -> dict[str, Any]:
        return {k: v for k, v in asdict(self).items() if v is not None}

    def log_stats(self, level: int = logging.INFO) -> None:
        stats_dict = self.to_dict()
        logger.log(level, "Cache statistics", extra={"cache_stats": stats_dict})

        if self.embedding_cache:
            logger.log(
                level,
                "embedding_cache",
                extra={
                    "cache_type": "embedding",
                    "hit_rate": self.embedding_cache.get("hit_rate", 0),
                    "size": self.embedding_cache.get("size", 0),
                    "memory_mb": self.embedding_cache.get("memory_mb", 0),
                },
            )

        if self.search_cache:
            logger.log(
                level,
                "search_cache",
                extra={
                    "cache_type": "search",
                    "hit_rate": self.search_cache.get("hit_rate", 0),
                    "size": self.search_cache.get("size", 0),
                },
            )


class CacheMonitor:
    def __init__(self, embedder=None, vector_store=None, search_engine=None):
        self.embedder = embedder
        self.vector_store = vector_store
        self.search_engine = search_engine

    def collect_stats(self) -> CacheMonitorStats:
        stats = CacheMonitorStats(timestamp=datetime.now(UTC).isoformat())

        if self.embedder and hasattr(self.embedder, "get_cache_stats"):
            stats.embedding_cache = self.embedder.get_cache_stats()

        if self.search_engine and hasattr(self.search_engine, "get_cache_stats"):
            engine_stats = self.search_engine.get_cache_stats()
            stats.search_cache = engine_stats.get("search")
            stats.deduplicator = engine_stats.get("deduplicator")

        if self.vector_store and hasattr(self.vector_store, "get_cache_stats"):
            vector_stats = self.vector_store.get_cache_stats()
            stats.vector_text_cache = vector_stats.get("text_search")
            stats.vector_image_cache = vector_stats.get("image_search")

        return stats

    def log_stats(self, level: int = logging.INFO) -> None:
        stats = self.collect_stats()
        stats.log_stats(level)

    def get_summary(self) -> dict[str, Any]:
        stats = self.collect_stats()
        summary = {
            "timestamp": stats.timestamp,
            "total_caches": 0,
            "total_hits": 0,
            "total_misses": 0,
            "total_memory_mb": 0,
            "overall_hit_rate": 0.0,
        }

        for cache_name in [
            "embedding_cache",
            "search_cache",
            "vector_text_cache",
            "vector_image_cache",
        ]:
            cache_stats = getattr(stats, cache_name)
            if cache_stats:
                summary["total_caches"] += 1
                summary["total_hits"] += cache_stats.get("hits", 0)
                summary["total_misses"] += cache_stats.get("misses", 0)
                summary["total_memory_mb"] += cache_stats.get("memory_mb", 0)

        total_requests = summary["total_hits"] + summary["total_misses"]
        if total_requests > 0:
            summary["overall_hit_rate"] = summary["total_hits"] / total_requests

        return summary
