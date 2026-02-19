"""
SearchEngine Module

Unified search interface with semantic, keyword, and hybrid search capabilities.
See README.md for full contract specification.

Basic Usage:
    >>> from search_engine import SearchEngine, SearchFilters
    >>> engine = SearchEngine(vector_store, embedder, db_session)
    >>> results = await engine.search("query text", limit=20)
    >>> filtered_results = await engine.search(
    ...     "query text",
    ...     filters=SearchFilters(file_types=['.txt', '.pdf']),
    ...     mode="hybrid"
    ... )
"""

from .db_hybrid_search import HybridSearchService
from .db_results import SearchResult as DbSearchResult
from .db_results import SearchResults
from .db_service import SearchMode, SearchService
from .db_text_search import TextSearchService
from .db_unified_search import UnifiedSearchResult, UnifiedSearchService
from .db_vector_search import VectorSearchService
from .engine import SearchEngine
from .filters import SearchFilters
from .types import DatabaseError, EmbeddingError, InvalidQueryError, SearchError, SearchResult

__all__ = [
    "DatabaseError",
    "DbSearchResult",
    "EmbeddingError",
    "HybridSearchService",
    "InvalidQueryError",
    "SearchEngine",
    "SearchError",
    "SearchFilters",
    "SearchMode",
    "SearchResult",
    "SearchResults",
    "SearchService",
    "TextSearchService",
    "UnifiedSearchResult",
    "UnifiedSearchService",
    "VectorSearchService",
]
