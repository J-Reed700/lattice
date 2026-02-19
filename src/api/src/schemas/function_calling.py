"""Pydantic schemas for LLM function calling tools.

These schemas define the input/output contracts for function calling tools
that LLMs can invoke to interact with the Recall system.

Design Philosophy (Bricks and Studs):
- Minimal: Only essential fields
- Clear: Descriptive names and documentation
- Stable: Versioned if breaking changes needed
- Validated: Strong type checking with Pydantic
"""

from __future__ import annotations

from datetime import datetime
from typing import Literal

from pydantic import BaseModel, Field

# =============================================================================
# Phase 1: Core Retrieval Functions
# =============================================================================


class SemanticSearchInput(BaseModel):
    """Input for semantic_search function.

    Searches the user's document vault using semantic similarity.
    Use this when the user asks to find, search, or retrieve information
    from their documents.
    """

    query: str = Field(
        ...,
        min_length=1,
        max_length=500,
        description="Search query describing what to find",
        examples=["machine learning papers", "project meeting notes from last week"],
    )
    limit: int = Field(
        default=10,
        ge=1,
        le=50,
        description="Maximum number of results to return",
    )
    threshold: float = Field(
        default=0.3,
        ge=0.0,
        le=1.0,
        description="Minimum similarity score (0.0-1.0). Lower = more results",
    )
    search_mode: Literal["semantic", "keyword", "hybrid"] = Field(
        default="hybrid",
        description="Search algorithm: semantic (embeddings), keyword (BM25), or hybrid (both)",
    )
    file_types: list[str] | None = Field(
        default=None,
        max_length=10,
        description="Filter by file extensions (e.g., ['pdf', 'txt'])",
        examples=[["pdf", "docx"], ["txt", "md"]],
    )
    date_from: datetime | None = Field(
        default=None,
        description="Only return documents modified after this date",
    )
    date_to: datetime | None = Field(
        default=None,
        description="Only return documents modified before this date",
    )


class DocumentResult(BaseModel):
    """A single document search result."""

    document_id: str = Field(..., description="Unique document identifier")
    filename: str = Field(..., description="Document filename")
    file_path: str = Field(..., description="Full path to document")
    mime_type: str = Field(..., description="MIME type (e.g., 'application/pdf')")
    score: float = Field(..., description="Relevance score (0.0-1.0)")
    snippet: str = Field(
        ...,
        max_length=500,
        description="Relevant excerpt from the document",
    )
    chunk_index: int | None = Field(
        default=None,
        description="Chunk position if result is from a chunk (0-based)",
    )
    modified_at: datetime = Field(..., description="Last modification timestamp")
    size_bytes: int = Field(..., description="File size in bytes")


class SemanticSearchOutput(BaseModel):
    """Output from semantic_search function."""

    results: list[DocumentResult] = Field(..., description="Search results")
    total_found: int = Field(..., description="Total results matching query")
    search_time_ms: float = Field(..., description="Query execution time in milliseconds")
    query: str = Field(..., description="Original search query")


class GetDocumentInput(BaseModel):
    """Input for get_document function.

    Retrieves the full content of a specific document by ID.
    Use this after semantic_search to get complete document text.
    """

    document_id: str = Field(
        ...,
        min_length=1,
        max_length=100,
        description="Document ID from search results",
        examples=["doc_abc123", "550e8400-e29b-41d4-a716-446655440000"],
    )
    include_metadata: bool = Field(
        default=True,
        description="Include file metadata (size, dates, tags)",
    )
    max_content_length: int = Field(
        default=50000,
        ge=1000,
        le=100000,
        description="Maximum content length in characters (prevents huge returns)",
    )


class DocumentMetadata(BaseModel):
    """Document metadata."""

    filename: str
    file_path: str
    mime_type: str
    extension: str
    size_bytes: int
    created_at: datetime | None = None
    modified_at: datetime
    indexed_at: datetime
    tags: list[str] = Field(default_factory=list)
    chunk_count: int


class GetDocumentOutput(BaseModel):
    """Output from get_document function."""

    document_id: str = Field(..., description="Document identifier")
    content: str = Field(..., description="Full document text content")
    content_truncated: bool = Field(
        ...,
        description="True if content was truncated due to max_content_length",
    )
    metadata: DocumentMetadata | None = Field(
        default=None,
        description="Document metadata (if include_metadata=true)",
    )


class ListDocumentsInput(BaseModel):
    """Input for list_documents function.

    Browse and filter documents in the vault.
    Use this to explore what documents exist, filter by type/date/tags,
    or get recent/favorite documents.
    """

    filter_mode: Literal["all", "recent", "favorites", "by_tag", "by_type"] = Field(
        default="all",
        description="Filtering mode for documents",
    )
    file_types: list[str] | None = Field(
        default=None,
        max_length=10,
        description="Filter by file extensions (for 'by_type' mode)",
        examples=[["pdf"], ["txt", "md", "docx"]],
    )
    tags: list[str] | None = Field(
        default=None,
        max_length=20,
        description="Filter by tags (for 'by_tag' mode)",
        examples=[["important", "work"], ["personal"]],
    )
    date_from: datetime | None = Field(
        default=None,
        description="Only return documents modified after this date",
    )
    date_to: datetime | None = Field(
        default=None,
        description="Only return documents modified before this date",
    )
    limit: int = Field(
        default=50,
        ge=1,
        le=500,
        description="Maximum number of documents to return",
    )
    offset: int = Field(
        default=0,
        ge=0,
        description="Number of documents to skip (for pagination)",
    )
    sort_by: Literal["modified", "created", "name", "size"] = Field(
        default="modified",
        description="Sort field",
    )
    sort_order: Literal["asc", "desc"] = Field(
        default="desc",
        description="Sort direction",
    )


class DocumentListItem(BaseModel):
    """Brief document information for list view."""

    document_id: str
    filename: str
    file_path: str
    mime_type: str
    extension: str
    size_bytes: int
    modified_at: datetime
    tags: list[str] = Field(default_factory=list)
    is_favorite: bool = False
    access_count: int = 0


class ListDocumentsOutput(BaseModel):
    """Output from list_documents function."""

    documents: list[DocumentListItem] = Field(..., description="List of documents")
    total: int = Field(..., description="Total documents matching filters")
    limit: int = Field(..., description="Requested limit")
    offset: int = Field(..., description="Requested offset")
    has_more: bool = Field(..., description="True if more results available")


# =============================================================================
# Phase 2: Web Integration Functions
# =============================================================================


class WebSearchInput(BaseModel):
    """Input for web_search function.

    Searches the web using DuckDuckGo when information isn't in the vault.
    Use this when the user asks about current events, external information,
    or when vault search returns no results.
    """

    query: str = Field(
        ...,
        min_length=1,
        max_length=300,
        description="Web search query",
        examples=["latest Python 3.12 features", "current weather in Tokyo"],
    )
    max_results: int = Field(
        default=5,
        ge=1,
        le=10,
        description="Maximum number of web results to return",
    )
    region: Literal["wt-wt", "us-en", "uk-en", "de-de", "fr-fr"] | None = Field(
        default="wt-wt",
        description="DuckDuckGo region code (wt-wt = worldwide)",
    )
    safesearch: Literal["off", "moderate", "strict"] = Field(
        default="moderate",
        description="Safe search level",
    )


class WebSearchResult(BaseModel):
    """A single web search result."""

    title: str = Field(..., description="Page title")
    url: str = Field(..., description="Page URL")
    snippet: str = Field(
        ...,
        max_length=500,
        description="Text snippet from the page",
    )
    published_date: datetime | None = Field(
        default=None,
        description="Publication date if available",
    )


class WebSearchOutput(BaseModel):
    """Output from web_search function."""

    results: list[WebSearchResult] = Field(..., description="Web search results")
    query: str = Field(..., description="Original search query")
    result_count: int = Field(..., description="Number of results returned")


class FetchUrlContentInput(BaseModel):
    """Input for fetch_url_content function.

    Fetches and extracts readable text from a web URL.
    Use this after web_search to get full content from interesting results,
    or when the user provides a URL to read/analyze.
    """

    url: str = Field(
        ...,
        min_length=10,
        max_length=2000,
        description="URL to fetch and extract content from",
        examples=["https://example.com/article", "https://docs.python.org/3/"],
    )
    max_content_length: int = Field(
        default=50000,
        ge=1000,
        le=100000,
        description="Maximum content length in characters",
    )
    extract_mode: Literal["article", "raw_text", "markdown"] = Field(
        default="article",
        description="Content extraction mode: article (main content), raw_text (all text), markdown (preserve formatting)",
    )
    timeout_seconds: int = Field(
        default=10,
        ge=1,
        le=30,
        description="Request timeout in seconds",
    )


class FetchUrlContentOutput(BaseModel):
    """Output from fetch_url_content function."""

    url: str = Field(..., description="Fetched URL (may differ if redirected)")
    title: str | None = Field(default=None, description="Page title if available")
    content: str = Field(..., description="Extracted text content")
    content_truncated: bool = Field(
        ...,
        description="True if content exceeded max_content_length",
    )
    word_count: int = Field(..., description="Word count of extracted content")
    fetch_time_ms: float = Field(..., description="Fetch time in milliseconds")
    content_type: str | None = Field(
        default=None,
        description="HTTP Content-Type header",
    )


# =============================================================================
# Common Error Response
# =============================================================================


class FunctionCallError(BaseModel):
    """Standard error response for function calls."""

    error_code: str = Field(
        ...,
        description="Machine-readable error code",
        examples=["DOCUMENT_NOT_FOUND", "RATE_LIMIT_EXCEEDED", "INVALID_URL"],
    )
    message: str = Field(..., description="Human-readable error message")
    details: dict[str, str] | None = Field(
        default=None,
        description="Additional error context",
    )
