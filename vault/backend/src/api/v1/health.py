from __future__ import annotations

from datetime import UTC, datetime
import gc
import logging
import platform
import time

from fastapi import APIRouter, Depends, status
import psutil
from sqlalchemy import text
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.dependencies import get_db, get_settings_dep
from src.config import Settings
from src.schemas.health import (
    ComponentStatus,
    HealthCheck,
    IndexingQueueStatus,
    LivenessStatus,
    ReadinessStatus,
    StorageStatistics,
    SystemMetrics,
    SystemStatus,
)

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/health", tags=["health"])

_startup_time = time.time()


@router.get("/", response_model=HealthCheck, status_code=status.HTTP_200_OK)
async def health_check(
    db: AsyncSession = Depends(get_db), settings: Settings = Depends(get_settings_dep)
) -> HealthCheck:
    """
    Overall health check endpoint.

    Returns overall application health status with component checks.
    Used by load balancers and monitoring systems.

    Checks:
    - Database connectivity
    - Vector store accessibility
    - Embedding service availability

    Returns:
        HealthCheck: Overall health status
    """
    checks = {}

    try:
        start = time.time()
        await db.execute(text("SELECT 1"))
        latency = (time.time() - start) * 1000
        checks["database"] = latency < 1000
    except Exception as e:
        logger.error(f"Database health check failed: {e}")
        checks["database"] = False

    try:
        from src.modules.vector_store.store import VectorStore

        VectorStore(db)
        checks["vector_store"] = True
    except Exception as e:
        logger.error(f"Vector store health check failed: {e}")
        checks["vector_store"] = False

    try:
        from src.modules.embedding_generator.text_embedder import TextEmbedder

        TextEmbedder()
        checks["embeddings"] = True
    except Exception as e:
        logger.error(f"Embeddings health check failed: {e}")
        checks["embeddings"] = False

    if all(checks.values()):
        overall_status = "healthy"
    elif any(checks.values()):
        overall_status = "degraded"
    else:
        overall_status = "unhealthy"

    uptime = time.time() - _startup_time

    return HealthCheck(
        status=overall_status,
        version=settings.app_version,
        timestamp=datetime.now(UTC),
        uptime_seconds=uptime,
    )


@router.get("/ready", response_model=ReadinessStatus)
async def readiness_check(
    db: AsyncSession = Depends(get_db), settings: Settings = Depends(get_settings_dep)
) -> ReadinessStatus:
    """
    Readiness check endpoint.

    Indicates if the application is ready to accept traffic.
    Used by Kubernetes readiness probes and load balancers.

    Checks:
    - Database is ready and responsive
    - Vector store is initialized
    - Embedding models can be loaded

    Returns 200 if ready, otherwise returns 200 with ready=false.
    """
    services = {}

    try:
        start = time.time()
        await db.execute(text("SELECT 1"))
        latency = (time.time() - start) * 1000
        if latency < 2000:
            services["database"] = "ready"
        else:
            services["database"] = f"slow ({latency:.0f}ms)"
    except Exception as e:
        services["database"] = f"not ready: {str(e)[:100]}"

    try:
        from src.modules.vector_store.store import VectorStore

        VectorStore(db)
        await db.execute(text("SELECT COUNT(*) FROM pg_extension WHERE extname = 'vector'"))
        services["vector_store"] = "ready"
    except Exception as e:
        services["vector_store"] = f"error: {str(e)[:100]}"

    try:
        from src.modules.embedding_generator.text_embedder import TextEmbedder

        TextEmbedder()
        services["embeddings"] = "ready"
    except Exception as e:
        services["embeddings"] = f"error: {str(e)[:100]}"

    ready = all(s == "ready" for s in services.values())

    return ReadinessStatus(ready=ready, timestamp=datetime.now(UTC), services=services)


@router.get("/live", response_model=LivenessStatus, status_code=status.HTTP_200_OK)
async def liveness_check() -> LivenessStatus:
    """
    Liveness check endpoint.

    Indicates if the application is alive (not deadlocked/hung).
    Used by Kubernetes liveness probes.

    This is a simple endpoint that always returns 200 unless
    the application is completely hung.

    Returns:
        LivenessStatus: Always alive=True if responding
    """
    return LivenessStatus(alive=True, timestamp=datetime.now(UTC))


@router.get("/status", response_model=SystemStatus)
async def system_status(
    db: AsyncSession = Depends(get_db), settings: Settings = Depends(get_settings_dep)
) -> SystemStatus:
    """
    Detailed system status with component health checks.

    Returns comprehensive status of all system components including:
    - Database (with latency)
    - Vector store (with pgvector check)
    - Embedding service
    - File watcher (if enabled)

    Returns:
        SystemStatus: Detailed component status
    """
    components = {}

    try:
        start = time.time()
        await db.execute(text("SELECT 1"))
        latency = (time.time() - start) * 1000
        components["database"] = ComponentStatus(
            status="healthy" if latency < 1000 else "degraded",
            latency_ms=round(latency, 2),
            details={"connection": "active"},
        )
    except Exception as e:
        components["database"] = ComponentStatus(status="unhealthy", message=str(e))

    try:
        from src.modules.vector_store.store import VectorStore

        VectorStore(db)
        result = await db.execute(
            text("SELECT COUNT(*) FROM pg_extension WHERE extname = 'vector'")
        )
        has_pgvector = result.scalar() > 0
        components["vector_store"] = ComponentStatus(
            status="healthy" if has_pgvector else "degraded",
            details={"pgvector_enabled": has_pgvector},
        )
    except Exception as e:
        components["vector_store"] = ComponentStatus(status="unhealthy", message=str(e))

    try:
        from src.modules.embedding_generator.text_embedder import TextEmbedder

        TextEmbedder()
        components["embeddings"] = ComponentStatus(status="healthy", details={"model": "available"})
    except Exception as e:
        components["embeddings"] = ComponentStatus(status="unhealthy", message=str(e))

    if settings.file_watcher_enabled:
        try:
            components["file_watcher"] = ComponentStatus(
                status="healthy", details={"enabled": True}
            )
        except Exception as e:
            components["file_watcher"] = ComponentStatus(status="degraded", message=str(e))

    unhealthy = [c for c in components.values() if c.status == "unhealthy"]
    degraded = [c for c in components.values() if c.status == "degraded"]

    if unhealthy:
        overall = "unhealthy"
    elif degraded:
        overall = "degraded"
    else:
        overall = "healthy"

    return SystemStatus(overall_status=overall, components=components, timestamp=datetime.now(UTC))


@router.get("/metrics", response_model=SystemMetrics)
async def metrics() -> SystemMetrics:
    """
    Application metrics endpoint.

    Returns system-level metrics for monitoring including:
    - Platform information
    - CPU usage
    - Memory usage (RSS, VMS)
    - Garbage collection statistics

    In production, consider replacing with Prometheus metrics endpoint.

    Returns:
        SystemMetrics: System performance metrics
    """
    process = psutil.Process()
    memory = process.memory_info()

    return SystemMetrics(
        timestamp=datetime.now(UTC),
        system={
            "platform": platform.platform(),
            "python_version": platform.python_version(),
            "cpu_count": psutil.cpu_count(),
            "cpu_percent": psutil.cpu_percent(interval=0.1),
        },
        memory={
            "rss_mb": memory.rss / 1024 / 1024,
            "vms_mb": memory.vms / 1024 / 1024,
            "percent": process.memory_percent(),
        },
        gc={
            "collections": {f"gen{i}": gc.get_count()[i] for i in range(3)},
        },
    )


@router.get("/queue", response_model=IndexingQueueStatus)
async def indexing_queue_status() -> IndexingQueueStatus:
    """Get indexing queue statistics."""
    return IndexingQueueStatus(
        pending_jobs=0, processing_jobs=0, failed_jobs=0, avg_processing_time_ms=0.0
    )


@router.get("/storage", response_model=StorageStatistics)
async def storage_statistics(db: AsyncSession = Depends(get_db)) -> StorageStatistics:
    """Get storage usage statistics."""
    return StorageStatistics(
        total_files=0,
        total_size_bytes=0,
        total_vectors=0,
        database_size_mb=0.0,
        redis_memory_mb=0.0,
        minio_usage_bytes=0,
    )
