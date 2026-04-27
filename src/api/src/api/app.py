from __future__ import annotations

from contextlib import asynccontextmanager
import logging
import time

from fastapi import FastAPI, Request
from fastapi.exceptions import RequestValidationError
from fastapi.middleware.cors import CORSMiddleware
from opentelemetry import trace
from slowapi import _rate_limit_exceeded_handler
from slowapi.errors import RateLimitExceeded
from sqlalchemy import select

from src.api.errors import (
    APIError,
    DatabaseError,
    EmbeddingError,
    NotFoundError,
    SearchError,
    StorageError,
    ValidationError,
    api_error_handler,
    internal_error_handler,
)
from src.api.errors import (
    validation_error_handler as custom_validation_handler,
)
from src.api.v1 import router as api_v1_router
from src.auth.token_blacklist import token_blacklist
from src.config import get_settings
from src.db import close_engine, get_session_factory, init_db
from src.middleware.csrf import CSRFMiddleware
from src.middleware.rate_limit import limiter
from src.models import WatchFolder
from src.modules.content_extractor.extractors.ocr_service import cleanup_ocr_service
from src.observability.tracing import setup_otel
from src.services.watch import WatchService

logger = logging.getLogger(__name__)

tracer = None
meter = None


@asynccontextmanager
async def lifespan(app: FastAPI):
    global tracer, meter

    logger.info("Starting Vault API...")
    settings = get_settings()

    tracer, meter = setup_otel(app)
    logger.info("OpenTelemetry initialized")

    try:
        await init_db()
        logger.info("Database initialized successfully")
    except Exception:
        logger.exception("Failed to initialize database", extra={"app": "Vault API"})
        raise

    try:
        from src.events import EventBus
        from src.events.subscribers import (
            setup_audit_logging,
            setup_cache_invalidation,
            setup_metrics,
        )

        event_bus = EventBus()
        setup_audit_logging(event_bus)
        setup_cache_invalidation(event_bus)
        setup_metrics(event_bus)

        app.state.event_bus = event_bus
        logger.info("Event bus initialized with subscribers")
    except Exception:
        logger.exception("Failed to initialize event bus", extra={"app": "Vault API"})
        raise

    try:
        await token_blacklist.start()
        logger.info("Token blacklist initialized successfully")
    except Exception:
        logger.exception("Failed to initialize token blacklist", extra={"app": "Vault API"})
        raise

    try:
        from src.auth.mfa_rate_limiter import mfa_rate_limiter

        await mfa_rate_limiter.start()
        logger.info("MFA rate limiter initialized successfully")
    except Exception:
        logger.exception("Failed to initialize MFA rate limiter", extra={"app": "Vault API"})
        raise

    watch_service = None
    if settings.file_watcher_enabled:
        try:
            watch_service = WatchService()
            watch_service.set_session_factory(get_session_factory())
            app.state.watch_service = watch_service

            async with get_session_factory()() as session:
                result = await session.execute(
                    select(WatchFolder).where(WatchFolder.active == True)
                )
                active_folders = result.scalars().all()

                for folder in active_folders:
                    try:
                        await watch_service.start_watching(
                            watch_folder_id=folder.id,
                            path=folder.path,
                            recursive=folder.recursive,
                            db_session=session,
                        )
                        logger.info(f"Started watching: {folder.path}")
                    except Exception as e:
                        logger.error(f"Failed to start watching {folder.path}: {e}", exc_info=True)

            logger.info(f"File watcher initialized with {len(active_folders)} active folders")
        except Exception:
            logger.error("Failed to initialize file watcher", exc_info=True)

    yield

    logger.info("Shutting down Vault API...")

    if watch_service:
        try:
            await watch_service.stop_all()
            logger.info("File watcher stopped")
        except Exception:
            logger.error("Error stopping file watcher", exc_info=True)

    try:
        cleanup_ocr_service()
        logger.info("VLM resources cleaned up")
    except Exception:
        logger.error("Error cleaning up OCR service", exc_info=True)

    try:
        await token_blacklist.stop()
        logger.info("Token blacklist stopped")
    except Exception:
        logger.error("Error stopping token blacklist", exc_info=True)

    try:
        from src.auth.mfa_rate_limiter import mfa_rate_limiter

        await mfa_rate_limiter.stop()
        logger.info("MFA rate limiter stopped")
    except Exception:
        logger.error("Error stopping MFA rate limiter", exc_info=True)

    try:
        await close_engine()
        logger.info("Database connections closed")
    except Exception:
        logger.error("Error closing database", exc_info=True, extra={"app": "Vault API"})


def create_app() -> FastAPI:
    settings = get_settings()

    app = FastAPI(
        title="Vault API",
        description="Personal knowledge vault with semantic search",
        version=settings.app_version,
        docs_url="/api/docs" if settings.enable_docs else None,
        redoc_url="/api/redoc" if settings.enable_docs else None,
        openapi_url="/api/openapi.json" if settings.enable_docs else None,
        lifespan=lifespan,
    )

    app.state.limiter = limiter
    app.add_exception_handler(RateLimitExceeded, _rate_limit_exceeded_handler)

    configure_middleware(app, settings)

    configure_exception_handlers(app)

    configure_routes(app, settings)

    return app


def configure_middleware(app: FastAPI, settings):
    if settings.csrf_enabled:
        app.add_middleware(
            CSRFMiddleware,
            cookie_secure=settings.csrf_cookie_secure,
            cookie_samesite=settings.csrf_cookie_samesite,
            cookie_httponly=settings.csrf_cookie_httponly,
            cookie_domain=settings.csrf_cookie_domain,
        )
        logger.info("CSRF protection enabled")

    # CORS Configuration
    # allow_credentials=True is required for cookie-based authentication (CSRF tokens, sessions)
    # SECURITY: Ensure cors_origins does NOT include wildcards (*) when credentials are enabled
    # This is enforced by settings validation to prevent credential exposure
    app.add_middleware(
        CORSMiddleware,
        allow_origins=settings.cors_origins,
        allow_credentials=settings.cors_allow_credentials,
        allow_methods=settings.cors_allow_methods,
        allow_headers=settings.cors_allow_headers,
        max_age=3600,
    )

    @app.middleware("http")
    async def add_security_headers(request: Request, call_next):
        """Add security headers to all responses."""
        response = await call_next(request)
        response.headers["X-Content-Type-Options"] = "nosniff"
        response.headers["X-Frame-Options"] = "DENY"
        response.headers["X-XSS-Protection"] = "1; mode=block"
        response.headers["Strict-Transport-Security"] = "max-age=31536000; includeSubDomains"
        response.headers["Content-Security-Policy"] = (
            "default-src 'self'; "
            "script-src 'self'; "
            "style-src 'self' 'unsafe-inline'; "
            "img-src 'self' data:;"
        )
        response.headers["Referrer-Policy"] = "strict-origin-when-cross-origin"
        return response

    @app.middleware("http")
    async def add_trace_context(request: Request, call_next):
        span = trace.get_current_span()

        if span.is_recording():
            span.set_attribute(
                "http.client_ip", request.client.host if request.client else "unknown"
            )
            span.set_attribute("http.user_agent", request.headers.get("user-agent", ""))
            span.set_attribute("http.request_id", request.headers.get("x-request-id", ""))

        response = await call_next(request)

        if span.is_recording():
            span.set_attribute("http.status_code", response.status_code)

        return response

    @app.middleware("http")
    async def log_requests(request: Request, call_next):
        start_time = time.time()

        logger.info(f"Request: {request.method} {request.url.path}")

        response = await call_next(request)

        duration = time.time() - start_time
        logger.info(
            f"Response: {request.method} {request.url.path} "
            f"- Status: {response.status_code} - Duration: {duration:.3f}s"
        )

        response.headers["X-Process-Time"] = str(duration)
        return response


def configure_exception_handlers(app: FastAPI):
    app.add_exception_handler(ValidationError, api_error_handler)
    app.add_exception_handler(NotFoundError, api_error_handler)
    app.add_exception_handler(StorageError, api_error_handler)
    app.add_exception_handler(SearchError, api_error_handler)
    app.add_exception_handler(EmbeddingError, api_error_handler)
    app.add_exception_handler(DatabaseError, api_error_handler)
    app.add_exception_handler(APIError, api_error_handler)
    app.add_exception_handler(RequestValidationError, custom_validation_handler)
    app.add_exception_handler(Exception, internal_error_handler)


def configure_routes(app: FastAPI, settings):
    app.include_router(api_v1_router)

    @app.get("/")
    async def root():
        return {
            "name": "Vault API",
            "version": settings.app_version,
            "description": "Personal knowledge vault with semantic search",
            "docs": "/api/docs",
            "health": "/api/v1/health",
        }


app = create_app()
