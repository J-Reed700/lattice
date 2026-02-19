"""
Sentry integration for error tracking and performance monitoring.
"""

from typing import Any

import sentry_sdk
from sentry_sdk.integrations.fastapi import FastApiIntegration
from sentry_sdk.integrations.logging import LoggingIntegration
from sentry_sdk.integrations.redis import RedisIntegration
from sentry_sdk.integrations.sqlalchemy import SqlalchemyIntegration
import structlog

logger = structlog.get_logger(__name__)


def sanitize_sentry_event(event: dict[str, Any], hint: dict[str, Any]) -> dict[str, Any] | None:
    """
    Sanitize Sentry event before sending to remove PII and sensitive data.

    Args:
        event: Sentry event dictionary
        hint: Additional context

    Returns:
        Sanitized event or None to drop the event
    """
    sensitive_keys = {
        "password",
        "token",
        "api_key",
        "secret",
        "authorization",
        "cookie",
        "session",
        "credentials",
        "access_token",
        "refresh_token",
    }

    def sanitize_dict(d: dict[str, Any]) -> None:
        for key in list(d.keys()):
            if any(sensitive in key.lower() for sensitive in sensitive_keys):
                d[key] = "[REDACTED]"
            elif isinstance(d[key], dict):
                sanitize_dict(d[key])
            elif isinstance(d[key], list):
                for item in d[key]:
                    if isinstance(item, dict):
                        sanitize_dict(item)

    if "request" in event:
        request = event["request"]

        if "headers" in request:
            sanitize_dict(request["headers"])

        if "cookies" in request:
            request["cookies"] = {}

        if "data" in request and isinstance(request["data"], dict):
            sanitize_dict(request["data"])

    if "extra" in event:
        sanitize_dict(event["extra"])

    if "contexts" in event:
        sanitize_dict(event["contexts"])

    return event


def init_sentry(
    dsn: str,
    environment: str,
    release: str | None = None,
    traces_sample_rate: float = 0.1,
    profiles_sample_rate: float = 0.1,
    enable_tracing: bool = True,
) -> None:
    """
    Initialize Sentry SDK with integrations.

    Args:
        dsn: Sentry DSN (Data Source Name)
        environment: Environment name (development, staging, production)
        release: Release version
        traces_sample_rate: Percentage of transactions to trace (0.0 to 1.0)
        profiles_sample_rate: Percentage of transactions to profile (0.0 to 1.0)
        enable_tracing: Whether to enable performance tracing
    """
    if not dsn:
        logger.warning("sentry_not_initialized", reason="No DSN provided")
        return

    integrations = [
        FastApiIntegration(transaction_style="endpoint"),
        SqlalchemyIntegration(),
        RedisIntegration(),
        LoggingIntegration(
            level=None,
            event_level=None,
        ),
    ]

    try:
        sentry_sdk.init(
            dsn=dsn,
            integrations=integrations,
            environment=environment,
            release=release,
            traces_sample_rate=traces_sample_rate if enable_tracing else 0.0,
            profiles_sample_rate=profiles_sample_rate if enable_tracing else 0.0,
            before_send=sanitize_sentry_event,
            send_default_pii=False,
            attach_stacktrace=True,
            max_breadcrumbs=50,
            debug=False,
        )

        logger.info(
            "sentry_initialized",
            environment=environment,
            release=release,
            traces_sample_rate=traces_sample_rate,
        )
    except Exception as e:
        logger.error(
            "sentry_initialization_failed",
            error=str(e),
            error_type=type(e).__name__,
        )


def capture_exception(
    exception: Exception,
    context: dict[str, Any] | None = None,
    level: str = "error",
    tags: dict[str, str] | None = None,
) -> str | None:
    """
    Capture an exception and send to Sentry.

    Args:
        exception: Exception to capture
        context: Additional context
        level: Error level (error, warning, info, fatal)
        tags: Tags to attach to the event

    Returns:
        Event ID if sent, None otherwise
    """
    try:
        with sentry_sdk.push_scope() as scope:
            scope.level = level

            if context:
                for key, value in context.items():
                    scope.set_context(key, value)

            if tags:
                for key, value in tags.items():
                    scope.set_tag(key, value)

            event_id = sentry_sdk.capture_exception(exception)

            logger.debug(
                "exception_captured_by_sentry",
                event_id=event_id,
                exception_type=type(exception).__name__,
            )

            return event_id
    except Exception as e:
        logger.error(
            "failed_to_capture_exception_in_sentry",
            error=str(e),
        )
        return None


def capture_message(
    message: str,
    level: str = "info",
    context: dict[str, Any] | None = None,
    tags: dict[str, str] | None = None,
) -> str | None:
    """
    Capture a message and send to Sentry.

    Args:
        message: Message to capture
        level: Message level (info, warning, error, fatal)
        context: Additional context
        tags: Tags to attach to the event

    Returns:
        Event ID if sent, None otherwise
    """
    try:
        with sentry_sdk.push_scope() as scope:
            scope.level = level

            if context:
                for key, value in context.items():
                    scope.set_context(key, value)

            if tags:
                for key, value in tags.items():
                    scope.set_tag(key, value)

            event_id = sentry_sdk.capture_message(message, level=level)

            logger.debug(
                "message_captured_by_sentry",
                event_id=event_id,
                message=message,
            )

            return event_id
    except Exception as e:
        logger.error(
            "failed_to_capture_message_in_sentry",
            error=str(e),
        )
        return None


def add_breadcrumb(
    message: str,
    category: str = "default",
    level: str = "info",
    data: dict[str, Any] | None = None,
) -> None:
    """
    Add a breadcrumb for Sentry error context.

    Breadcrumbs provide a trail of events leading up to an error.

    Args:
        message: Breadcrumb message
        category: Breadcrumb category
        level: Breadcrumb level
        data: Additional data
    """
    try:
        sentry_sdk.add_breadcrumb(
            message=message,
            category=category,
            level=level,
            data=data or {},
        )
    except Exception:
        pass


def set_user(
    user_id: str | None = None,
    email: str | None = None,
    username: str | None = None,
    **kwargs: Any,
) -> None:
    """
    Set user context for Sentry events.

    Args:
        user_id: User ID
        email: User email (will be redacted in sanitize_sentry_event)
        username: Username
        kwargs: Additional user attributes
    """
    try:
        user_data = {}

        if user_id:
            user_data["id"] = user_id
        if email:
            user_data["email"] = email
        if username:
            user_data["username"] = username

        user_data.update(kwargs)

        sentry_sdk.set_user(user_data)
    except Exception:
        pass


def set_tag(key: str, value: str) -> None:
    """
    Set a tag for Sentry events.

    Tags are searchable in Sentry UI.

    Args:
        key: Tag key
        value: Tag value
    """
    try:
        sentry_sdk.set_tag(key, value)
    except Exception:
        pass


def set_context(name: str, context: dict[str, Any]) -> None:
    """
    Set context for Sentry events.

    Args:
        name: Context name
        context: Context dictionary
    """
    try:
        sentry_sdk.set_context(name, context)
    except Exception:
        pass


def start_transaction(
    name: str,
    op: str = "http.server",
    **kwargs: Any,
) -> Any:
    """
    Start a Sentry performance transaction.

    Args:
        name: Transaction name
        op: Operation type
        kwargs: Additional transaction attributes

    Returns:
        Transaction object
    """
    try:
        return sentry_sdk.start_transaction(
            name=name,
            op=op,
            **kwargs,
        )
    except Exception:
        return None


def start_span(
    op: str,
    description: str | None = None,
    **kwargs: Any,
) -> Any:
    """
    Start a Sentry performance span.

    Args:
        op: Operation type
        description: Span description
        kwargs: Additional span attributes

    Returns:
        Span object
    """
    try:
        return sentry_sdk.start_span(
            op=op,
            description=description,
            **kwargs,
        )
    except Exception:
        return None
