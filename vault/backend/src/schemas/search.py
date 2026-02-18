from datetime import datetime
from typing import Literal
from uuid import UUID

from pydantic import BaseModel, Field


class SearchRequest(BaseModel):
    """Unified search request."""

    query: str = Field(..., min_length=1, max_length=500)
    search_type: Literal["semantic", "keyword", "hybrid", "bm25"] = "hybrid"
    top_k: int = Field(10, ge=1, le=100)

    file_types: list[str] | None = None
    tags: list[str] | None = None
    date_from: datetime | None = None
    date_to: datetime | None = None
    min_score: float = Field(0.0, ge=0.0, le=1.0)

    semantic_weight: float = Field(0.7, ge=0.0, le=1.0)
    keyword_weight: float = Field(0.3, ge=0.0, le=1.0)

    hybrid_strategy: Literal["rrf", "weighted"] | None = None
    rrf_k: int | None = Field(None, ge=1, le=200)


class SearchResultItem(BaseModel):
    """Single search result."""

    file_id: UUID
    filename: str
    mime_type: str
    score: float
    snippet: str
    chunk_index: int | None = None
    matched_at: datetime
    file_metadata: dict


class SearchResponse(BaseModel):
    """Search results response."""

    query: str
    search_type: str
    results: list[SearchResultItem]
    total_results: int
    execution_time_ms: float
    fusion_strategy: str | None = None


class AutocompleteRequest(BaseModel):
    """Autocomplete/suggestion request."""

    query: str = Field(..., min_length=1, max_length=100)
    limit: int = Field(5, ge=1, le=20)


class AutocompleteResponse(BaseModel):
    """Autocomplete suggestions."""

    suggestions: list[str]
