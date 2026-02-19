"""Service layer exports with lazy imports."""

from __future__ import annotations

from typing import Any

__all__ = [
    "BackgroundTaskQueue",
    "IndexingService",
    "IndexingTask",
    "SearchResult",
    "SearchResults",
    "SearchService",
    "StorageService",
    "WatchService",
]


def __getattr__(name: str) -> Any:
    if name in {"BackgroundTaskQueue", "IndexingTask"}:
        from .queue import BackgroundTaskQueue, IndexingTask

        return {"BackgroundTaskQueue": BackgroundTaskQueue, "IndexingTask": IndexingTask}[name]

    if name == "StorageService":
        from .storage import StorageService

        return StorageService

    if name == "WatchService":
        from .watch import WatchService

        return WatchService

    if name == "IndexingService":
        from .indexing import IndexingService

        return IndexingService

    if name == "SearchService":
        from src.modules.search_engine.db_service import SearchService

        return SearchService

    if name in {"SearchResult", "SearchResults"}:
        from src.modules.search_engine.db_results import SearchResult, SearchResults

        return {"SearchResult": SearchResult, "SearchResults": SearchResults}[name]

    raise AttributeError(f"module 'src.services' has no attribute '{name}'")
