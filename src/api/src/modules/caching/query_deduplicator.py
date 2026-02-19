import asyncio
import logging
import threading
from typing import Any, TypeVar

from .utils import make_cache_key

logger = logging.getLogger(__name__)

T = TypeVar("T")


class QueryDeduplicator:
    def __init__(self):
        self._in_flight: dict[str, asyncio.Task] = {}
        self._lock = threading.Lock()
        self._dedup_hits = 0
        self._total_queries = 0

    async def deduplicate(
        self,
        key: str,
        coro_factory: callable,
    ) -> Any:
        self._total_queries += 1

        with self._lock:
            if key in self._in_flight:
                self._dedup_hits += 1
                existing_task = self._in_flight[key]
                logger.debug(f"Query deduplication: reusing in-flight task for key: {key[:16]}...")

                try:
                    return await existing_task
                except Exception as e:
                    logger.error(f"In-flight task failed: {e}")
                    raise

        task = asyncio.create_task(coro_factory())

        with self._lock:
            self._in_flight[key] = task

        try:
            result = await task
            return result
        finally:
            with self._lock:
                if key in self._in_flight and self._in_flight[key] == task:
                    del self._in_flight[key]

    async def execute_with_dedup(self, coro_factory: callable, *args: Any, **kwargs: Any) -> Any:
        key = make_cache_key(*args, **kwargs)
        return await self.deduplicate(key, coro_factory)

    def get_stats(self) -> dict[str, Any]:
        with self._lock:
            return {
                "total_queries": self._total_queries,
                "dedup_hits": self._dedup_hits,
                "in_flight": len(self._in_flight),
                "dedup_rate": self._dedup_hits / self._total_queries
                if self._total_queries > 0
                else 0.0,
            }

    def clear(self) -> None:
        with self._lock:
            self._in_flight.clear()
            self._dedup_hits = 0
            self._total_queries = 0
            logger.info("Query deduplicator cleared")
