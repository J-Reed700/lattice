"""Observability module for tracing, metrics, and monitoring."""

from .database import (
    SLOW_QUERY_THRESHOLD_MS,
    get_pool_stats,
    log_pool_stats,
    setup_database_instrumentation,
    trace_query,
)
from .tracing import get_meter, get_tracer, setup_otel

__all__ = [
    # Tracing
    "setup_otel",
    "get_tracer",
    "get_meter",
    # Database observability
    "setup_database_instrumentation",
    "trace_query",
    "get_pool_stats",
    "log_pool_stats",
    "SLOW_QUERY_THRESHOLD_MS",
]
