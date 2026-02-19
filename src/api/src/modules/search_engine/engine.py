"""
Search Engine

Unified search interface with semantic, keyword, and hybrid search modes.
"""

import asyncio
import logging
from pathlib import Path
from typing import Any

import aiofiles

from ..caching.query_deduplicator import QueryDeduplicator
from ..caching.search_cache import SearchCache
from .filters import SearchFilters
from .ranker import ResultRanker
from .snippets import SnippetGenerator
from .types import EmbeddingError, InvalidQueryError, SearchError, SearchResult

DEFAULT_LIMIT = 20
MAX_LIMIT = 100


logger = logging.getLogger(__name__)


class SearchEngine:
    """Unified search engine with auto-detection and hybrid search.

    Example:
        >>> engine = SearchEngine(vector_store, embedder, db_session)
        >>> results = await engine.search("machine learning", limit=10)
    """

    _search_cache = None
    _deduplicator = None

    def __init__(
        self, vector_store: Any, embedder: Any, db_session: Any, enable_cache: bool = True
    ):
        """Initialize search engine.

        Args:
            vector_store: Vector database for semantic search
            embedder: Embedding model for query/image encoding
            db_session: SQLAlchemy database session
            enable_cache: Enable caching for search results
        """
        self.vector_store = vector_store
        self.embedder = embedder
        self.db_session = db_session
        self._enable_cache = enable_cache

        if enable_cache:
            if SearchEngine._search_cache is None:
                SearchEngine._search_cache = SearchCache(ttl_seconds=300, max_size=1000)
            if SearchEngine._deduplicator is None:
                SearchEngine._deduplicator = QueryDeduplicator()

            self._cache = SearchEngine._search_cache
            self._deduplicator = SearchEngine._deduplicator
        else:
            self._cache = None
            self._deduplicator = None

    async def search(
        self,
        query: str | bytes,
        mode: str = "auto",
        filters: SearchFilters | None = None,
        limit: int = DEFAULT_LIMIT,
        rerank: bool = True,
        semantic_weight: float = 0.7,
        keyword_weight: float = 0.3,
    ) -> list[SearchResult]:
        """Unified search interface with auto-detection.

        Args:
            query: Text string or image bytes
            mode: Search mode ('auto', 'semantic', 'keyword', 'hybrid')
            filters: Optional SearchFilters for result filtering
            limit: Maximum results to return
            rerank: Apply cross-encoder reranking to improve result quality
            semantic_weight: Weight for semantic search in hybrid mode (0.0-1.0)
            keyword_weight: Weight for keyword search in hybrid mode (0.0-1.0)

        Returns:
            List of SearchResult objects sorted by relevance

        Raises:
            InvalidQueryError: Query validation failed
            SearchError: Search operation failed

        Example:
            >>> results = await engine.search(
            ...     "python programming",
            ...     mode="hybrid",
            ...     filters=SearchFilters(file_types=['.py']),
            ...     limit=20,
            ...     rerank=True,
            ...     semantic_weight=0.7,
            ...     keyword_weight=0.3
            ... )
        """
        limit = min(limit, MAX_LIMIT)

        if not query:
            raise InvalidQueryError("Query cannot be empty")

        if mode == "auto":
            mode = self._detect_query_type(query)

        if self._cache and isinstance(query, str):
            cache_params = {
                "mode": mode,
                "limit": limit,
                "rerank": rerank,
                "semantic_weight": semantic_weight,
                "keyword_weight": keyword_weight,
                "filters": str(filters) if filters else None,
            }
            cached_results = self._cache.get(query, cache_params)
            if cached_results is not None:
                logger.debug(f"Cache hit for search query: {query[:50]}...")
                return cached_results

        try:
            if mode == "semantic":
                results = await self._search_semantic(query, filters, limit)
            elif mode == "keyword":
                results = await self._search_keyword(query, filters, limit)
            elif mode == "hybrid":
                results = await self._search_hybrid(
                    query, filters, limit, rerank, semantic_weight, keyword_weight
                )
            else:
                raise InvalidQueryError(f"Invalid search mode: {mode}")

            if filters:
                results = filters.apply(results)

            for result in results:
                if result.snippet is None:
                    result.snippet = await self._load_snippet(result, query)

            final_results = results[:limit]

            if self._cache and isinstance(query, str):
                cache_params = {
                    "mode": mode,
                    "limit": limit,
                    "rerank": rerank,
                    "semantic_weight": semantic_weight,
                    "keyword_weight": keyword_weight,
                    "filters": str(filters) if filters else None,
                }
                self._cache.put(query, cache_params, final_results)

            return final_results

        except (InvalidQueryError, SearchError, EmbeddingError):
            raise
        except Exception as e:
            logger.exception(
                "Unexpected search error", extra={"mode": mode, "query_type": type(query).__name__}
            )
            raise SearchError(f"Search operation failed: {e}") from e

    async def _search_semantic(
        self, query: str | bytes, filters: SearchFilters | None, limit: int
    ) -> list[SearchResult]:
        """Vector similarity search.

        Args:
            query: Text string or image bytes
            filters: Optional SearchFilters
            limit: Maximum results

        Returns:
            List of SearchResult objects with similarity scores
        """
        try:
            if isinstance(query, bytes):
                query_vector = await self.embedder.encode_image(query)
            else:
                query_vector = await self.embedder.encode_text(query)
        except Exception as e:
            logger.error(
                "Embedding generation failed",
                exc_info=True,
                extra={"query_type": type(query).__name__},
            )
            raise EmbeddingError(f"Failed to generate embedding: {e}") from e

        sql_filters = filters.to_sql_filters() if filters else {}

        vector_results = await self.vector_store.similarity_search(
            query_vector=query_vector, limit=limit * 2, filters=sql_filters
        )

        results = []
        for vr in vector_results:
            result = SearchResult(
                file_id=vr["file_id"],
                file_path=vr["file_path"],
                score=vr["similarity_score"],
                metadata=vr.get("metadata", {}),
            )
            results.append(result)

        return results

    async def _search_keyword(
        self, query: str | bytes, filters: SearchFilters | None, limit: int
    ) -> list[SearchResult]:
        """BM25 keyword search using SQLite FTS5.

        Args:
            query: Search query (must be text)
            filters: Search filters
            limit: Maximum results

        Returns:
            List of SearchResult objects with BM25 scores

        Raises:
            InvalidQueryError: If query is not text
        """
        if isinstance(query, bytes):
            raise InvalidQueryError("Keyword search requires text query")

        if not query.strip():
            raise InvalidQueryError("Query cannot be empty")

        from .bm25 import BM25Scorer

        try:
            bm25_results = await BM25Scorer.search_fts5(
                query=query, db_session=self.db_session, filters={}, limit=limit * 2
            )

            results = []
            for br in bm25_results:
                result = SearchResult(
                    file_id=br["document_id"],
                    file_path=br["file_path"],
                    score=br["bm25_score"],
                    metadata={
                        "raw_bm25": br["raw_bm25"],
                        "search_method": "bm25",
                        "filename": br["filename"],
                        "extension": br["extension"],
                    },
                )
                results.append(result)

            logger.info(f"BM25 keyword search returned {len(results)} results")
            return results

        except Exception as e:
            logger.error(f"BM25 keyword search failed: {e}", exc_info=True)
            raise

    async def _search_hybrid(
        self,
        query: str | bytes,
        filters: SearchFilters | None,
        limit: int,
        rerank: bool = True,
        semantic_weight: float = 0.7,
        keyword_weight: float = 0.3,
    ) -> list[SearchResult]:
        """Combine semantic and keyword search using RRF.

        Args:
            query: Text string or image bytes
            filters: Optional SearchFilters
            limit: Maximum results
            rerank: Apply cross-encoder reranking
            semantic_weight: Weight for semantic search (0.0-1.0)
            keyword_weight: Weight for keyword search (0.0-1.0)

        Returns:
            List of SearchResult objects with RRF scores
        """
        if isinstance(query, bytes):
            logger.warning("Image query in hybrid mode, using semantic search only")
            return await self._search_semantic(query, filters, limit)

        try:
            semantic_results, keyword_results = await asyncio.gather(
                self._search_semantic(query, filters, limit),
                self._search_keyword(query, filters, limit),
                return_exceptions=True,
            )

            result_lists = []

            if isinstance(semantic_results, list):
                result_lists.append(semantic_results)
            else:
                logger.warning(f"Semantic search failed: {semantic_results}")

            if isinstance(keyword_results, list):
                result_lists.append(keyword_results)
            else:
                logger.warning(f"Keyword search failed: {keyword_results}")

            if not result_lists:
                logger.error("Both search methods failed")
                return []

            if len(result_lists) > 1:
                merged = ResultRanker.weighted_reciprocal_rank_fusion(
                    result_lists, [semantic_weight, keyword_weight]
                )
            else:
                merged = result_lists[0]

            merged = ResultRanker.deduplicate(merged)

            if rerank and isinstance(query, str):
                try:
                    from modules.reranker.service import RerankService

                    reranker = RerankService()

                    candidates = merged[:100]
                    reranked = await reranker.rerank(
                        query=query, results=candidates, top_k=50, timeout=0.5
                    )

                    logger.info(f"Reranked {len(candidates)} results to {len(reranked)}")
                    return reranked

                except Exception:
                    logger.warning(
                        "Reranking failed, returning original results",
                        exc_info=True,
                        extra={"num_results": len(candidates)},
                    )
                    return merged[:limit]

            return merged[:limit]

        except (SearchError, EmbeddingError):
            raise
        except Exception:
            logger.error("Hybrid search failed, falling back to semantic search", exc_info=True)
            try:
                return await self._search_semantic(query, filters, limit)
            except Exception as fallback_error:
                logger.exception("Fallback semantic search also failed")
                raise SearchError(
                    "Both hybrid and fallback semantic search failed"
                ) from fallback_error

    def _detect_query_type(self, query: str | bytes) -> str:
        """Detect whether query is text or image.

        Args:
            query: Text string or image bytes

        Returns:
            'semantic' for images, 'hybrid' for text
        """
        if isinstance(query, bytes):
            return "semantic"
        return "hybrid"

    async def _load_snippet(self, result: SearchResult, query: str | bytes) -> str:
        """Load file content and generate snippet.

        Args:
            result: SearchResult object
            query: Original search query

        Returns:
            KWIC snippet with highlighted terms
        """
        if isinstance(query, bytes):
            return ""

        try:
            file_path = Path(result.file_path)
            if not file_path.exists():
                return ""

            if file_path.stat().st_size > 1_000_000:
                async with aiofiles.open(file_path, encoding="utf-8", errors="ignore") as f:
                    content = await f.read(10000)
            else:
                async with aiofiles.open(file_path, encoding="utf-8", errors="ignore") as f:
                    content = await f.read()

            snippet = SnippetGenerator.generate_kwic(content, query)
            return snippet

        except Exception:
            logger.warning(
                "Failed to load snippet", exc_info=True, extra={"file_path": result.file_path}
            )
            return ""

    def invalidate_cache(self) -> None:
        if self._cache:
            self._cache.invalidate_all()

    def get_cache_stats(self) -> dict[str, Any]:
        stats = {}
        if self._cache:
            stats["search"] = self._cache.get_stats().to_dict()
        if self._deduplicator:
            stats["deduplicator"] = self._deduplicator.get_stats()
        return stats

    # NOTE: Disabled - File model missing content_vector field
    # ORM-based implementation ready when field is added
    async def _execute_fulltext_search(self, query: str, filters: dict, limit: int) -> list[dict]:
        """Execute PostgreSQL full-text search (DISABLED - awaiting model update)."""
        logger.warning(
            "_execute_fulltext_search called but disabled: "
            "File model missing content_vector TSVector field"
        )
        return []
