"""Database query observability with OpenTelemetry.

Provides comprehensive observability for database operations:
- Query latency metrics (p50, p95, p99)
- Slow query logging (>100ms)
- Query count per endpoint
- Connection pool monitoring
- Failed query tracking
"""

from __future__ import annotations

from collections.abc import Generator
from contextlib import contextmanager
import logging
import time
from typing import Any

from opentelemetry import metrics, trace
from sqlalchemy import event
from sqlalchemy.engine import Engine
from sqlalchemy.pool import Pool

logger = logging.getLogger(__name__)

# Get OpenTelemetry instruments
tracer = trace.get_tracer(__name__)
meter = metrics.get_meter(__name__)

# Metrics
query_counter = meter.create_counter(
    "db.query.count", description="Number of database queries executed", unit="1"
)

query_latency = meter.create_histogram(
    "db.query.latency", description="Database query execution time", unit="ms"
)

slow_query_counter = meter.create_counter(
    "db.query.slow", description="Number of slow queries (>100ms)", unit="1"
)

failed_query_counter = meter.create_counter(
    "db.query.failed", description="Number of failed queries", unit="1"
)

pool_size_gauge = meter.create_up_down_counter(
    "db.pool.size", description="Current connection pool size", unit="1"
)

pool_checked_out = meter.create_up_down_counter(
    "db.pool.checked_out", description="Number of connections checked out from pool", unit="1"
)

pool_overflow = meter.create_up_down_counter(
    "db.pool.overflow", description="Number of connections in overflow", unit="1"
)

# Constants
SLOW_QUERY_THRESHOLD_MS = 100


def setup_database_instrumentation(engine: Engine) -> None:
    """Setup database instrumentation with OpenTelemetry.

    Args:
        engine: SQLAlchemy engine to instrument

    This adds:
    - Query timing and metrics
    - Slow query logging
    - Connection pool monitoring
    - Query tracing integration
    """
    logger.info("Setting up database observability instrumentation")

    # Query execution tracking
    @event.listens_for(engine, "before_cursor_execute")
    def before_cursor_execute(conn, cursor, statement, parameters, context, executemany):
        """Track query start time."""
        context._query_start_time = time.time()

    @event.listens_for(engine, "after_cursor_execute")
    def after_cursor_execute(conn, cursor, statement, parameters, context, executemany):
        """Record query metrics after execution."""
        total_time = time.time() - context._query_start_time
        latency_ms = total_time * 1000

        # Get query type (SELECT, INSERT, UPDATE, DELETE)
        query_type = statement.strip().split()[0].upper() if statement else "UNKNOWN"

        # Record metrics
        query_counter.add(1, {"query_type": query_type})
        query_latency.record(latency_ms, {"query_type": query_type})

        # Log slow queries
        if latency_ms > SLOW_QUERY_THRESHOLD_MS:
            slow_query_counter.add(1, {"query_type": query_type})
            logger.warning(
                "Slow query detected",
                extra={
                    "latency_ms": latency_ms,
                    "query_type": query_type,
                    "statement": statement[:200],  # Truncate long queries
                    "parameters": str(parameters)[:100] if parameters else None,
                },
            )

    @event.listens_for(engine, "handle_error")
    def handle_error(exception_context):
        """Track failed queries."""
        statement = exception_context.statement
        query_type = statement.strip().split()[0].upper() if statement else "UNKNOWN"

        failed_query_counter.add(1, {"query_type": query_type})
        logger.error(
            "Query failed",
            extra={
                "query_type": query_type,
                "error": str(exception_context.original_exception),
                "statement": statement[:200] if statement else None,
            },
        )

    # Connection pool monitoring
    @event.listens_for(engine.pool, "connect")
    def on_connect(dbapi_conn, connection_record):
        """Track new connection creation."""
        pool_size_gauge.add(1, {"pool": "main"})
        logger.debug("New database connection created")

    @event.listens_for(engine.pool, "checkout")
    def on_checkout(dbapi_conn, connection_record, connection_proxy):
        """Track connection checkout from pool."""
        pool_checked_out.add(1, {"pool": "main"})
        logger.debug("Connection checked out from pool")

    @event.listens_for(engine.pool, "checkin")
    def on_checkin(dbapi_conn, connection_record):
        """Track connection checkin to pool."""
        pool_checked_out.add(-1, {"pool": "main"})
        logger.debug("Connection checked in to pool")

    @event.listens_for(engine.pool, "close")
    def on_close(dbapi_conn, connection_record):
        """Track connection close."""
        pool_size_gauge.add(-1, {"pool": "main"})
        logger.debug("Database connection closed")

    logger.info("Database observability instrumentation enabled")


@contextmanager
def trace_query(query_name: str, **attributes: Any) -> Generator[trace.Span, None, None]:
    """Context manager for tracing database queries.

    Usage:
        with trace_query("fetch_documents", user_id=123):
            docs = await db.execute(select(Document).where(...))

    Args:
        query_name: Name of the query operation
        **attributes: Additional attributes to add to span

    Yields:
        OpenTelemetry span
    """
    with tracer.start_as_current_span(f"db.query.{query_name}") as span:
        span.set_attribute("db.operation", query_name)

        for key, value in attributes.items():
            span.set_attribute(f"db.{key}", value)

        start_time = time.time()

        try:
            yield span

            latency_ms = (time.time() - start_time) * 1000
            span.set_attribute("db.latency_ms", latency_ms)
            span.set_attribute("success", True)

            if latency_ms > SLOW_QUERY_THRESHOLD_MS:
                span.set_attribute("slow_query", True)
                logger.warning(
                    f"Slow query: {query_name} took {latency_ms:.2f}ms",
                    extra={"query_name": query_name, "latency_ms": latency_ms, **attributes},
                )

        except Exception as e:
            span.record_exception(e)
            span.set_status(trace.Status(trace.StatusCode.ERROR, str(e)))
            span.set_attribute("success", False)
            span.set_attribute("error.type", type(e).__name__)
            raise


def get_pool_stats(pool: Pool) -> dict[str, int]:
    """Get current connection pool statistics.

    Args:
        pool: SQLAlchemy connection pool

    Returns:
        Dictionary with pool statistics:
        - size: Current pool size
        - checked_out: Connections currently in use
        - overflow: Connections in overflow
        - checked_in: Available connections
    """
    return {
        "size": pool.size(),
        "checked_out": pool.checkedout(),
        "overflow": pool.overflow(),
        "checked_in": pool.size() - pool.checkedout(),
    }


def log_pool_stats(pool: Pool) -> None:
    """Log current connection pool statistics.

    Args:
        pool: SQLAlchemy connection pool
    """
    stats = get_pool_stats(pool)
    logger.info(
        "Connection pool stats",
        extra={
            "pool_size": stats["size"],
            "checked_out": stats["checked_out"],
            "overflow": stats["overflow"],
            "available": stats["checked_in"],
        },
    )


# Export public API
__all__ = [
    "SLOW_QUERY_THRESHOLD_MS",
    "get_pool_stats",
    "log_pool_stats",
    "setup_database_instrumentation",
    "trace_query",
]
