from datetime import UTC, datetime
from typing import Any, Literal

from pydantic import BaseModel, Field


class HealthCheck(BaseModel):
    """Basic health check."""

    status: Literal["healthy", "degraded", "unhealthy"]
    version: str
    timestamp: datetime = Field(default_factory=lambda: datetime.now(UTC))
    uptime_seconds: float | None = None


class ComponentStatus(BaseModel):
    """Individual component status."""

    status: Literal["healthy", "unhealthy", "degraded"]
    latency_ms: float | None = None
    message: str | None = None
    details: dict[str, Any] | None = None


class ReadinessStatus(BaseModel):
    """Readiness check response."""

    ready: bool
    timestamp: datetime = Field(default_factory=lambda: datetime.now(UTC))
    services: dict[str, str]


class LivenessStatus(BaseModel):
    """Liveness check response."""

    alive: bool
    timestamp: datetime = Field(default_factory=lambda: datetime.now(UTC))


class SystemStatus(BaseModel):
    """Detailed system status."""

    overall_status: Literal["healthy", "degraded", "unhealthy"]
    components: dict[str, ComponentStatus]
    timestamp: datetime = Field(default_factory=lambda: datetime.now(UTC))


class SystemMetrics(BaseModel):
    """System metrics for monitoring."""

    timestamp: datetime = Field(default_factory=lambda: datetime.now(UTC))
    system: dict[str, Any]
    memory: dict[str, float]
    gc: dict[str, Any]


class IndexingQueueStatus(BaseModel):
    """Indexing queue status."""

    pending_jobs: int
    processing_jobs: int
    failed_jobs: int
    avg_processing_time_ms: float


class StorageStatistics(BaseModel):
    """Storage usage statistics."""

    total_files: int
    total_size_bytes: int
    total_vectors: int
    database_size_mb: float
    redis_memory_mb: float
    minio_usage_bytes: int
