"""
Middleware for metrics collection and structured logging.
"""

from collections.abc import Callable
import time

from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
from starlette.responses import Response
from starlette.types import ASGIApp
import structlog

from src.monitoring.metrics import (
    http_request_duration_seconds,
    http_request_size_bytes,
    http_requests_in_progress,
    http_requests_total,
    http_response_size_bytes,
)

logger = structlog.get_logger(__name__)


class MetricsMiddleware(BaseHTTPMiddleware):
    """
    Middleware to collect Prometheus metrics for HTTP requests.

    Tracks:
    - Request count by method, endpoint, and status code
    - Request duration with percentiles
    - Active requests (in-progress gauge)
    - Request and response sizes
    """

    def __init__(self, app: ASGIApp):
        super().__init__(app)

    async def dispatch(self, request: Request, call_next: Callable) -> Response:
        method = request.method
        path = self._get_path_template(request)

        http_requests_in_progress.labels(method=method, endpoint=path).inc()

        start_time = time.time()

        try:
            request_size = int(request.headers.get("content-length", 0))
            if request_size > 0:
                http_request_size_bytes.labels(method=method, endpoint=path).observe(request_size)
        except (ValueError, TypeError):
            pass

        try:
            response = await call_next(request)
        except Exception:
            duration = time.time() - start_time
            http_request_duration_seconds.labels(method=method, endpoint=path).observe(duration)
            http_requests_total.labels(method=method, endpoint=path, status_code="500").inc()
            http_requests_in_progress.labels(method=method, endpoint=path).dec()
            raise

        duration = time.time() - start_time

        http_request_duration_seconds.labels(method=method, endpoint=path).observe(duration)

        http_requests_total.labels(
            method=method, endpoint=path, status_code=str(response.status_code)
        ).inc()

        try:
            response_size = int(response.headers.get("content-length", 0))
            if response_size > 0:
                http_response_size_bytes.labels(method=method, endpoint=path).observe(response_size)
        except (ValueError, TypeError):
            pass

        http_requests_in_progress.labels(method=method, endpoint=path).dec()

        response.headers["X-Process-Time"] = str(duration)

        return response

    def _get_path_template(self, request: Request) -> str:
        """
        Get the path template for the request to avoid high cardinality.

        For example, /api/v1/files/123 becomes /api/v1/files/{file_id}
        """
        if hasattr(request, "scope") and "route" in request.scope:
            route = request.scope["route"]
            if hasattr(route, "path"):
                return route.path

        path = request.url.path

        if path.startswith("/api/v1/"):
            parts = path.split("/")
            if len(parts) > 4:
                if parts[3] in {"files", "documents", "tags", "clusters", "export"}:
                    if len(parts) > 4 and parts[4]:
                        parts[4] = "{id}"
            path = "/".join(parts)

        return path


class StructuredLoggingMiddleware(BaseHTTPMiddleware):
    """
    Middleware for structured logging of HTTP requests and responses.

    Logs all requests with contextual information in JSON format.
    """

    def __init__(self, app: ASGIApp):
        super().__init__(app)

    async def dispatch(self, request: Request, call_next: Callable) -> Response:
        start_time = time.time()
        request_id = request.headers.get("X-Request-ID", "")

        log = logger.bind(
            request_id=request_id,
            method=request.method,
            path=request.url.path,
            query_params=str(request.query_params),
            client_host=request.client.host if request.client else None,
            user_agent=request.headers.get("user-agent"),
        )

        log.info("request_started")

        try:
            response = await call_next(request)
            duration = time.time() - start_time

            log = log.bind(
                status_code=response.status_code,
                duration=f"{duration:.3f}",
            )

            if response.status_code >= 500:
                log.error("request_completed_with_error")
            elif response.status_code >= 400:
                log.warning("request_completed_with_client_error")
            else:
                log.info("request_completed")

            return response

        except Exception as e:
            duration = time.time() - start_time

            log.bind(
                duration=f"{duration:.3f}",
                error=str(e),
                error_type=type(e).__name__,
            ).exception("request_failed")

            raise


class SecurityLoggingMiddleware(BaseHTTPMiddleware):
    """
    Middleware for security event logging.

    Logs security-relevant events such as:
    - Authentication failures
    - Unauthorized access attempts
    - Suspicious request patterns
    """

    def __init__(self, app: ASGIApp):
        super().__init__(app)

    async def dispatch(self, request: Request, call_next: Callable) -> Response:
        response = await call_next(request)

        if response.status_code == 401:
            logger.warning(
                "authentication_failed",
                method=request.method,
                path=request.url.path,
                client_host=request.client.host if request.client else None,
                user_agent=request.headers.get("user-agent"),
            )
        elif response.status_code == 403:
            logger.warning(
                "authorization_failed",
                method=request.method,
                path=request.url.path,
                client_host=request.client.host if request.client else None,
                user_agent=request.headers.get("user-agent"),
            )

        return response


class RateLimitLoggingMiddleware(BaseHTTPMiddleware):
    """
    Middleware to log rate limit events.
    """

    def __init__(self, app: ASGIApp):
        super().__init__(app)

    async def dispatch(self, request: Request, call_next: Callable) -> Response:
        response = await call_next(request)

        if response.status_code == 429:
            logger.warning(
                "rate_limit_exceeded",
                method=request.method,
                path=request.url.path,
                client_host=request.client.host if request.client else None,
                user_agent=request.headers.get("user-agent"),
                retry_after=response.headers.get("Retry-After"),
            )

        return response
