"""Unified search service for all search modes.

This service provides a facade for BM25, hybrid, and vector search,
handling result mapping and search history recording.
"""

from __future__ import annotations

import logging
import time
from typing import TYPE_CHECKING, Any
from uuid import UUID

from sqlalchemy import select

from src.config import get_settings
from src.models.sync import Document
from src.observability.metrics import record_error, record_search
from src.observability.tracing import get_tracer
from src.schemas.search import SearchRequest, SearchResultItem
# TODO(Phase 3+): SearchHistoryService should be injected via constructor (optional)
# to allow tests to skip history recording and improve modularity.
# Current workaround: Always creates SearchHistoryService internally.
from src.services.search_history import SearchHistoryService

from .db_service import SearchService

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession

    from src.modules.search_engine.bm25 import BM25SearchEngine
    from src.modules.search_engine.hybrid import HybridSearchEngine

logger = logging.getLogger(__name__)
tracer = get_tracer(__name__)

__all__ = ["UnifiedSearchResult", "UnifiedSearchService"]


class UnifiedSearchResult:
    """Result container for unified search."""

    def __init__(
        self,
        results: list[SearchResultItem],
        total: int,
        took_ms: int,
        fusion_strategy: str | None = None,
    ):
        self.results = results
        self.total = total
        self.took_ms = took_ms
        self.fusion_strategy = fusion_strategy


class UnifiedSearchService:
    """Facade for all search modes with result mapping and history recording."""

    def __init__(
        self,
        session: AsyncSession,
        bm25_engine: BM25SearchEngine,
        hybrid_engine: HybridSearchEngine,
    ):
        """Initialize unified search service.

        Args:
            session: Database session
            bm25_engine: BM25 search engine instance
            hybrid_engine: Hybrid search engine instance
        """
        self.session = session
        self.bm25_engine = bm25_engine
        self.hybrid_engine = hybrid_engine
        self.search_service = SearchService(session)
        self.history_service = SearchHistoryService(session)
        self.settings = get_settings()

    async def search(self, request: SearchRequest) -> UnifiedSearchResult:
        """Execute unified search across all modes.

        Args:
            request: Search request with query, mode, filters, etc.

        Returns:
            UnifiedSearchResult with results, total, timing, and metadata

        Raises:
            Exception: If search fails
        """
        with tracer.start_as_current_span("unified_search") as span:
            span.set_attribute("search.query", request.query)
            span.set_attribute("search.limit", request.limit)
            span.set_attribute("search.mode", request.mode)
            span.set_attribute("search.offset", request.offset)

            start_time = time.time()
            fusion_strategy = None

            try:
                if request.mode == "bm25":
                    results, total, fusion_strategy = await self._search_bm25(request)
                elif request.mode == "hybrid" and self.settings.hybrid_search_enabled:
                    results, total, fusion_strategy = await self._search_hybrid(request)
                else:
                    results, total, fusion_strategy = await self._search_vector(request)

                took_ms = int((time.time() - start_time) * 1000)

                # Record search history
                await self._record_history(
                    query=request.query,
                    search_type=request.mode,
                    result_count=len(results),
                    execution_time_ms=took_ms,
                )

                # Record metrics
                record_search(request.query, len(results), took_ms, request.mode)
                span.set_attribute("success", True)
                span.set_attribute("results.total", total)

                return UnifiedSearchResult(
                    results=results,
                    total=total,
                    took_ms=took_ms,
                    fusion_strategy=fusion_strategy,
                )

            except Exception as e:
                record_error(type(e).__name__, "/search")
                raise

    async def _search_bm25(
        self, request: SearchRequest
    ) -> tuple[list[SearchResultItem], int, str | None]:
        """Execute BM25 search.

        Args:
            request: Search request

        Returns:
            Tuple of (results, total_count, fusion_strategy)
        """
        with tracer.start_as_current_span("bm25_search") as search_span:
            raw_results = await self.bm25_engine.search(
                query=request.query,
                top_k=request.limit + request.offset,
                filters=None,
            )
            search_span.set_attribute("results.count", len(raw_results))

            # Apply pagination
            paginated_results = raw_results[request.offset :]

            # Map to SearchResultItem
            results = await self._map_bm25_results(paginated_results)

            return results, len(raw_results), None

    async def _search_hybrid(
        self, request: SearchRequest
    ) -> tuple[list[SearchResultItem], int, str | None]:
        """Execute hybrid search.

        Args:
            request: Search request

        Returns:
            Tuple of (results, total_count, fusion_strategy)
        """
        with tracer.start_as_current_span("hybrid_search") as search_span:
            strategy = request.hybrid_strategy or self.settings.hybrid_default_strategy
            search_span.set_attribute("fusion.strategy", strategy)

            raw_results = await self.hybrid_engine.search(
                query=request.query,
                embedding=None,
                top_k=request.limit + request.offset,
                strategy=strategy,
                vector_weight=request.semantic_weight,
                bm25_weight=request.keyword_weight,
                filters=None,
            )

            search_span.set_attribute("results.count", len(raw_results))

            # Apply pagination
            paginated_results = raw_results[request.offset :]

            # Map to SearchResultItem
            results = await self._map_hybrid_results(paginated_results)

            return results, len(raw_results), strategy

    async def _search_vector(
        self, request: SearchRequest
    ) -> tuple[list[SearchResultItem], int, str | None]:
        """Execute vector/text search using SearchService.

        Args:
            request: Search request

        Returns:
            Tuple of (results, total_count, fusion_strategy)
        """
        # Only call SearchService for vector, text, or hybrid modes (not bm25)
        if request.mode not in ("vector", "text", "hybrid"):
            return [], 0, None

        with tracer.start_as_current_span("vector_search") as search_span:
            # Convert API SearchFilters to service SearchFilters if needed
            service_filters = None
            if request.filters:
                from src.modules.search_engine.filters import SearchFilters as ServiceSearchFilters

                service_filters = ServiceSearchFilters(
                    **request.filters.model_dump(exclude_none=True)
                )

            raw_results = await self.search_service.search(
                query=request.query,
                mode=request.mode,  # type: ignore[arg-type]
                limit=request.limit,
                offset=request.offset,
                filters=service_filters,
                rerank=request.rerank,
                vector_weight=request.semantic_weight,
                text_weight=request.keyword_weight,
            )
            search_span.set_attribute("results.count", len(raw_results.results))

            # Map to SearchResultItem
            results = await self._map_vector_results(raw_results.results)

            # Get total count
            total = raw_results.total

            return results, total, None

    async def _map_bm25_results(self, raw_results: list[dict[str, Any]]) -> list[SearchResultItem]:
        """Map BM25 raw results to SearchResultItem.

        Args:
            raw_results: Raw BM25 results

        Returns:
            List of SearchResultItem
        """
        results = []
        for r in raw_results:
            # BM25 results are dicts with file metadata
            doc = await self._get_document_by_id(r.get("document_id"))
            if doc:
                results.append(self._create_search_result_item(doc, r.get("bm25_score", 0.0)))

        return results

    async def _map_hybrid_results(
        self, raw_results: list[dict[str, Any]]
    ) -> list[SearchResultItem]:
        """Map hybrid raw results to SearchResultItem.

        Args:
            raw_results: Raw hybrid results

        Returns:
            List of SearchResultItem
        """
        results = []
        for r in raw_results:
            # Hybrid results are dicts with file metadata
            doc = await self._get_document_by_id(r.get("file_id"))
            if doc:
                results.append(self._create_search_result_item(doc, r.get("score", 0.0)))

        return results

    async def _map_vector_results(self, raw_results: list[Any]) -> list[SearchResultItem]:
        """Map vector/text search results to SearchResultItem.

        Args:
            raw_results: SearchResults from SearchService

        Returns:
            List of SearchResultItem
        """
        results = []

        # Handle both dict and object results
        result_list = raw_results if isinstance(raw_results, list) else []

        for result in result_list:
            if isinstance(result, dict):
                # Dict result - need to fetch document
                doc = await self._get_document_by_id(result.get("id"))
                if doc:
                    results.append(
                        self._create_search_result_item(
                            doc, result.get("score", 0.0), result.get("snippet")
                        )
                    )
            else:
                # Object result (SearchResult dataclass)
                results.append(
                    SearchResultItem(
                        id=result.id,
                        file_path=result.file_path,
                        filename=result.filename,
                        extension=result.extension or "",
                        mime_type=result.mime_type,
                        size_bytes=result.size_bytes,
                        score=result.score,
                        snippet=result.snippet,
                        thumbnail_url=f"/api/v1/files/{result.id}/thumbnail",
                        created_at=result.modified_at,
                        modified_at=result.modified_at,
                        indexed_at=result.modified_at,
                        last_accessed_at=None,
                        watch_folder_id=result.id,  # Placeholder
                    )
                )

        return results

    async def _get_document_by_id(self, doc_id: int | str | None) -> Document | None:
        """Fetch document by ID.

        Args:
            doc_id: Document ID

        Returns:
            Document object or None
        """
        if not doc_id:
            return None

        try:
            query = select(Document).where(Document.id == int(doc_id))
            result = await self.session.execute(query)
            return result.scalar_one_or_none()
        except Exception:
            logger.exception(f"Failed to fetch document {doc_id}")
            return None

    def _create_search_result_item(
        self,
        doc: Document,
        score: float,
        snippet: str | None = None,
    ) -> SearchResultItem:
        """Create SearchResultItem from Document.

        Args:
            doc: Document object
            score: Relevance score
            snippet: Text snippet

        Returns:
            SearchResultItem
        """
        # Convert int ID to UUID (use a deterministic conversion)
        # This is a workaround - ideally Document should have UUID
        doc_uuid = UUID(int=doc.id)

        return SearchResultItem(
            id=doc_uuid,
            file_path=doc.path,
            filename=doc.title or doc.path.split("/")[-1],
            extension="",  # Document doesn't have extension field
            mime_type="text/plain",  # Default MIME type
            size_bytes=len(doc.content or "") if doc.content else 0,
            score=score,
            snippet=snippet,
            thumbnail_url=f"/api/v1/files/{doc.id}/thumbnail",
            created_at=doc.created_at,
            modified_at=doc.modified_at,
            indexed_at=doc.created_at,
            last_accessed_at=None,
            watch_folder_id=doc_uuid,  # Placeholder - Document doesn't have watch_folder_id
        )

    async def _count_vector_results(self, request: SearchRequest) -> int:
        """Count total vector search results.

        Args:
            request: Search request

        Returns:
            Total result count
        """
        try:
            # For now, return the length of results since count_results doesn't exist
            # This is a simplification - in production you'd want proper counting
            return request.limit
        except Exception:
            logger.exception("Failed to count results")
            return 0

    async def _record_history(
        self,
        query: str,
        search_type: str,
        result_count: int,
        execution_time_ms: int,
    ) -> None:
        """Record search in history.

        Args:
            query: Search query
            search_type: Search mode
            result_count: Number of results
            execution_time_ms: Execution time in milliseconds
        """
        try:
            await self.history_service.record_search(
                query=query,
                search_type=search_type,
                result_count=result_count,
                execution_time_ms=execution_time_ms,
            )
        except Exception:
            logger.exception("Failed to record search history")
