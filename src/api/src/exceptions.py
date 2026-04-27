"""
Shared Exception Classes

Application-wide exception classes that can be used across all layers
(services, API, modules, etc.) without creating circular dependencies.
"""

from __future__ import annotations

from fastapi import status


class APIError(Exception):
    """Base exception for API errors."""

    def __init__(
        self,
        message: str,
        code: str = "INTERNAL_ERROR",
        status_code: int = status.HTTP_500_INTERNAL_SERVER_ERROR,
        details: dict | None = None,
    ):
        self.message = message
        self.code = code
        self.status_code = status_code
        self.details = details or {}
        super().__init__(self.message)


class NotFoundError(APIError):
    """404 Not Found error."""

    def __init__(self, message: str = "Resource not found", details: dict | None = None):
        super().__init__(
            message=message,
            code="NOT_FOUND",
            status_code=status.HTTP_404_NOT_FOUND,
            details=details,
        )


class ValidationError(APIError):
    """400 Bad Request validation error."""

    def __init__(self, message: str = "Validation failed", details: dict | None = None):
        super().__init__(
            message=message,
            code="VALIDATION_ERROR",
            status_code=status.HTTP_400_BAD_REQUEST,
            details=details,
        )


class IndexingError(APIError):
    """Error during file indexing."""

    def __init__(self, message: str = "Indexing failed", details: dict | None = None):
        super().__init__(
            message=message,
            code="INDEXING_FAILED",
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            details=details,
        )


class StorageError(APIError):
    """Error during storage operations."""

    def __init__(self, message: str = "Storage operation failed", details: dict | None = None):
        super().__init__(
            message=message,
            code="STORAGE_ERROR",
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            details=details,
        )


class SearchError(APIError):
    """Error during search operations."""

    def __init__(self, message: str = "Search operation failed", details: dict | None = None):
        super().__init__(
            message=message,
            code="SEARCH_ERROR",
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            details=details,
        )


class EmbeddingError(APIError):
    """Error during embedding generation."""

    def __init__(self, message: str = "Embedding generation failed", details: dict | None = None):
        super().__init__(
            message=message,
            code="EMBEDDING_ERROR",
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            details=details,
        )


class DatabaseError(APIError):
    """Error during database operations."""

    def __init__(self, message: str = "Database operation failed", details: dict | None = None):
        super().__init__(
            message=message,
            code="DATABASE_ERROR",
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            details=details,
        )


class RerankerError(APIError):
    """Error during reranking operations."""

    def __init__(self, message: str = "Reranking failed", details: dict | None = None):
        super().__init__(
            message=message,
            code="RERANKER_ERROR",
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            details=details,
        )


class ContentExtractionError(APIError):
    """Error during content extraction."""

    def __init__(self, message: str = "Content extraction failed", details: dict | None = None):
        super().__init__(
            message=message,
            code="CONTENT_EXTRACTION_ERROR",
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            details=details,
        )


__all__ = [
    "APIError",
    "NotFoundError",
    "ValidationError",
    "IndexingError",
    "StorageError",
    "SearchError",
    "EmbeddingError",
    "DatabaseError",
    "RerankerError",
    "ContentExtractionError",
]
