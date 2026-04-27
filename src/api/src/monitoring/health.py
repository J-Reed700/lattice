"""
Health check functionality for readiness and liveness probes.
"""

import asyncio
from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import Enum
from typing import Any

import psutil
from redis.asyncio import Redis
from sqlalchemy import text
from sqlalchemy.ext.asyncio import AsyncEngine
import structlog

logger = structlog.get_logger(__name__)


class HealthStatus(str, Enum):
    """Health status enumeration."""

    HEALTHY = "healthy"
    DEGRADED = "degraded"
    UNHEALTHY = "unhealthy"


@dataclass
class ComponentHealth:
    """Health status of a component."""

    name: str
    status: HealthStatus
    message: str | None = None
    response_time_ms: float | None = None
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass
class HealthCheckResult:
    """Overall health check result."""

    status: HealthStatus
    timestamp: datetime
    components: dict[str, ComponentHealth]
    overall_response_time_ms: float

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for API response."""
        return {
            "status": self.status.value,
            "timestamp": self.timestamp.isoformat(),
            "response_time_ms": self.overall_response_time_ms,
            "components": {
                name: {
                    "status": comp.status.value,
                    "message": comp.message,
                    "response_time_ms": comp.response_time_ms,
                    "metadata": comp.metadata,
                }
                for name, comp in self.components.items()
            },
        }


class HealthCheck:
    """
    Health check manager for monitoring system health.

    Provides liveness and readiness probes for orchestration platforms.
    """

    def __init__(
        self,
        db_engine: AsyncEngine | None = None,
        redis_client: Redis | None = None,
    ):
        self.db_engine = db_engine
        self.redis_client = redis_client
        self.custom_checks: dict[str, Callable[[], Awaitable[ComponentHealth]]] = {}

    def register_check(
        self,
        name: str,
        check_func: Callable[[], Awaitable[ComponentHealth]],
    ) -> None:
        """
        Register a custom health check.

        Args:
            name: Check name
            check_func: Async function that returns ComponentHealth
        """
        self.custom_checks[name] = check_func
        logger.info("health_check_registered", check_name=name)

    async def liveness(self) -> dict[str, Any]:
        """
        Liveness probe - is the application running?

        This should be a simple check that always succeeds unless
        the application is completely broken.

        Returns:
            Liveness status
        """
        return {
            "status": "alive",
            "timestamp": datetime.now(UTC).isoformat(),
        }

    async def readiness(self) -> HealthCheckResult:
        """
        Readiness probe - is the application ready to serve traffic?

        Checks all critical dependencies and returns detailed status.

        Returns:
            Complete health check result
        """
        start_time = asyncio.get_event_loop().time()
        components: dict[str, ComponentHealth] = {}

        checks = [
            ("database", self._check_database()),
            ("redis", self._check_redis()),
            ("disk", self._check_disk()),
            ("memory", self._check_memory()),
        ]

        for name, check_func in self.custom_checks.items():
            checks.append((name, check_func()))

        results = await asyncio.gather(*[check for _, check in checks], return_exceptions=True)

        for (name, _), result in zip(checks, results, strict=False):
            if isinstance(result, Exception):
                logger.error(
                    "health_check_failed",
                    component=name,
                    error=str(result),
                )
                components[name] = ComponentHealth(
                    name=name,
                    status=HealthStatus.UNHEALTHY,
                    message=f"Check failed: {result!s}",
                )
            else:
                components[name] = result

        overall_status = self._determine_overall_status(components)

        end_time = asyncio.get_event_loop().time()
        response_time_ms = (end_time - start_time) * 1000

        result = HealthCheckResult(
            status=overall_status,
            timestamp=datetime.now(UTC),
            components=components,
            overall_response_time_ms=response_time_ms,
        )

        logger.info(
            "health_check_completed",
            status=overall_status.value,
            response_time_ms=response_time_ms,
        )

        return result

    async def _check_database(self) -> ComponentHealth:
        """Check database connectivity and health."""
        if not self.db_engine:
            return ComponentHealth(
                name="database",
                status=HealthStatus.DEGRADED,
                message="Database engine not configured",
            )

        start_time = asyncio.get_event_loop().time()

        try:
            async with self.db_engine.connect() as conn:
                await conn.execute(text("SELECT 1"))

            end_time = asyncio.get_event_loop().time()
            response_time_ms = (end_time - start_time) * 1000

            pool = self.db_engine.pool
            pool_size = pool.size()
            checked_out = pool.checkedout()

            return ComponentHealth(
                name="database",
                status=HealthStatus.HEALTHY,
                message="Database connection successful",
                response_time_ms=response_time_ms,
                metadata={
                    "pool_size": pool_size,
                    "checked_out": checked_out,
                    "available": pool_size - checked_out,
                },
            )
        except Exception as e:
            end_time = asyncio.get_event_loop().time()
            response_time_ms = (end_time - start_time) * 1000

            logger.error(
                "database_health_check_failed",
                error=str(e),
                error_type=type(e).__name__,
            )

            return ComponentHealth(
                name="database",
                status=HealthStatus.UNHEALTHY,
                message=f"Database connection failed: {e!s}",
                response_time_ms=response_time_ms,
            )

    async def _check_redis(self) -> ComponentHealth:
        """Check Redis connectivity and health."""
        if not self.redis_client:
            return ComponentHealth(
                name="redis",
                status=HealthStatus.DEGRADED,
                message="Redis client not configured",
            )

        start_time = asyncio.get_event_loop().time()

        try:
            await self.redis_client.ping()

            end_time = asyncio.get_event_loop().time()
            response_time_ms = (end_time - start_time) * 1000

            info = await self.redis_client.info()

            return ComponentHealth(
                name="redis",
                status=HealthStatus.HEALTHY,
                message="Redis connection successful",
                response_time_ms=response_time_ms,
                metadata={
                    "connected_clients": info.get("connected_clients"),
                    "used_memory_human": info.get("used_memory_human"),
                    "uptime_seconds": info.get("uptime_in_seconds"),
                },
            )
        except Exception as e:
            end_time = asyncio.get_event_loop().time()
            response_time_ms = (end_time - start_time) * 1000

            logger.error(
                "redis_health_check_failed",
                error=str(e),
                error_type=type(e).__name__,
            )

            return ComponentHealth(
                name="redis",
                status=HealthStatus.UNHEALTHY,
                message=f"Redis connection failed: {e!s}",
                response_time_ms=response_time_ms,
            )

    async def _check_disk(self) -> ComponentHealth:
        """Check disk space availability."""
        try:
            disk = psutil.disk_usage("/")
            usage_percent = disk.percent

            if usage_percent >= 95:
                status = HealthStatus.UNHEALTHY
                message = f"Disk critically full: {usage_percent:.1f}%"
            elif usage_percent >= 85:
                status = HealthStatus.DEGRADED
                message = f"Disk space low: {usage_percent:.1f}%"
            else:
                status = HealthStatus.HEALTHY
                message = f"Disk space available: {usage_percent:.1f}% used"

            return ComponentHealth(
                name="disk",
                status=status,
                message=message,
                metadata={
                    "total_gb": disk.total / (1024**3),
                    "used_gb": disk.used / (1024**3),
                    "free_gb": disk.free / (1024**3),
                    "percent": usage_percent,
                },
            )
        except Exception as e:
            logger.error(
                "disk_health_check_failed",
                error=str(e),
            )
            return ComponentHealth(
                name="disk",
                status=HealthStatus.UNHEALTHY,
                message=f"Disk check failed: {e!s}",
            )

    async def _check_memory(self) -> ComponentHealth:
        """Check memory availability."""
        try:
            mem = psutil.virtual_memory()
            usage_percent = mem.percent

            if usage_percent >= 95:
                status = HealthStatus.UNHEALTHY
                message = f"Memory critically high: {usage_percent:.1f}%"
            elif usage_percent >= 85:
                status = HealthStatus.DEGRADED
                message = f"Memory usage high: {usage_percent:.1f}%"
            else:
                status = HealthStatus.HEALTHY
                message = f"Memory available: {usage_percent:.1f}% used"

            return ComponentHealth(
                name="memory",
                status=status,
                message=message,
                metadata={
                    "total_gb": mem.total / (1024**3),
                    "used_gb": mem.used / (1024**3),
                    "available_gb": mem.available / (1024**3),
                    "percent": usage_percent,
                },
            )
        except Exception as e:
            logger.error(
                "memory_health_check_failed",
                error=str(e),
            )
            return ComponentHealth(
                name="memory",
                status=HealthStatus.UNHEALTHY,
                message=f"Memory check failed: {e!s}",
            )

    def _determine_overall_status(self, components: dict[str, ComponentHealth]) -> HealthStatus:
        """
        Determine overall health status from component statuses.

        Rules:
        - If any critical component is UNHEALTHY, overall is UNHEALTHY
        - If any component is DEGRADED, overall is DEGRADED
        - Otherwise, overall is HEALTHY

        Args:
            components: Component health statuses

        Returns:
            Overall health status
        """
        if not components:
            return HealthStatus.UNHEALTHY

        critical_components = {"database"}
        has_unhealthy = False
        has_degraded = False

        for name, component in components.items():
            if component.status == HealthStatus.UNHEALTHY:
                if name in critical_components:
                    return HealthStatus.UNHEALTHY
                has_unhealthy = True
            elif component.status == HealthStatus.DEGRADED:
                has_degraded = True

        if has_unhealthy:
            return HealthStatus.UNHEALTHY

        if has_degraded:
            return HealthStatus.DEGRADED

        return HealthStatus.HEALTHY
