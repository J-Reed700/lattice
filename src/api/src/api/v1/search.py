from __future__ import annotations

import logging
import time

from fastapi import APIRouter, Body, Depends, Request
from slowapi import Limiter
from slowapi.util import get_remote_address
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.dependencies import get_db
from src.api.errors import EmbeddingError, SearchError, ValidationError
from src.auth.dependencies import get_current_active_user
from src.auth.models import User
from src.middleware.csrf import csrf_protect
from src.schemas.search import (
    AutocompleteRequest,
    AutocompleteResponse,
    SearchRequest,
    SearchResponse,
    SearchResultItem,
)
from src.modules.search_engine import SearchFilters, SearchMode, SearchService

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/search", tags=["search"])
limiter = Limiter(key_func=get_remote_address)


@router.post("/", response_model=SearchResponse)
@limiter.limit("20/minute")
async def search(
    request: Request,
    search_request: SearchRequest = Body(...),
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
) -> SearchResponse:
    """
    Unified search endpoint.
    """
    start = time.time()

    try:
        search_service = SearchService(session=db)

        search_mode_map = {"semantic": "vector", "keyword": "text", "hybrid": "hybrid"}
        mode: SearchMode = search_mode_map[search_request.search_type]

        filters = None
        if any([search_request.file_types, search_request.tags, search_request.date_from, search_request.date_to]):
            filters = SearchFilters(
                mime_types=search_request.file_types, date_from=search_request.date_from, date_to=search_request.date_to
            )

        results = await search_service.search(
            query=search_request.query,
            mode=mode,
            limit=search_request.top_k,
            filters=filters,
            vector_weight=search_request.semantic_weight,
            text_weight=search_request.keyword_weight,
            similarity_threshold=search_request.min_score,
        )

        result_items = [
            SearchResultItem(
                file_id=result.id,
                filename=result.filename,
                mime_type=result.mime_type,
                score=result.score,
                snippet=result.snippet or "",
                chunk_index=None,
                matched_at=result.modified_at,
                file_metadata={
                    "file_path": result.file_path,
                    "size_bytes": result.size_bytes,
                    "extension": result.extension,
                },
            )
            for result in results.results
        ]

        return SearchResponse(
            query=search_request.query,
            search_type=search_request.search_type,
            results=result_items,
            total_results=results.total,
            execution_time_ms=round((time.time() - start) * 1000, 2),
        )

    except (ValidationError, SearchError, EmbeddingError):
        raise
    except ValueError as e:
        logger.error("Invalid search request", exc_info=True, extra={"query": search_request.query[:100]})
        raise ValidationError(f"Invalid search parameters: {e!s}") from e
    except KeyError as e:
        logger.error(
            "Invalid search type", exc_info=True, extra={"search_type": search_request.search_type}
        )
        raise ValidationError(f"Invalid search type: {search_request.search_type}") from e
    except Exception as e:
        logger.exception(
            "Unexpected search error",
            extra={"query": search_request.query[:100], "search_type": search_request.search_type},
        )
        raise SearchError("Search operation failed unexpectedly") from e


@router.post("/semantic", response_model=SearchResponse)
@limiter.limit("20/minute")
async def semantic_search(
    request: Request,
    query: str = Body(..., embed=True),
    top_k: int = Body(10, embed=True),
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
) -> SearchResponse:
    """Semantic search only (convenience endpoint)."""
    search_request = SearchRequest(query=query, search_type="semantic", top_k=top_k)
    return await search(request, search_request, current_user, _, db)


@router.post("/keyword", response_model=SearchResponse)
@limiter.limit("20/minute")
async def keyword_search(
    request: Request,
    query: str = Body(..., embed=True),
    top_k: int = Body(10, embed=True),
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
) -> SearchResponse:
    """Keyword search only (convenience endpoint)."""
    search_request = SearchRequest(query=query, search_type="keyword", top_k=top_k)
    return await search(request, search_request, current_user, _, db)


@router.post("/autocomplete", response_model=AutocompleteResponse)
@limiter.limit("30/minute")
async def autocomplete(
    request: Request,
    autocomplete_request: AutocompleteRequest = Body(...),
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
) -> AutocompleteResponse:
    """Search suggestions/autocomplete."""
    try:
        search_service = SearchService(session=db)

        results = await search_service.search(query=autocomplete_request.query, mode="text", limit=autocomplete_request.limit)

        suggestions = list(set([result.filename for result in results.results]))[: autocomplete_request.limit]

        return AutocompleteResponse(suggestions=suggestions)

    except SearchError:
        logger.warning(
            "Autocomplete search failed, returning empty list",
            exc_info=True,
            extra={"query": request.query[:100]},
        )
        return AutocompleteResponse(suggestions=[])
    except Exception:
        logger.exception("Unexpected autocomplete error", extra={"query": request.query[:100]})
        return AutocompleteResponse(suggestions=[])
