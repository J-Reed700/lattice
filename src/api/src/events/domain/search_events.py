"""Search-related domain events.

Events emitted by SearchHistoryService for query tracking and analytics.
"""

from __future__ import annotations

from datetime import datetime
from typing import Any

from pydantic import BaseModel, Field, field_validator

__all__ = [
    "SearchQueryRecorded",
    "SearchQueryRecordingFailed",
]


class SearchQueryRecorded(BaseModel):
    """Event emitted when a search query is successfully recorded in history.

    This event is emitted AFTER the search query has been persisted to the database.
    Subscribers can use this for analytics, trending queries, or user behavior tracking.

    Attributes:
        query: The search query text
        search_type: Type of search (semantic, keyword, hybrid)
        result_count: Number of results returned
        execution_time_ms: Query execution time in milliseconds
        user_id: User who performed the search
        timestamp: When the query was recorded (UTC)
        metadata: Additional contextual information

    Example:
        >>> event = SearchQueryRecorded(
        ...     query="machine learning",
        ...     search_type="hybrid",
        ...     result_count=15,
        ...     execution_time_ms=125.5,
        ...     user_id="user123"
        ... )
        >>> assert event.query == "machine learning"
        >>> assert event.execution_time_ms == 125.5
    """

    query: str = Field(..., min_length=1, max_length=500, description="Search query text")
    search_type: str = Field(
        ..., pattern="^(semantic|keyword|hybrid)$", description="Type of search performed"
    )
    result_count: int = Field(..., ge=0, description="Number of results returned")
    execution_time_ms: float = Field(..., ge=0, description="Execution time in milliseconds")
    user_id: str = Field(default="default", description="User who performed the search")
    timestamp: datetime = Field(default_factory=datetime.utcnow, description="When recorded (UTC)")
    metadata: dict[str, Any] = Field(
        default_factory=dict, description="Additional contextual information"
    )

    @field_validator("query")
    @classmethod
    def query_must_not_be_empty(cls, v: str) -> str:
        """Validate query is not empty or whitespace-only."""
        if not v.strip():
            raise ValueError("Query must not be empty or whitespace-only")
        return v.strip()

    class Config:
        """Pydantic configuration."""

        frozen = True  # Immutable event
        json_schema_extra = {
            "example": {
                "query": "machine learning algorithms",
                "search_type": "hybrid",
                "result_count": 15,
                "execution_time_ms": 125.5,
                "user_id": "user123",
                "timestamp": "2024-01-15T10:30:00Z",
                "metadata": {"filters": {"tags": ["python"]}, "reranked": True},
            }
        }


class SearchQueryRecordingFailed(BaseModel):
    """Event emitted when recording a search query fails.

    This event is emitted when an error occurs while persisting the search query
    to the database. Subscribers can use this for error tracking and alerting.

    Attributes:
        query: The search query text that failed to record
        search_type: Type of search (semantic, keyword, hybrid)
        result_count: Number of results returned
        execution_time_ms: Query execution time in milliseconds
        user_id: User who performed the search
        error: Error message describing the failure
        timestamp: When the failure occurred (UTC)
        metadata: Additional contextual information

    Example:
        >>> event = SearchQueryRecordingFailed(
        ...     query="test query",
        ...     search_type="hybrid",
        ...     result_count=10,
        ...     execution_time_ms=50.0,
        ...     user_id="user123",
        ...     error="Database connection timeout"
        ... )
        >>> assert event.error == "Database connection timeout"
    """

    query: str = Field(..., min_length=1, max_length=500, description="Search query text")
    search_type: str = Field(
        ..., pattern="^(semantic|keyword|hybrid)$", description="Type of search performed"
    )
    result_count: int = Field(..., ge=0, description="Number of results returned")
    execution_time_ms: float = Field(..., ge=0, description="Execution time in milliseconds")
    user_id: str = Field(default="default", description="User who performed the search")
    error: str = Field(..., min_length=1, description="Error message describing the failure")
    timestamp: datetime = Field(default_factory=datetime.utcnow, description="When failed (UTC)")
    metadata: dict[str, Any] = Field(
        default_factory=dict, description="Additional contextual information"
    )

    @field_validator("query")
    @classmethod
    def query_must_not_be_empty(cls, v: str) -> str:
        """Validate query is not empty or whitespace-only."""
        if not v.strip():
            raise ValueError("Query must not be empty or whitespace-only")
        return v.strip()

    @field_validator("error")
    @classmethod
    def error_must_not_be_empty(cls, v: str) -> str:
        """Validate error is not empty or whitespace-only."""
        if not v.strip():
            raise ValueError("Error must not be empty or whitespace-only")
        return v.strip()

    class Config:
        """Pydantic configuration."""

        frozen = True  # Immutable event
        json_schema_extra = {
            "example": {
                "query": "failed query",
                "search_type": "semantic",
                "result_count": 0,
                "execution_time_ms": 50.0,
                "user_id": "user123",
                "error": "Database connection timeout",
                "timestamp": "2024-01-15T10:30:00Z",
                "metadata": {"retry_count": 3},
            }
        }
