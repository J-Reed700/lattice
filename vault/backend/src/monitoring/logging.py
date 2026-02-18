"""
Structured logging configuration using structlog.

Provides JSON-formatted logging with contextual information
for better observability and log aggregation.
"""

import logging
import sys
from typing import Any

import structlog
from structlog.types import EventDict, Processor


def add_app_context(logger: Any, method_name: str, event_dict: EventDict) -> EventDict:
    """
    Add application context to log events.
    """
    event_dict["app"] = "vault"
    event_dict["service"] = "backend"
    return event_dict


def drop_color_message_key(logger: Any, method_name: str, event_dict: EventDict) -> EventDict:
    """
    Remove color formatting from console logs for JSON output.
    """
    event_dict.pop("color_message", None)
    return event_dict


def censor_sensitive_data(logger: Any, method_name: str, event_dict: EventDict) -> EventDict:
    """
    Remove sensitive data from logs.

    Censors:
    - Passwords
    - API keys
    - Tokens
    - Authorization headers
    - Secrets
    """
    sensitive_keys = {
        "password",
        "token",
        "api_key",
        "secret",
        "authorization",
        "auth",
        "credentials",
        "access_token",
        "refresh_token",
        "session_id",
        "cookie",
    }

    for key in list(event_dict.keys()):
        key_lower = key.lower()
        if any(sensitive in key_lower for sensitive in sensitive_keys):
            event_dict[key] = "***REDACTED***"

    if "headers" in event_dict and isinstance(event_dict["headers"], dict):
        headers = event_dict["headers"]
        for key in list(headers.keys()):
            key_lower = key.lower()
            if any(sensitive in key_lower for sensitive in sensitive_keys):
                headers[key] = "***REDACTED***"

    return event_dict


def setup_logging(
    level: str = "INFO",
    json_logs: bool = True,
    log_file: str | None = None,
) -> None:
    """
    Configure structured logging with structlog.

    Args:
        level: Log level (DEBUG, INFO, WARNING, ERROR, CRITICAL)
        json_logs: If True, output logs in JSON format, else use console format
        log_file: Optional file path to write logs to
    """
    log_level = getattr(logging, level.upper(), logging.INFO)

    processors: list[Processor] = [
        structlog.contextvars.merge_contextvars,
        structlog.stdlib.filter_by_level,
        structlog.stdlib.add_logger_name,
        structlog.stdlib.add_log_level,
        structlog.stdlib.PositionalArgumentsFormatter(),
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.StackInfoRenderer(),
        structlog.processors.format_exc_info,
        structlog.processors.UnicodeDecoder(),
        add_app_context,
        censor_sensitive_data,
    ]

    if json_logs:
        processors.extend(
            [
                drop_color_message_key,
                structlog.processors.JSONRenderer(),
            ]
        )
    else:
        processors.append(
            structlog.dev.ConsoleRenderer(
                colors=True,
                exception_formatter=structlog.dev.RichTracebackFormatter(),
            )
        )

    structlog.configure(
        processors=processors,
        wrapper_class=structlog.stdlib.BoundLogger,
        logger_factory=structlog.stdlib.LoggerFactory(),
        cache_logger_on_first_use=True,
    )

    handlers: list[logging.Handler] = [logging.StreamHandler(sys.stdout)]

    if log_file:
        file_handler = logging.FileHandler(log_file)
        file_handler.setLevel(log_level)
        handlers.append(file_handler)

    logging.basicConfig(
        format="%(message)s",
        level=log_level,
        handlers=handlers,
        force=True,
    )

    for logger_name in ["uvicorn", "uvicorn.access", "uvicorn.error"]:
        uvicorn_logger = logging.getLogger(logger_name)
        uvicorn_logger.handlers = []
        uvicorn_logger.propagate = True


def get_logger(name: str) -> structlog.stdlib.BoundLogger:
    """
    Get a structured logger instance.

    Args:
        name: Logger name (usually __name__)

    Returns:
        Configured structlog logger
    """
    return structlog.get_logger(name)


class LoggerAdapter:
    """
    Adapter to provide context-specific logging.

    Example:
        logger = LoggerAdapter(get_logger(__name__))
        with logger.bind(user_id=123):
            logger.info("user_action", action="login")
    """

    def __init__(self, logger: structlog.stdlib.BoundLogger):
        self._logger = logger

    def bind(self, **kwargs: Any) -> "LoggerAdapter":
        """
        Bind context to logger.
        """
        return LoggerAdapter(self._logger.bind(**kwargs))

    def debug(self, event: str, **kwargs: Any) -> None:
        self._logger.debug(event, **kwargs)

    def info(self, event: str, **kwargs: Any) -> None:
        self._logger.info(event, **kwargs)

    def warning(self, event: str, **kwargs: Any) -> None:
        self._logger.warning(event, **kwargs)

    def error(self, event: str, **kwargs: Any) -> None:
        self._logger.error(event, **kwargs)

    def critical(self, event: str, **kwargs: Any) -> None:
        self._logger.critical(event, **kwargs)

    def exception(self, event: str, **kwargs: Any) -> None:
        self._logger.exception(event, **kwargs)


def log_function_call(logger: structlog.stdlib.BoundLogger):
    """
    Decorator to log function calls with arguments and results.

    Example:
        @log_function_call(logger)
        async def process_file(file_id: str):
            ...
    """

    def decorator(func):
        async def async_wrapper(*args, **kwargs):
            logger.debug(
                "function_called",
                function=func.__name__,
                args=args,
                kwargs=kwargs,
            )
            try:
                result = await func(*args, **kwargs)
                logger.debug(
                    "function_completed",
                    function=func.__name__,
                )
                return result
            except Exception as e:
                logger.error(
                    "function_failed",
                    function=func.__name__,
                    error=str(e),
                    error_type=type(e).__name__,
                )
                raise

        def sync_wrapper(*args, **kwargs):
            logger.debug(
                "function_called",
                function=func.__name__,
                args=args,
                kwargs=kwargs,
            )
            try:
                result = func(*args, **kwargs)
                logger.debug(
                    "function_completed",
                    function=func.__name__,
                )
                return result
            except Exception as e:
                logger.error(
                    "function_failed",
                    function=func.__name__,
                    error=str(e),
                    error_type=type(e).__name__,
                )
                raise

        import asyncio

        if asyncio.iscoroutinefunction(func):
            return async_wrapper
        return sync_wrapper

    return decorator


def log_business_event(
    logger: structlog.stdlib.BoundLogger,
    event: str,
    **context: Any,
) -> None:
    """
    Log a business event with context.

    Business events are important application events that should be tracked
    for analytics and monitoring.

    Args:
        logger: Logger instance
        event: Event name (e.g., "file_uploaded", "search_performed")
        context: Event context
    """
    logger.info(
        event,
        event_type="business",
        **context,
    )


def log_security_event(
    logger: structlog.stdlib.BoundLogger,
    event: str,
    severity: str = "warning",
    **context: Any,
) -> None:
    """
    Log a security event.

    Args:
        logger: Logger instance
        event: Event name (e.g., "unauthorized_access", "failed_login")
        severity: Event severity (info, warning, error, critical)
        context: Event context
    """
    log_func = getattr(logger, severity, logger.warning)
    log_func(
        event,
        event_type="security",
        **context,
    )


def log_performance_metric(
    logger: structlog.stdlib.BoundLogger,
    metric: str,
    value: float,
    unit: str = "seconds",
    **context: Any,
) -> None:
    """
    Log a performance metric.

    Args:
        logger: Logger instance
        metric: Metric name
        value: Metric value
        unit: Metric unit
        context: Additional context
    """
    logger.info(
        "performance_metric",
        metric=metric,
        value=value,
        unit=unit,
        **context,
    )
