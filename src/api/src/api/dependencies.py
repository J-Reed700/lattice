from __future__ import annotations

import asyncio
from collections.abc import AsyncGenerator
import contextlib
from functools import lru_cache

from fastapi import Depends
from sqlalchemy.ext.asyncio import AsyncSession

from src.config import Settings, get_settings
from src.db import get_session as db_get_session
from src.modules.search_engine import SearchService, UnifiedSearchService, VectorSearchService
from src.modules.search_engine.bm25 import BM25SearchEngine
from src.modules.search_engine.hybrid import HybridSearchEngine
from src.services.indexing_batch import BatchIndexingService
from src.services.storage import StorageService


async def get_db() -> AsyncGenerator[AsyncSession, None]:
    """
    Database session dependency.

    Yields an async database session that is automatically
    closed after the request completes.

    Usage:
        @app.get("/items")
        async def get_items(db: AsyncSession = Depends(get_db)):
            ...
    """
    async for session in db_get_session():
        yield session


@lru_cache
def get_settings_dep() -> Settings:
    """
    Settings dependency.

    Returns the application settings singleton.

    Usage:
        @app.get("/config")
        async def get_config(settings: Settings = Depends(get_settings_dep)):
            ...
    """
    return get_settings()


async def get_search_service(session: AsyncSession = Depends(get_db)) -> SearchService:
    """
    Search service dependency.

    Returns an initialized SearchService instance.

    Usage:
        @app.get("/search")
        async def search(
            service: SearchService = Depends(get_search_service)
        ):
            ...
    """
    return SearchService(session)


async def get_indexing_service(session: AsyncSession = Depends(get_db)) -> BatchIndexingService:
    """
    Indexing service dependency.

    Returns an initialized BatchIndexingService instance for high-performance indexing.
    Performance optimization: BatchIndexingService provides 5-10x speedup over sequential indexing
    by using batch database operations and parallel embedding generation.

    Usage:
        @app.post("/index")
        async def index_file(
            service: BatchIndexingService = Depends(get_indexing_service)
        ):
            ...
    """
    return BatchIndexingService()


async def get_storage_service() -> StorageService:
    """
    Storage service dependency.

    Returns an initialized StorageService instance.

    Usage:
        @app.post("/upload")
        async def upload_file(
            service: StorageService = Depends(get_storage_service)
        ):
            ...
    """
    settings = get_settings()
    return StorageService(settings)


class SearchEngineCache:
    """Thread-safe cache for search engine instances.

    Uses async locks to prevent race conditions during initialization
    of singleton search engine instances.
    """

    def __init__(self):
        self._bm25_engine: BM25SearchEngine | None = None
        self._hybrid_engine: HybridSearchEngine | None = None
        self._bm25_lock = asyncio.Lock()
        self._hybrid_lock = asyncio.Lock()

    async def get_bm25_engine(self, session: AsyncSession, settings: Settings) -> BM25SearchEngine:
        """Get or create BM25 search engine instance."""
        if self._bm25_engine is None:
            async with self._bm25_lock:
                # Double-check inside lock to prevent race condition
                if self._bm25_engine is None:
                    self._bm25_engine = BM25SearchEngine(
                        db_session=session, top_k=settings.bm25_top_k
                    )
                    if settings.bm25_corpus_auto_build:
                        with contextlib.suppress(Exception):
                            await self._bm25_engine.build_corpus()
        return self._bm25_engine

    async def get_hybrid_engine(
        self, session: AsyncSession, settings: Settings
    ) -> HybridSearchEngine:
        """Get or create hybrid search engine instance."""
        if self._hybrid_engine is None:
            async with self._hybrid_lock:
                # Double-check inside lock to prevent race condition
                if self._hybrid_engine is None:
                    vector_service = VectorSearchService(session)
                    bm25_engine = await self.get_bm25_engine(session, settings)

                    self._hybrid_engine = HybridSearchEngine(
                        vector_engine=vector_service,
                        bm25_engine=bm25_engine,
                        default_strategy=settings.hybrid_default_strategy,
                        default_vector_weight=settings.hybrid_vector_weight,
                        default_bm25_weight=settings.hybrid_bm25_weight,
                        rrf_k=settings.hybrid_rrf_k,
                    )
        return self._hybrid_engine


# Global singleton instance with proper thread-safe initialization
_search_engine_cache = SearchEngineCache()


async def get_bm25_engine(
    session: AsyncSession = Depends(get_db),
    settings: Settings = Depends(get_settings_dep),
) -> BM25SearchEngine:
    """
    BM25 search engine dependency.

    Returns a singleton BM25SearchEngine instance with thread-safe initialization.

    Usage:
        @app.get("/search")
        async def search(
            engine: BM25SearchEngine = Depends(get_bm25_engine)
        ):
            ...
    """
    return await _search_engine_cache.get_bm25_engine(session, settings)


async def get_hybrid_engine(
    session: AsyncSession = Depends(get_db),
    settings: Settings = Depends(get_settings_dep),
) -> HybridSearchEngine:
    """
    Hybrid search engine dependency.

    Returns a singleton HybridSearchEngine instance with thread-safe initialization.

    Usage:
        @app.get("/search")
        async def search(
            engine: HybridSearchEngine = Depends(get_hybrid_engine)
        ):
            ...
    """
    return await _search_engine_cache.get_hybrid_engine(session, settings)


async def get_unified_search_service(
    session: AsyncSession = Depends(get_db),
    bm25_engine: BM25SearchEngine = Depends(get_bm25_engine),
    hybrid_engine: HybridSearchEngine = Depends(get_hybrid_engine),
) -> UnifiedSearchService:
    """
    Unified search service dependency.

    Returns an initialized UnifiedSearchService that provides a facade
    for all search modes (BM25, hybrid, vector, text).

    Usage:
        @app.post("/search")
        async def search(
            service: UnifiedSearchService = Depends(get_unified_search_service)
        ):
            ...
    """
    return UnifiedSearchService(
        session=session,
        bm25_engine=bm25_engine,
        hybrid_engine=hybrid_engine,
    )
