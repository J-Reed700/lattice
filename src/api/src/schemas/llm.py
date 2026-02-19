"""Pydantic schemas for LLM API endpoints."""

from __future__ import annotations

from typing import Any

from pydantic import BaseModel, Field, field_validator


class AskRequest(BaseModel):
    """Request schema for asking a question."""

    question: str = Field(
        ..., min_length=1, max_length=2000, description="The question to ask about your documents"
    )
    max_context_docs: int = Field(
        default=5, ge=1, le=20, description="Maximum number of documents to use for context"
    )
    search_mode: str = Field(
        default="hybrid", description="Search mode: 'vector', 'text', or 'hybrid'"
    )
    model: str | None = Field(default=None, description="Override the default LLM model")
    temperature: float | None = Field(
        default=0.7, ge=0.0, le=2.0, description="Sampling temperature for generation"
    )
    max_tokens: int | None = Field(
        default=2000, ge=100, le=8000, description="Maximum tokens to generate"
    )

    @field_validator("search_mode")
    @classmethod
    def validate_search_mode(cls, v: str) -> str:
        """Validate search mode is one of the allowed values."""
        allowed_modes = ["vector", "text", "hybrid"]
        if v not in allowed_modes:
            raise ValueError(f"search_mode must be one of {allowed_modes}")
        return v


class SourceDocument(BaseModel):
    """Source document referenced in the answer."""

    id: int = Field(..., description="Source number")
    file_path: str = Field(..., description="Path to the source file")
    filename: str = Field(..., description="Name of the source file")
    score: float = Field(..., description="Relevance score")
    snippet: str | None = Field(None, description="Text snippet from the document")
    modified_at: str | None = Field(None, description="Last modified timestamp")


class AskResponse(BaseModel):
    """Response schema for ask question endpoint (non-streaming)."""

    answer: str = Field(..., description="The generated answer")
    sources: list[SourceDocument] = Field(..., description="Source documents used")
    metadata: dict[str, Any] = Field(..., description="Additional metadata")


class HealthCheckResponse(BaseModel):
    """Response schema for health check endpoint."""

    status: str = Field(..., description="Health status: 'healthy' or 'unhealthy'")
    ollama_available: bool = Field(..., description="Whether Ollama is available")
    model: str = Field(..., description="Default model being used")


class ListModelsResponse(BaseModel):
    """Response schema for list models endpoint."""

    models: list[str] = Field(..., description="Available model names")
    default_model: str = Field(..., description="Default model being used")


class ErrorResponse(BaseModel):
    """Error response schema."""

    error: str = Field(..., description="Error message")
    detail: str | None = Field(None, description="Additional error details")


# =============================================================================
# Function Calling Schemas
# =============================================================================


class FunctionCallRequest(BaseModel):
    """Request schema for executing a single function call."""

    function_name: str = Field(
        ...,
        min_length=1,
        max_length=100,
        description="Name of the function to call",
        examples=["semantic_search", "get_document", "web_search"],
    )
    arguments: dict[str, Any] = Field(
        ...,
        description="Function arguments as key-value pairs",
        examples=[{"query": "machine learning", "limit": 10}],
    )
    user_id: str | None = Field(
        default=None,
        description="User ID for audit logging",
    )


class FunctionCallResponse(BaseModel):
    """Response schema for function call execution."""

    success: bool = Field(..., description="Whether function call succeeded")
    result: dict[str, Any] | None = Field(
        default=None,
        description="Function execution result (if successful)",
    )
    error: dict[str, str] | None = Field(
        default=None,
        description="Error details (if failed)",
    )
    execution_time_ms: float | None = Field(
        default=None,
        description="Execution time in milliseconds",
    )


class QueryWithFunctionsRequest(BaseModel):
    """Request schema for LLM query with function calling support."""

    query: str = Field(
        ...,
        min_length=1,
        max_length=2000,
        description="User query to the LLM",
    )
    model: str | None = Field(
        default=None,
        description="Model name (Ollama model)",
    )
    enable_functions: bool = Field(
        default=True,
        description="Whether to enable function calling",
    )
    max_turns: int = Field(
        default=5,
        ge=1,
        le=10,
        description="Maximum agentic turns (function calls + responses)",
    )
    temperature: float = Field(
        default=0.7,
        ge=0.0,
        le=2.0,
        description="Sampling temperature",
    )


class FunctionCallInvocation(BaseModel):
    """Record of a function call invocation."""

    function_name: str
    arguments: dict[str, Any]
    result: dict[str, Any] | None = None
    error: str | None = None
    execution_time_ms: float


class QueryWithFunctionsResponse(BaseModel):
    """Response schema for LLM query with function calling."""

    response: str = Field(..., description="Final LLM response")
    function_calls: list[FunctionCallInvocation] = Field(
        default_factory=list,
        description="List of function calls made",
    )
    total_turns: int = Field(..., description="Total agentic turns")
    total_time_ms: float = Field(..., description="Total query time in milliseconds")
