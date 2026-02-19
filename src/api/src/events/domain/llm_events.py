"""LLM-related domain events.

Events emitted by LLMService for Q&A operations, cache management, and health monitoring.
"""

from __future__ import annotations

from datetime import datetime
from typing import Any

from pydantic import BaseModel, Field, field_validator, model_validator

__all__ = [
    "QuestionAsked",
    "QuestionAnswered",
    "QuestionAnsweringFailed",
    "LLMCacheInvalidated",
    "LLMHealthCheckPerformed",
    "LLMModelsListed",
]


class QuestionAsked(BaseModel):
    """Event emitted when a question is asked to the LLM service.

    This event is emitted BEFORE the RAG pipeline begins processing. It captures
    the question and search parameters for analytics and monitoring.

    Attributes:
        question: The user's question
        user_id: Optional user who asked the question
        search_mode: Search mode used (vector, text, hybrid)
        max_context_docs: Maximum documents to include in context
        model: LLM model used for generation
        timestamp: When the question was asked (UTC)
        metadata: Additional contextual information (filters, temperature, etc.)

    Example:
        >>> event = QuestionAsked(
        ...     question="What is machine learning?",
        ...     search_mode="hybrid",
        ...     max_context_docs=5,
        ...     model="llama2"
        ... )
        >>> assert event.question == "What is machine learning?"
    """

    question: str = Field(..., min_length=1, max_length=2000, description="User's question")
    user_id: str | None = Field(None, description="User who asked the question")
    search_mode: str = Field(
        default="hybrid",
        pattern="^(vector|text|hybrid)$",
        description="Search mode used for retrieval",
    )
    max_context_docs: int = Field(
        default=5, ge=1, le=50, description="Maximum documents in context"
    )
    model: str = Field(default="llama2", description="LLM model used")
    timestamp: datetime = Field(
        default_factory=datetime.utcnow, description="When question was asked (UTC)"
    )
    metadata: dict[str, Any] = Field(
        default_factory=dict, description="Additional contextual information"
    )

    @field_validator("question")
    @classmethod
    def question_must_not_be_empty(cls, v: str) -> str:
        """Validate question is not empty or whitespace-only."""
        if not v.strip():
            raise ValueError("Question must not be empty or whitespace-only")
        return v.strip()

    class Config:
        """Pydantic configuration."""

        frozen = True  # Immutable event
        json_schema_extra = {
            "example": {
                "question": "What is machine learning?",
                "user_id": "user123",
                "search_mode": "hybrid",
                "max_context_docs": 5,
                "model": "llama2",
                "timestamp": "2024-01-15T10:30:00Z",
                "metadata": {
                    "temperature": 0.7,
                    "max_tokens": 2000,
                    "filters": {"tags": ["ai"]},
                },
            }
        }


class QuestionAnswered(BaseModel):
    """Event emitted when a question is successfully answered.

    This event is emitted AFTER the RAG pipeline completes. It includes metrics
    about the retrieval and generation process.

    Attributes:
        question: The user's question
        answer_length: Length of the generated answer in characters
        sources_count: Number of source documents used
        search_mode: Search mode used for retrieval
        model: LLM model used for generation
        execution_time_ms: Total execution time in milliseconds
        user_id: Optional user who asked the question
        timestamp: When the answer was generated (UTC)
        metadata: Additional metrics (tokens, cache hit, etc.)

    Example:
        >>> event = QuestionAnswered(
        ...     question="What is ML?",
        ...     answer_length=500,
        ...     sources_count=3,
        ...     search_mode="hybrid",
        ...     model="llama2",
        ...     execution_time_ms=1250.5
        ... )
        >>> assert event.answer_length == 500
    """

    question: str = Field(..., min_length=1, max_length=2000, description="User's question")
    answer_length: int = Field(..., ge=0, description="Length of generated answer (characters)")
    sources_count: int = Field(..., ge=0, description="Number of source documents used")
    search_mode: str = Field(
        default="hybrid",
        pattern="^(vector|text|hybrid)$",
        description="Search mode used for retrieval",
    )
    model: str = Field(default="llama2", description="LLM model used")
    execution_time_ms: float = Field(..., ge=0, description="Total execution time (ms)")
    user_id: str | None = Field(None, description="User who asked the question")
    timestamp: datetime = Field(
        default_factory=datetime.utcnow, description="When answer was generated (UTC)"
    )
    metadata: dict[str, Any] = Field(
        default_factory=dict, description="Additional metrics and information"
    )

    @field_validator("question")
    @classmethod
    def question_must_not_be_empty(cls, v: str) -> str:
        """Validate question is not empty or whitespace-only."""
        if not v.strip():
            raise ValueError("Question must not be empty or whitespace-only")
        return v.strip()

    class Config:
        """Pydantic configuration."""

        frozen = True  # Immutable event
        json_schema_extra = {
            "example": {
                "question": "What is machine learning?",
                "answer_length": 500,
                "sources_count": 3,
                "search_mode": "hybrid",
                "model": "llama2",
                "execution_time_ms": 1250.5,
                "user_id": "user123",
                "timestamp": "2024-01-15T10:31:00Z",
                "metadata": {
                    "cache_hit": True,
                    "tokens_generated": 150,
                    "temperature": 0.7,
                },
            }
        }


class QuestionAnsweringFailed(BaseModel):
    """Event emitted when answering a question fails.

    This event is emitted when an error occurs during the RAG pipeline (search or
    generation). Subscribers can use this for error tracking and alerting.

    Attributes:
        question: The user's question that failed
        error: Error message describing the failure
        error_type: Type of error (search, generation, connection, timeout, other)
        search_mode: Search mode attempted
        model: LLM model attempted
        user_id: Optional user who asked the question
        timestamp: When the failure occurred (UTC)
        metadata: Additional error context

    Example:
        >>> event = QuestionAnsweringFailed(
        ...     question="Test?",
        ...     error="Ollama connection timeout",
        ...     error_type="connection",
        ...     search_mode="hybrid",
        ...     model="llama2"
        ... )
        >>> assert event.error_type == "connection"
    """

    question: str = Field(..., min_length=1, max_length=2000, description="User's question")
    error: str = Field(..., min_length=1, description="Error message")
    error_type: str = Field(
        default="other",
        pattern="^(search|generation|connection|timeout|other)$",
        description="Type of error that occurred",
    )
    search_mode: str = Field(
        default="hybrid",
        pattern="^(vector|text|hybrid)$",
        description="Search mode attempted",
    )
    model: str = Field(default="llama2", description="LLM model attempted")
    user_id: str | None = Field(None, description="User who asked the question")
    timestamp: datetime = Field(
        default_factory=datetime.utcnow, description="When failure occurred (UTC)"
    )
    metadata: dict[str, Any] = Field(
        default_factory=dict, description="Additional error context"
    )

    @field_validator("question")
    @classmethod
    def question_must_not_be_empty(cls, v: str) -> str:
        """Validate question is not empty or whitespace-only."""
        if not v.strip():
            raise ValueError("Question must not be empty or whitespace-only")
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
                "question": "What is ML?",
                "error": "Ollama connection timeout",
                "error_type": "connection",
                "search_mode": "hybrid",
                "model": "llama2",
                "user_id": "user123",
                "timestamp": "2024-01-15T10:30:30Z",
                "metadata": {"retry_count": 3, "timeout_seconds": 120},
            }
        }


class LLMCacheInvalidated(BaseModel):
    """Event emitted when the LLM prompt cache is invalidated.

    This event is emitted when the cache is cleared, typically after document
    corpus changes to prevent serving stale cached responses.

    Attributes:
        reason: Reason for cache invalidation
        cache_size_before: Number of entries before invalidation
        timestamp: When cache was invalidated (UTC)
        metadata: Additional context

    Example:
        >>> event = LLMCacheInvalidated(
        ...     reason="Document corpus updated",
        ...     cache_size_before=150
        ... )
        >>> assert event.reason == "Document corpus updated"
    """

    reason: str = Field(default="manual", description="Reason for invalidation")
    cache_size_before: int = Field(default=0, ge=0, description="Cache entries before clearing")
    timestamp: datetime = Field(
        default_factory=datetime.utcnow, description="When cache invalidated (UTC)"
    )
    metadata: dict[str, Any] = Field(default_factory=dict, description="Additional context")

    class Config:
        """Pydantic configuration."""

        frozen = True  # Immutable event
        json_schema_extra = {
            "example": {
                "reason": "Document corpus updated",
                "cache_size_before": 150,
                "timestamp": "2024-01-15T10:30:00Z",
                "metadata": {"documents_added": 10, "documents_removed": 2},
            }
        }


class LLMHealthCheckPerformed(BaseModel):
    """Event emitted when an LLM health check is performed.

    This event is emitted after checking Ollama server availability.

    Attributes:
        is_healthy: Whether Ollama is responding
        response_time_ms: Health check response time
        timestamp: When check was performed (UTC)
        metadata: Additional health information

    Example:
        >>> event = LLMHealthCheckPerformed(
        ...     is_healthy=True,
        ...     response_time_ms=45.2
        ... )
        >>> assert event.is_healthy is True
    """

    is_healthy: bool = Field(..., description="Whether Ollama is responding")
    response_time_ms: float = Field(..., ge=0, description="Response time in milliseconds")
    timestamp: datetime = Field(
        default_factory=datetime.utcnow, description="When check performed (UTC)"
    )
    metadata: dict[str, Any] = Field(default_factory=dict, description="Additional health info")

    class Config:
        """Pydantic configuration."""

        frozen = True  # Immutable event
        json_schema_extra = {
            "example": {
                "is_healthy": True,
                "response_time_ms": 45.2,
                "timestamp": "2024-01-15T10:30:00Z",
                "metadata": {"ollama_version": "0.1.17", "models_available": 3},
            }
        }


class LLMModelsListed(BaseModel):
    """Event emitted when available LLM models are listed.

    This event is emitted after successfully retrieving the list of available models
    from Ollama.

    Attributes:
        models_count: Number of available models
        models: List of model names
        timestamp: When models were listed (UTC)
        metadata: Additional model information

    Example:
        >>> event = LLMModelsListed(
        ...     models_count=3,
        ...     models=["llama2", "mistral", "codellama"]
        ... )
        >>> assert event.models_count == 3
    """

    models_count: int = Field(..., ge=0, description="Number of available models")
    models: list[str] = Field(default_factory=list, description="List of model names")
    timestamp: datetime = Field(
        default_factory=datetime.utcnow, description="When models listed (UTC)"
    )
    metadata: dict[str, Any] = Field(default_factory=dict, description="Additional model info")

    @model_validator(mode="after")
    def validate_models_count(self) -> "LLMModelsListed":
        """Validate models_count matches length of models list."""
        if self.models_count != len(self.models):
            raise ValueError(
                f"models_count ({self.models_count}) does not match models list length ({len(self.models)})"
            )
        return self

    class Config:
        """Pydantic configuration."""

        frozen = True  # Immutable event
        json_schema_extra = {
            "example": {
                "models_count": 3,
                "models": ["llama2", "mistral", "codellama"],
                "timestamp": "2024-01-15T10:30:00Z",
                "metadata": {"ollama_version": "0.1.17", "total_size_gb": 12.5},
            }
        }
