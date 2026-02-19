"""
API Error Handlers

FastAPI error handlers for consistent error responses with tracing support.
Exception classes are imported from src.exceptions for shared use across layers.
"""

from __future__ import annotations

from datetime import UTC, datetime
import logging

from fastapi import Request, status
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse
from opentelemetry import trace

# Import exception classes from shared module
from src.exceptions import (
    APIError,
    ContentExtractionError,
    DatabaseError,
    EmbeddingError,
    IndexingError,
    NotFoundError,
    RerankerError,
    SearchError,
    StorageError,
    ValidationError,
)

logger = logging.getLogger(__name__)

# Re-export for backward compatibility
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
    "api_error_handler",
    "validation_error_handler",
    "internal_error_handler",
    "get_trace_id",
]


def get_trace_id() -> str:
    """Get current trace ID from OpenTelemetry context."""
    span = trace.get_current_span()
    if span and span.get_span_context().is_valid:
        return format(span.get_span_context().trace_id, "032x")
    return "no-trace"


async def api_error_handler(request: Request, exc: APIError) -> JSONResponse:
    """Handle custom API errors with consistent format and trace ID."""
    trace_id = get_trace_id()

    logger.error(
        f"API error: {exc.code} - {exc.message}",
        extra={"details": exc.details, "trace_id": trace_id},
    )

    return JSONResponse(
        status_code=exc.status_code,
        content={
            "error": {
                "code": exc.code,
                "message": exc.message,
                "details": exc.details,
                "trace_id": trace_id,
                "timestamp": datetime.now(UTC).isoformat() + "Z",
            }
        },
    )


async def validation_error_handler(request: Request, exc: RequestValidationError) -> JSONResponse:
    """Handle Pydantic validation errors with consistent format and trace ID."""
    trace_id = get_trace_id()

    errors = []
    for error in exc.errors():
        errors.append(
            {
                "field": ".".join(str(loc) for loc in error["loc"]),
                "message": error["msg"],
                "type": error["type"],
            }
        )

    logger.warning(f"Validation error: {errors}", extra={"trace_id": trace_id})

    return JSONResponse(
        status_code=status.HTTP_400_BAD_REQUEST,
        content={
            "error": {
                "code": "VALIDATION_ERROR",
                "message": "Request validation failed",
                "details": {"errors": errors},
                "trace_id": trace_id,
                "timestamp": datetime.now(UTC).isoformat() + "Z",
            }
        },
    )


async def internal_error_handler(request: Request, exc: Exception) -> JSONResponse:
    """Handle unexpected internal errors with consistent format and trace ID."""
    trace_id = get_trace_id()

    logger.exception(
        "Unexpected internal error",
        extra={
            "path": request.url.path,
            "method": request.method,
            "error_type": type(exc).__name__,
            "trace_id": trace_id,
        },
    )

    return JSONResponse(
        status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
        content={
            "error": {
                "code": "INTERNAL_ERROR",
                "message": "An unexpected error occurred",
                "details": {},
                "trace_id": trace_id,
                "timestamp": datetime.now(UTC).isoformat() + "Z",
            }
        },
    )
