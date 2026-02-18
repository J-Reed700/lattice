"""
Monitoring and observability module for Vault.

This module provides comprehensive monitoring capabilities including:
- Prometheus metrics collection
- Structured logging
- Error tracking with Sentry
- Health checks
- Performance monitoring
"""

from src.monitoring.health import HealthCheck, HealthStatus
from src.monitoring.logging import get_logger, setup_logging
from src.monitoring.metrics import (
    active_watch_folders,
    db_connections_active,
    db_connections_idle,
    db_query_duration_seconds,
    embedding_generation_duration_seconds,
    exports_total,
    files_indexed_total,
    files_uploaded_total,
    http_request_duration_seconds,
    http_request_size_bytes,
    http_requests_in_progress,
    http_requests_total,
    http_response_size_bytes,
    ocr_operations_total,
    search_query_duration_seconds,
    searches_total,
    storage_bytes,
    summarization_requests_total,
    system_cpu_usage,
    system_disk_available_bytes,
    system_disk_usage_bytes,
    system_memory_available_bytes,
    system_memory_usage_bytes,
)
from src.monitoring.middleware import (
    MetricsMiddleware,
    StructuredLoggingMiddleware,
)
from src.monitoring.sentry import capture_exception, capture_message, init_sentry

__all__ = [
    # Metrics
    "http_requests_total",
    "http_request_duration_seconds",
    "http_requests_in_progress",
    "http_request_size_bytes",
    "http_response_size_bytes",
    "files_uploaded_total",
    "files_indexed_total",
    "searches_total",
    "exports_total",
    "ocr_operations_total",
    "summarization_requests_total",
    "active_watch_folders",
    "storage_bytes",
    "db_query_duration_seconds",
    "db_connections_active",
    "db_connections_idle",
    "system_cpu_usage",
    "system_memory_usage_bytes",
    "system_memory_available_bytes",
    "system_disk_usage_bytes",
    "system_disk_available_bytes",
    "embedding_generation_duration_seconds",
    "search_query_duration_seconds",
    # Logging
    "setup_logging",
    "get_logger",
    # Middleware
    "MetricsMiddleware",
    "StructuredLoggingMiddleware",
    # Health
    "HealthCheck",
    "HealthStatus",
    # Sentry
    "init_sentry",
    "capture_exception",
    "capture_message",
]
