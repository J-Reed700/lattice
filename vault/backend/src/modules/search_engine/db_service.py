import asyncio
from collections import OrderedDict
from datetime import datetime, timedelta
import hashlib
import json
import logging
from typing import Any, Literal

import numpy as np
from sqlalchemy.ext.asyncio import AsyncSession

from src.config.settings import get_settings
from src.exceptions import DatabaseError, EmbeddingError, SearchError
from src.modules.caching.embedding_cache import EmbeddingCache
from src.modules.embedding_generator import EmbeddingService
from src.modules.reranker.service import RerankService
from src.modules.search_engine.types import SearchResult as EngineSearchResult

from .db_hybrid_search import HybridSearchService
from .db_results import SearchResult, SearchResults
from .db_text_search import TextSearchService
from .db_vector_search import VectorSearchService
from .filters import SearchFilters

logger = logging.getLogger(__name__)

SearchMode = Literal["vector", "text", "hybrid"]


class QueryResultCache:
    """Async LRU cache for search results with TTL.

    Performance optimization: Cache frequent queries to avoid repeated database/vector searches.
    """

    def __init__(self, max_size: int = 1000, ttl_seconds: int = 300):
        self.max_size = max_size
        self.ttl = timedelta(seconds=ttl_seconds)
        self._cache: OrderedDict[str, tuple[SearchResults, datetime]] = OrderedDict()
        self._lock = asyncio.Lock()

    def _make_key(self, query: str, mode: str, limit: int, offset: int, filters: Any) -> str:
        """Create cache key from query parameters."""
        key_data = {
            "query": query.strip().lower(),
            "mode": mode,
            "limit": limit,
            "offset": offset,
            "filters": str(filters) if filters else None,
        }
        key_str = json.dumps(key_data, sort_keys=True)
        return hashlib.sha256(key_str.encode()).hexdigest()

    async def get(
        self, query: str, mode: str, limit: int, offset: int, filters: Any
    ) -> SearchResults | None:
        """Get cached results if available and not expired."""
        key = self._make_key(query, mode, limit, offset, filters)

        async with self._lock:
            if key in self._cache:
                results, timestamp = self._cache[key]
                if datetime.now() - timestamp < self.ttl:
                    self._cache.move_to_end(key)
                    logger.debug(f"Query result cache hit: {query[:50]}...")
                    return results
                del self._cache[key]
                logger.debug(f"Query result cache expired: {query[:50]}...")

        return None

    async def put(
        self, query: str, mode: str, limit: int, offset: int, filters: Any, results: SearchResults
    ) -> None:
        """Store results in cache with current timestamp."""
        key = self._make_key(query, mode, limit, offset, filters)

        async with self._lock:
            if len(self._cache) >= self.max_size and key not in self._cache:
                self._cache.popitem(last=False)

            self._cache[key] = (results, datetime.now())
            self._cache.move_to_end(key)

    async def invalidate(self) -> None:
        """Clear all cached results."""
        async with self._lock:
            self._cache.clear()
            logger.info("Query result cache cleared")


class SearchService:
    def __init__(self, session: AsyncSession, embedding_service: EmbeddingService | None = None):
        self.session = session
        self.embedding_service = embedding_service or EmbeddingService()
        self.vector_search = VectorSearchService(session)
        self.text_search = TextSearchService(session)
        self.hybrid_search = HybridSearchService(session)

        # Initialize embedding cache for query embeddings
        self.embedding_cache = EmbeddingCache(max_size=10000, embedding_dim=1024)

        # Performance optimization: Query result cache with 5-minute TTL
        self.result_cache = QueryResultCache(max_size=1000, ttl_seconds=300)

        # Cache for reranker instance
        self._reranker: RerankService | None = None
        self._settings = get_settings()

    async def _get_cached_embedding(self, query: str) -> list[float]:
        """Get embedding from cache or generate and cache it (async optimized)."""
        model_name = "BAAI/bge-m3"

        # Try to get from cache first (async cache prevents event loop blocking)
        cached_emb = await self.embedding_cache.get(query, model_name)
        if cached_emb is not None:
            logger.debug(f"Cache hit for query: '{query[:50]}...'")
            return cached_emb.tolist()

        # Cache miss - generate embedding
        logger.debug(f"Cache miss for query: '{query[:50]}...'")
        embedding = await self.embedding_service.text_generator.generate_from_text(query)

        # Store in cache (async to prevent blocking)
        await self.embedding_cache.put(query, model_name, np.array(embedding))

        return embedding

    def _get_reranker(self) -> RerankService:
        """Get or create cached reranker instance."""
        if self._reranker is None:
            self._reranker = RerankService(
                model_name=self._settings.reranking_model,
                device=self._settings.ml_device,
                max_content_length=self._settings.reranking_max_content_length,
                batch_size=self._settings.reranking_batch_size,
                cache_enabled=self._settings.reranking_cache_enabled,
                cache_ttl=self._settings.reranking_cache_ttl,
            )
        return self._reranker

    async def search(
        self,
        query: str,
        mode: SearchMode = "hybrid",
        limit: int = 20,
        offset: int = 0,
        filters: SearchFilters | None = None,
        vector_weight: float = 0.7,
        text_weight: float = 0.3,
        similarity_threshold: float = 0.3,
        rerank: bool = True,
    ) -> SearchResults:
        if not query or not query.strip():
            logger.warning("Empty query provided to search service")
            return SearchResults.from_results([], 0, offset, limit)

        if filters:
            filters.validate()

        # Performance optimization: Check result cache first
        cached_results = await self.result_cache.get(query, mode, limit, offset, filters)
        if cached_results is not None:
            return cached_results

        logger.info(
            f"Performing {mode} search for query: '{query}' (limit={limit}, offset={offset})"
        )

        try:
            if mode == "vector":
                results = await self._vector_search(
                    query=query,
                    limit=limit,
                    offset=offset,
                    filters=filters,
                    similarity_threshold=similarity_threshold,
                )
            elif mode == "text":
                results = await self._text_search(
                    query=query,
                    limit=limit,
                    offset=offset,
                    filters=filters,
                )
            elif mode == "hybrid":
                results = await self._hybrid_search(
                    query=query,
                    limit=limit,
                    offset=offset,
                    filters=filters,
                    vector_weight=vector_weight,
                    text_weight=text_weight,
                    similarity_threshold=similarity_threshold,
                    rerank=rerank,
                )
            else:
                raise ValueError(f"Invalid search mode: {mode}")

            total = len(results)

            search_results = SearchResults.from_results(
                results=results,
                total=total,
                offset=offset,
                limit=limit,
            )

            # Performance optimization: Cache the results
            await self.result_cache.put(query, mode, limit, offset, filters, search_results)

            return search_results

        except (EmbeddingError, DatabaseError, SearchError):
            raise
        except ValueError as e:
            logger.error(
                "Invalid search parameters",
                exc_info=True,
                extra={"mode": mode, "query": query[:100]},
            )
            raise SearchError(f"Invalid search parameters: {e!s}") from e
        except Exception as e:
            logger.exception("Unexpected search error", extra={"mode": mode, "query": query[:100]})
            raise SearchError("Search operation failed unexpectedly") from e

    async def _vector_search(
        self,
        query: str,
        limit: int,
        offset: int,
        filters: SearchFilters | None,
        similarity_threshold: float,
    ) -> list[SearchResult]:
        # Use cached embedding
        embedding = await self._get_cached_embedding(query)

        results = await self.vector_search.search_with_best_chunk(
            embedding=embedding,
            limit=limit,
            offset=offset,
            filters=filters,
            similarity_threshold=similarity_threshold,
        )

        return results

    async def _text_search(
        self,
        query: str,
        limit: int,
        offset: int,
        filters: SearchFilters | None,
    ) -> list[SearchResult]:
        results = await self.text_search.search(
            query_text=query,
            limit=limit,
            offset=offset,
            filters=filters,
        )

        return results

    async def _hybrid_search(
        self,
        query: str,
        limit: int,
        offset: int,
        filters: SearchFilters | None,
        vector_weight: float,
        text_weight: float,
        similarity_threshold: float,
        rerank: bool = True,
    ) -> list[SearchResult]:
        # Use cached embedding
        embedding = await self._get_cached_embedding(query)

        results = await self.hybrid_search.search(
            query_text=query,
            embedding=embedding,
            limit=limit,
            offset=offset,
            filters=filters,
            vector_weight=vector_weight,
            text_weight=text_weight,
            similarity_threshold=similarity_threshold,
        )

        if rerank and results:
            try:
                if not self._settings.reranking_enabled:
                    logger.debug("Reranking disabled in settings")
                    return results

                # Use cached reranker instance
                reranker = self._get_reranker()

                rerank_input = min(len(results), self._settings.reranking_top_k_input)
                rerank_output = min(rerank_input, self._settings.reranking_top_k_output, limit)

                engine_results = []
                for result in results[:rerank_input]:
                    engine_result = EngineSearchResult(
                        file_id=str(result.id),
                        file_path=result.file_path,
                        score=result.score,
                        metadata={},
                    )
                    engine_results.append(engine_result)

                reranked = await reranker.rerank(
                    query=query,
                    results=engine_results,
                    top_k=rerank_output,
                    timeout=self._settings.reranking_timeout,
                )

                reranked_ids = {r.file_id: r.score for r in reranked}

                filtered_results = []
                for result in results:
                    result_id = str(result.id)
                    if result_id in reranked_ids:
                        updated_result = result.with_score(reranked_ids[result_id])
                        filtered_results.append(updated_result)

                filtered_results.sort(key=lambda x: x.score, reverse=True)

                logger.info(
                    f"Reranked {rerank_input} results to {len(filtered_results)} "
                    f"using {self._settings.reranking_model}"
                )
                return filtered_results

            except Exception as e:
                logger.error(f"Reranking failed: {e}", exc_info=True)
                return results

        return results

    async def search_with_reranking(
        self,
        query: str,
        limit: int = 20,
        offset: int = 0,
        filters: SearchFilters | None = None,
        vector_weight: float = 0.6,
        text_weight: float = 0.3,
        recency_weight: float = 0.1,
        similarity_threshold: float = 0.3,
    ) -> SearchResults:
        if not query or not query.strip():
            logger.warning("Empty query provided to search with reranking")
            return SearchResults.from_results([], 0, offset, limit)

        if filters:
            filters.validate()

        logger.info(
            f"Performing hybrid search with reranking for query: '{query}' "
            f"(limit={limit}, offset={offset})"
        )

        try:
            # Use cached embedding
            embedding = await self._get_cached_embedding(query)

            results = await self.hybrid_search.search_reranked(
                query_text=query,
                embedding=embedding,
                limit=limit,
                offset=offset,
                filters=filters,
                vector_weight=vector_weight,
                text_weight=text_weight,
                recency_weight=recency_weight,
                similarity_threshold=similarity_threshold,
            )

            total = len(results)

            return SearchResults.from_results(
                results=results,
                total=total,
                offset=offset,
                limit=limit,
            )

        except (EmbeddingError, DatabaseError, SearchError):
            raise
        except ValueError as e:
            logger.error(
                "Invalid search parameters in reranking",
                exc_info=True,
                extra={"query": query[:100]},
            )
            raise SearchError(f"Invalid search parameters: {e!s}") from e
        except Exception as e:
            logger.exception("Unexpected search with reranking error", extra={"query": query[:100]})
            raise SearchError("Search with reranking failed unexpectedly") from e

    async def search_by_embedding(
        self,
        embedding: list[float],
        limit: int = 20,
        offset: int = 0,
        filters: SearchFilters | None = None,
        similarity_threshold: float = 0.3,
    ) -> SearchResults:
        if filters:
            filters.validate()

        logger.info(f"Performing vector search by embedding (limit={limit}, offset={offset})")

        try:
            results = await self.vector_search.search_with_best_chunk(
                embedding=embedding,
                limit=limit,
                offset=offset,
                filters=filters,
                similarity_threshold=similarity_threshold,
            )

            total = len(results)

            return SearchResults.from_results(
                results=results,
                total=total,
                offset=offset,
                limit=limit,
            )

        except (DatabaseError, SearchError):
            raise
        except ValueError as e:
            logger.error(
                "Invalid embedding vector", exc_info=True, extra={"embedding_dim": len(embedding)}
            )
            raise SearchError(f"Invalid embedding vector: {e!s}") from e
        except Exception as e:
            logger.exception(
                "Unexpected vector search error", extra={"embedding_dim": len(embedding)}
            )
            raise SearchError("Vector search failed unexpectedly") from e

    async def search_phrase(
        self,
        query: str,
        limit: int = 20,
        offset: int = 0,
        filters: SearchFilters | None = None,
    ) -> SearchResults:
        if not query or not query.strip():
            logger.warning("Empty query provided to phrase search")
            return SearchResults.from_results([], 0, offset, limit)

        if filters:
            filters.validate()

        logger.info(
            f"Performing phrase search for query: '{query}' (limit={limit}, offset={offset})"
        )

        try:
            results = await self.text_search.search_with_phrases(
                query_text=query,
                limit=limit,
                offset=offset,
                filters=filters,
            )

            total = len(results)

            return SearchResults.from_results(
                results=results,
                total=total,
                offset=offset,
                limit=limit,
            )

        except (DatabaseError, SearchError):
            raise
        except ValueError as e:
            logger.error(
                "Invalid phrase search parameters", exc_info=True, extra={"query": query[:100]}
            )
            raise SearchError(f"Invalid phrase search parameters: {e!s}") from e
        except Exception as e:
            logger.exception("Unexpected phrase search error", extra={"query": query[:100]})
            raise SearchError("Phrase search failed unexpectedly") from e
