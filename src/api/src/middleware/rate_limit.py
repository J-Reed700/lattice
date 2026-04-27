"""
Rate Limiting Module

Provides configurable rate limiting for API endpoints using Redis backend.
Prevents brute force attacks, credential stuffing, and abuse.

Usage:
    from src.middleware.rate_limit import limiter, check_login_rate_limit

    @router.post("/login")
    @limiter.limit("5/minute")
    async def login(...):
        await check_login_rate_limit(request, username)
"""

from collections.abc import Callable
from functools import wraps

from fastapi import HTTPException, Request, status
import redis.asyncio as redis
from slowapi import Limiter
from slowapi.util import get_remote_address
import structlog

logger = structlog.get_logger(__name__)

redis_client: redis.Redis | None = None


async def get_redis() -> redis.Redis:
    """Get Redis client for rate limiting"""
    global redis_client
    if redis_client is None:
        from src.config.settings import get_settings

        settings = get_settings()
        redis_client = await redis.from_url(
            settings.redis_url, encoding="utf-8", decode_responses=True
        )
    return redis_client


def get_rate_limit_key(request: Request, suffix: str = "") -> str:
    """
    Generate rate limit key for request.

    Combines IP address with optional suffix (like username) for granular limits.

    Args:
        request: FastAPI request
        suffix: Optional suffix for key (e.g., username)

    Returns:
        Rate limit key
    """
    ip = get_remote_address(request)
    if suffix:
        return f"{ip}:{suffix}"
    return ip


limiter = Limiter(
    key_func=get_remote_address,
    default_limits=["1000/hour"],
)


class RateLimitExceededError(HTTPException):
    """Custom exception for rate limit exceeded"""

    def __init__(self, retry_after: int, limit: int = 0, remaining: int = 0):
        headers = {
            "Retry-After": str(retry_after),
        }
        if limit > 0:
            headers["X-RateLimit-Limit"] = str(limit)
            headers["X-RateLimit-Remaining"] = "0"
            headers["X-RateLimit-Reset"] = str(retry_after)

        super().__init__(
            status_code=status.HTTP_429_TOO_MANY_REQUESTS,
            detail=f"Rate limit exceeded. Try again in {retry_after} seconds.",
            headers=headers,
        )


async def check_rate_limit_by_key(
    key: str, limit: int, window_seconds: int
) -> tuple[bool, int, int, int]:
    """
    Check rate limit for a specific key.

    Args:
        key: Rate limit key (e.g., "login:username" or "upload:user_id")
        limit: Maximum requests allowed
        window_seconds: Time window in seconds

    Returns:
        Tuple of (allowed, retry_after_seconds, remaining, limit_value)
    """
    try:
        redis_conn = await get_redis()

        current = await redis_conn.incr(key)

        if current == 1:
            await redis_conn.expire(key, window_seconds)

        if current > limit:
            ttl = await redis_conn.ttl(key)

            logger.warning(
                "rate_limit_exceeded",
                event_type="security",
                key=key,
                current=current,
                limit=limit,
                window_seconds=window_seconds,
                retry_after=retry_after,
            )

            retry_after = ttl if ttl > 0 else window_seconds
            return False, retry_after, 0, limit

        remaining = max(0, limit - current)
        return True, 0, remaining, limit
    except Exception as e:
        logger.error("rate_limit_check_failed", error=str(e), exc_info=True)
        return True, 0, limit, limit


def rate_limit_by_username(limit: int, window_seconds: int):
    """
    Rate limit decorator that limits by username instead of IP.

    Use for login attempts to prevent brute force on specific accounts.

    Usage:
        @rate_limit_by_username(limit=10, window_seconds=3600)
        async def login(username: str, ...):
            pass

    Args:
        limit: Maximum requests allowed
        window_seconds: Time window in seconds
    """

    def decorator(func: Callable):
        @wraps(func)
        async def wrapper(*args, **kwargs):
            username = kwargs.get("username") or (
                kwargs.get("form_data").username if "form_data" in kwargs else None
            )

            if not username:
                return await func(*args, **kwargs)

            key = f"auth:login:username:{username}"
            allowed, retry_after, remaining, limit_val = await check_rate_limit_by_key(
                key, limit, window_seconds
            )

            if not allowed:
                raise RateLimitExceededError(retry_after, limit_val, remaining)

            return await func(*args, **kwargs)

        return wrapper

    return decorator


def rate_limit_by_user_id(limit: int, window_seconds: int):
    """
    Rate limit decorator that limits by authenticated user ID.

    Use for file uploads, exports, etc. to prevent abuse by authenticated users.

    Usage:
        @rate_limit_by_user_id(limit=10, window_seconds=60)
        async def upload_file(current_user: User, ...):
            pass

    Args:
        limit: Maximum requests allowed
        window_seconds: Time window in seconds
    """

    def decorator(func: Callable):
        @wraps(func)
        async def wrapper(*args, **kwargs):
            current_user = kwargs.get("current_user")

            if not current_user:
                return await func(*args, **kwargs)

            key = f"user:{current_user.id}:{func.__name__}"
            allowed, retry_after, remaining, limit_val = await check_rate_limit_by_key(
                key, limit, window_seconds
            )

            if not allowed:
                raise RateLimitExceededError(retry_after, limit_val, remaining)

            return await func(*args, **kwargs)

        return wrapper

    return decorator


async def check_login_rate_limit(request: Request, username: str) -> None:
    """
    Check rate limit for login attempts.

    Enforces TWO limits:
    1. Per IP: 5 attempts/minute (prevent distributed brute force)
    2. Per username: 10 attempts/hour (prevent targeted brute force)

    Args:
        request: FastAPI request
        username: Username being attempted

    Raises:
        RateLimitExceededError: If rate limit exceeded
    """
    ip_key = f"auth:login:ip:{get_remote_address(request)}"
    allowed, retry_after, remaining, limit_val = await check_rate_limit_by_key(ip_key, 5, 60)
    if not allowed:
        raise RateLimitExceededError(retry_after, limit_val, remaining)

    username_key = f"auth:login:username:{username}"
    allowed, retry_after, remaining, limit_val = await check_rate_limit_by_key(
        username_key, 10, 3600
    )
    if not allowed:
        raise RateLimitExceededError(retry_after, limit_val, remaining)


async def reset_login_rate_limit(username: str) -> None:
    """
    Reset rate limit for successful login.

    Call this after successful login to reset the counter.

    Args:
        username: Username that logged in successfully
    """
    try:
        redis_conn = await get_redis()
        key = f"auth:login:username:{username}"
        await redis_conn.delete(key)
    except Exception as e:
        logger.error("rate_limit_reset_failed", error=str(e), exc_info=True)
