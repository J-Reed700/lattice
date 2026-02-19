"""
Prometheus metrics definitions for Vault.

This module defines all application, business, and system metrics
that are exposed via the /metrics endpoint for Prometheus scraping.
"""

import time

from prometheus_client import Counter, Gauge, Histogram, Info
import psutil

# Application version info
app_info = Info("vault_app", "Vault application information")

# ============================================================================
# HTTP Request Metrics
# ============================================================================

http_requests_total = Counter(
    "vault_http_requests_total",
    "Total HTTP requests",
    ["method", "endpoint", "status_code"],
)

http_request_duration_seconds = Histogram(
    "vault_http_request_duration_seconds",
    "HTTP request duration in seconds",
    ["method", "endpoint"],
    buckets=[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
)

http_requests_in_progress = Gauge(
    "vault_http_requests_in_progress",
    "Number of HTTP requests currently being processed",
    ["method", "endpoint"],
)

http_request_size_bytes = Histogram(
    "vault_http_request_size_bytes",
    "HTTP request size in bytes",
    ["method", "endpoint"],
    buckets=[100, 1000, 10000, 100000, 1000000, 10000000, 100000000],
)

http_response_size_bytes = Histogram(
    "vault_http_response_size_bytes",
    "HTTP response size in bytes",
    ["method", "endpoint"],
    buckets=[100, 1000, 10000, 100000, 1000000, 10000000, 100000000],
)

# ============================================================================
# Business Metrics
# ============================================================================

files_uploaded_total = Counter(
    "vault_files_uploaded_total",
    "Total number of files uploaded",
    ["mime_type"],
)

files_indexed_total = Counter(
    "vault_files_indexed_total",
    "Total number of files successfully indexed",
    ["mime_type"],
)

files_indexing_failed_total = Counter(
    "vault_files_indexing_failed_total",
    "Total number of files that failed indexing",
    ["mime_type", "error_type"],
)

searches_total = Counter(
    "vault_searches_total",
    "Total number of searches performed",
    ["search_type"],
)

exports_total = Counter(
    "vault_exports_total",
    "Total number of export operations",
    ["format", "status"],
)

ocr_operations_total = Counter(
    "vault_ocr_operations_total",
    "Total number of OCR operations",
    ["status"],
)

summarization_requests_total = Counter(
    "vault_summarization_requests_total",
    "Total number of summarization requests",
    ["status"],
)

active_watch_folders = Gauge(
    "vault_active_watch_folders",
    "Number of currently active watch folders",
)

storage_bytes = Gauge(
    "vault_storage_bytes",
    "Storage usage in bytes",
    ["category"],
)

# ============================================================================
# Database Metrics
# ============================================================================

db_query_duration_seconds = Histogram(
    "vault_db_query_duration_seconds",
    "Database query duration in seconds",
    ["query_type"],
    buckets=[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0],
)

db_connections_active = Gauge(
    "vault_db_connections_active",
    "Number of active database connections",
)

db_connections_idle = Gauge(
    "vault_db_connections_idle",
    "Number of idle database connections",
)

db_connection_errors_total = Counter(
    "vault_db_connection_errors_total",
    "Total number of database connection errors",
    ["error_type"],
)

db_query_errors_total = Counter(
    "vault_db_query_errors_total",
    "Total number of database query errors",
    ["query_type", "error_type"],
)

# ============================================================================
# Embedding & Search Metrics
# ============================================================================

embedding_generation_duration_seconds = Histogram(
    "vault_embedding_generation_duration_seconds",
    "Time to generate embeddings in seconds",
    ["model"],
    buckets=[0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
)

embedding_generation_total = Counter(
    "vault_embedding_generation_total",
    "Total number of embeddings generated",
    ["model", "status"],
)

search_query_duration_seconds = Histogram(
    "vault_search_query_duration_seconds",
    "Search query duration in seconds",
    ["search_type"],
    buckets=[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0],
)

search_results_count = Histogram(
    "vault_search_results_count",
    "Number of results returned per search",
    ["search_type"],
    buckets=[0, 1, 5, 10, 25, 50, 100, 250, 500],
)

# ============================================================================
# System Metrics
# ============================================================================

system_cpu_usage = Gauge(
    "vault_system_cpu_usage_percent",
    "CPU usage percentage",
)

system_memory_usage_bytes = Gauge(
    "vault_system_memory_usage_bytes",
    "Memory usage in bytes",
)

system_memory_available_bytes = Gauge(
    "vault_system_memory_available_bytes",
    "Available memory in bytes",
)

system_disk_usage_bytes = Gauge(
    "vault_system_disk_usage_bytes",
    "Disk usage in bytes",
    ["path"],
)

system_disk_available_bytes = Gauge(
    "vault_system_disk_available_bytes",
    "Available disk space in bytes",
    ["path"],
)

system_open_file_descriptors = Gauge(
    "vault_system_open_file_descriptors",
    "Number of open file descriptors",
)

system_thread_count = Gauge(
    "vault_system_thread_count",
    "Number of threads",
)

# ============================================================================
# Cache Metrics
# ============================================================================

cache_hits_total = Counter(
    "vault_cache_hits_total",
    "Total number of cache hits",
    ["cache_name"],
)

cache_misses_total = Counter(
    "vault_cache_misses_total",
    "Total number of cache misses",
    ["cache_name"],
)

cache_size_bytes = Gauge(
    "vault_cache_size_bytes",
    "Cache size in bytes",
    ["cache_name"],
)

# ============================================================================
# Task Queue Metrics (Celery)
# ============================================================================

task_queue_length = Gauge(
    "vault_task_queue_length",
    "Number of tasks in queue",
    ["queue_name"],
)

task_execution_duration_seconds = Histogram(
    "vault_task_execution_duration_seconds",
    "Task execution duration in seconds",
    ["task_name"],
    buckets=[0.1, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0],
)

task_execution_total = Counter(
    "vault_task_execution_total",
    "Total number of task executions",
    ["task_name", "status"],
)

# ============================================================================
# Helper Functions
# ============================================================================


def update_system_metrics() -> None:
    """
    Update system metrics (CPU, memory, disk).
    Should be called periodically.
    """
    try:
        system_cpu_usage.set(psutil.cpu_percent(interval=None))

        mem = psutil.virtual_memory()
        system_memory_usage_bytes.set(mem.used)
        system_memory_available_bytes.set(mem.available)

        process = psutil.Process()
        try:
            system_open_file_descriptors.set(process.num_fds())
        except (AttributeError, NotImplementedError):
            pass

        system_thread_count.set(process.num_threads())

    except Exception:
        pass


def update_disk_metrics(path: str = "/") -> None:
    """
    Update disk usage metrics for a given path.

    Args:
        path: File system path to check
    """
    try:
        disk = psutil.disk_usage(path)
        system_disk_usage_bytes.labels(path=path).set(disk.used)
        system_disk_available_bytes.labels(path=path).set(disk.free)
    except Exception:
        pass


def update_storage_metrics(
    files_size: int,
    embeddings_size: int,
    database_size: int,
    cache_size: int,
) -> None:
    """
    Update storage usage metrics by category.

    Args:
        files_size: Size of stored files in bytes
        embeddings_size: Size of embeddings storage in bytes
        database_size: Size of database in bytes
        cache_size: Size of cache in bytes
    """
    storage_bytes.labels(category="files").set(files_size)
    storage_bytes.labels(category="embeddings").set(embeddings_size)
    storage_bytes.labels(category="database").set(database_size)
    storage_bytes.labels(category="cache").set(cache_size)


class MetricsTimer:
    """
    Context manager for timing operations and recording to histogram.

    Example:
        with MetricsTimer(db_query_duration_seconds.labels(query_type="select")):
            # Execute query
            result = await db.execute(query)
    """

    def __init__(self, histogram_metric):
        self.metric = histogram_metric
        self.start_time: float | None = None

    def __enter__(self):
        self.start_time = time.time()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        if self.start_time is not None:
            duration = time.time() - self.start_time
            self.metric.observe(duration)


async def collect_db_pool_metrics(engine) -> None:
    """
    Collect database connection pool metrics.

    Args:
        engine: SQLAlchemy async engine
    """
    try:
        pool = engine.pool
        db_connections_active.set(pool.checkedout())
        db_connections_idle.set(pool.size() - pool.checkedout())
    except Exception:
        pass
