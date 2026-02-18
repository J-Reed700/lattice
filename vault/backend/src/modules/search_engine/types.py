"""
Search Engine Types

Data models and exceptions for the search engine module.
"""

from dataclasses import dataclass, field
from typing import Any


@dataclass
class SearchResult:
    """Search result with ranking score and metadata.

    Attributes:
        file_id: Unique file identifier
        file_path: Absolute path to file
        score: Relevance score (0.0-1.0)
        metadata: File metadata (size, type, timestamps)
        snippet: Text excerpt with query terms highlighted
        thumbnail_path: Optional path to thumbnail image

    Example:
        >>> result = SearchResult(
        ...     file_id="abc123",
        ...     file_path="/path/to/file.txt",
        ...     score=0.95,
        ...     metadata={"size": 1024, "type": "text/plain"},
        ...     snippet="This is a **highlighted** excerpt",
        ...     thumbnail_path=None
        ... )
    """

    file_id: str
    file_path: str
    score: float
    metadata: dict[str, Any] = field(default_factory=dict)
    snippet: str | None = None
    thumbnail_path: str | None = None

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for API serialization.

        Returns:
            Dictionary representation of search result

        Example:
            >>> result.to_dict()
            {
                "file_id": "abc123",
                "file_path": "/path/to/file.txt",
                "score": 0.95,
                "metadata": {"size": 1024},
                "snippet": "...",
                "thumbnail_path": None
            }
        """
        return {
            "file_id": self.file_id,
            "file_path": self.file_path,
            "score": self.score,
            "metadata": self.metadata,
            "snippet": self.snippet,
            "thumbnail_path": self.thumbnail_path,
        }


class SearchError(Exception):
    """Base exception for search engine errors."""


class InvalidQueryError(SearchError):
    """Query validation failed."""


class EmbeddingError(SearchError):
    """Embedding generation failed."""


class DatabaseError(SearchError):
    """Database operation failed."""
