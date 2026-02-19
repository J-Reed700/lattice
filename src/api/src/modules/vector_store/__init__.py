"""VectorStore module for Vault.

This module provides vector storage and similarity search functionality
using PostgreSQL with pgvector extension. It handles both text embeddings
(BAAI/bge-base-en-v1.5, 768 dimensions) and image embeddings
(CLIP ViT-B/32, 512 dimensions).

Public Interface:
    - VectorStore: Main class for vector operations
    - TextSearchResult: Result type for text similarity search
    - ImageSearchResult: Result type for image similarity search
    - EmbeddingStats: Statistics about stored embeddings

Example:
    >>> from modules.vector_store import VectorStore
    >>> from src.db import session_context
    >>>
    >>> async with session_context() as session:
    ...     store = VectorStore(session)
    ...     results = await store.search_text_similar(query_embedding)
"""

from .store import VectorStore
from .types import EmbeddingStats, ImageSearchResult, TextSearchResult

__all__ = [
    "EmbeddingStats",
    "ImageSearchResult",
    "TextSearchResult",
    "VectorStore",
]
