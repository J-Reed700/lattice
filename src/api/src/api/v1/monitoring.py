"""
Monitoring endpoints for metrics, health checks, and observability.
"""

from __future__ import annotations

from fastapi import APIRouter, Depends, Response, status
from prometheus_client import CONTENT_TYPE_LATEST, generate_latest
from sqlalchemy.ext.asyncio import AsyncSession
import structlog

from src.db import get_engine, get_session
from src.monitoring.health import HealthCheck, HealthStatus
from src.monitoring.metrics import (
    update_disk_metrics,
    update_system_metrics,
)

logger = structlog.get_logger(__name__)
router = APIRouter(prefix="/monitoring", tags=["monitoring"])


@router.get("/metrics", include_in_schema=False)
async def metrics() -> Response:
    """
    Prometheus metrics endpoint.

    Exposes metrics in Prometheus text format for scraping.

    Returns:
        Prometheus metrics in text format
    """
    update_system_metrics()
    update_disk_metrics("/")

    metrics_output = generate_latest()
    return Response(content=metrics_output, media_type=CONTENT_TYPE_LATEST)


@router.get("/health/live", status_code=status.HTTP_200_OK)
async def liveness() -> dict:
    """
    Liveness probe endpoint.

    Used by Kubernetes and other orchestrators to determine if the
    application is alive and should be restarted if unhealthy.

    This check should be simple and always succeed unless the
    application is completely broken.

    Returns:
        Liveness status
    """
    health_check = HealthCheck()
    result = await health_check.liveness()

    logger.info("liveness_check", status=result["status"])

    return result


@router.get("/health/ready")
async def readiness(
    db: AsyncSession = Depends(get_session),
) -> dict:
    """
    Readiness probe endpoint.

    Used by Kubernetes and other orchestrators to determine if the
    application is ready to serve traffic.

    Checks all critical dependencies:
    - Database connectivity
    - Redis connectivity
    - Disk space
    - Memory availability

    Returns:
        Readiness status with detailed component health
    """
    engine = get_engine()

    health_check = HealthCheck(
        db_engine=engine,
        redis_client=None,
    )

    result = await health_check.readiness()

    logger.info(
        "readiness_check",
        status=result.status.value,
        response_time_ms=result.overall_response_time_ms,
    )

    status_code = status.HTTP_200_OK
    if result.status == HealthStatus.UNHEALTHY:
        status_code = status.HTTP_503_SERVICE_UNAVAILABLE
    elif result.status == HealthStatus.DEGRADED:
        status_code = status.HTTP_200_OK

    return Response(
        content=result.to_dict().__str__(),
        status_code=status_code,
        media_type="application/json",
    )


@router.get("/health")
async def health(
    db: AsyncSession = Depends(get_session),
) -> dict:
    """
    General health check endpoint.

    Alias for readiness check.

    Returns:
        Health status
    """
    return await readiness(db)


@router.get("/ping")
async def ping() -> dict:
    """
    Simple ping endpoint for basic connectivity checks.

    Returns:
        Pong response
    """
    return {"message": "pong", "status": "ok"}
