"""Function executor with security controls.

Executes function calls with:
- Rate limiting (per-function limits)
- Input validation (Pydantic schemas)
- Audit logging (success/failure)
- Error handling (user-friendly messages)
"""

from __future__ import annotations

import asyncio
from collections import defaultdict
from datetime import datetime, timedelta
import logging
from typing import TYPE_CHECKING, Any

from pydantic import ValidationError

if TYPE_CHECKING:
    from .registry import FunctionRegistry

logger = logging.getLogger(__name__)


def _sanitize_user_id(user_id: str | None) -> str:
    """
    Sanitize user ID for logging (prevents PII exposure).

    Masks the user ID to show only first/last 4 characters.

    Args:
        user_id: User ID to sanitize

    Returns:
        Sanitized user ID string
    """
    if not user_id:
        return "anonymous"

    # If user_id is too short, just return masked
    if len(user_id) <= 8:
        return "***"

    # Show first 4 and last 4 chars
    return f"{user_id[:4]}...{user_id[-4:]}"


class RateLimiter:
    """Per-function rate limiter using sliding window.

    Tracks requests per function and enforces per-minute limits.
    """

    def __init__(self) -> None:
        """Initialize rate limiter."""
        self._requests: dict[str, list[datetime]] = defaultdict(list)
        self._lock = asyncio.Lock()

    async def check(self, function_name: str, limit: int) -> None:
        """
        Check if request is within rate limit.

        Args:
            function_name: Function name
            limit: Maximum requests per minute

        Raises:
            RateLimitError: If rate limit exceeded
        """
        async with self._lock:
            now = datetime.now()
            window_start = now - timedelta(minutes=1)

            # Remove old requests outside window
            self._requests[function_name] = [
                req_time for req_time in self._requests[function_name] if req_time > window_start
            ]

            # Check limit
            if len(self._requests[function_name]) >= limit:
                msg = (
                    f"Rate limit exceeded for {function_name}: "
                    f"{len(self._requests[function_name])} requests in last minute (limit: {limit})"
                )
                raise RateLimitError(msg)

            # Add current request
            self._requests[function_name].append(now)

    async def reset(self, function_name: str | None = None) -> None:
        """
        Reset rate limiter.

        Args:
            function_name: Specific function to reset, or None for all
        """
        async with self._lock:
            if function_name:
                self._requests.pop(function_name, None)
            else:
                self._requests.clear()


class RateLimitError(Exception):
    """Raised when rate limit is exceeded."""


class FunctionCallError(Exception):
    """Raised when function call fails."""

    def __init__(self, error_code: str, message: str, details: dict[str, str] | None = None):
        """
        Initialize function call error.

        Args:
            error_code: Machine-readable error code
            message: Human-readable message
            details: Additional context
        """
        self.error_code = error_code
        self.message = message
        self.details = details or {}
        super().__init__(message)


class FunctionExecutor:
    """Executes function calls with security controls and audit logging.

    Features:
    - Rate limiting per function
    - Input validation via Pydantic
    - Audit logging for all calls
    - Graceful error handling

    Example:
        >>> executor = FunctionExecutor(registry)
        >>> result = await executor.execute("semantic_search", {"query": "test"})
    """

    def __init__(self, registry: FunctionRegistry):
        """
        Initialize executor.

        Args:
            registry: Function registry with tool definitions
        """
        self.registry = registry
        self.rate_limiter = RateLimiter()

    async def execute(
        self,
        function_name: str,
        arguments: dict[str, Any],
        user_id: str | None = None,
    ) -> dict[str, Any]:
        """
        Execute a function call with security controls.

        Process:
        1. Validate function exists
        2. Check rate limit
        3. Validate input with Pydantic
        4. Execute function handler
        5. Validate output with Pydantic
        6. Log audit event

        Args:
            function_name: Name of function to call
            arguments: Function arguments as dict
            user_id: Optional user ID for audit logging

        Returns:
            Function result as dict

        Raises:
            FunctionCallError: If function call fails
            RateLimitError: If rate limit exceeded
        """
        # Get tool definition
        tool = self.registry.get_tool(function_name)
        if not tool:
            logger.error(f"Function not found: {function_name}")
            raise FunctionCallError(
                error_code="FUNCTION_NOT_FOUND",
                message=f"Function '{function_name}' not found",
                details={"available_functions": [t.name for t in self.registry.list_tools()]},
            )

        # Rate limiting
        try:
            await self.rate_limiter.check(function_name, tool.rate_limit)
        except RateLimitError as e:
            logger.warning(f"Rate limit exceeded for {function_name}: {_sanitize_user_id(user_id)}")
            self._audit_failure(
                function_name=function_name,
                user_id=user_id,
                error=str(e),
            )
            raise

        # Input validation
        try:
            input_obj = tool.input_schema(**arguments)
        except ValidationError as e:
            logger.error(f"Input validation failed for {function_name}: {e}")
            self._audit_failure(
                function_name=function_name,
                user_id=user_id,
                error=f"Invalid input: {e}",
            )
            raise FunctionCallError(
                error_code="INVALID_INPUT",
                message="Input validation failed",
                details={"errors": e.errors()},
            ) from e

        # Execute function
        try:
            start_time = datetime.now()
            result = await tool.handler(input_obj)
            execution_time = (datetime.now() - start_time).total_seconds() * 1000

            # Validate output
            if not isinstance(result, tool.output_schema):
                logger.error(f"Output validation failed for {function_name}: wrong type")
                raise FunctionCallError(
                    error_code="INVALID_OUTPUT",
                    message="Function returned invalid output type",
                )

            # Convert to dict
            result_dict = result.model_dump()

            # Audit success
            self._audit_success(
                function_name=function_name,
                user_id=user_id,
                execution_time_ms=execution_time,
            )

            logger.info(
                f"Function executed successfully: {function_name} "
                f"(user: {_sanitize_user_id(user_id)}, time: {execution_time:.2f}ms)"
            )

            return result_dict

        except FileNotFoundError as e:
            logger.error(f"Resource not found in {function_name}: {e}")
            self._audit_failure(function_name, user_id, str(e))
            raise FunctionCallError(
                error_code="RESOURCE_NOT_FOUND",
                message=str(e),
            ) from e

        except ValueError as e:
            logger.error(f"Value error in {function_name}: {e}")
            self._audit_failure(function_name, user_id, str(e))
            raise FunctionCallError(
                error_code="INVALID_VALUE",
                message=str(e),
            ) from e

        except RuntimeError as e:
            logger.error(f"Runtime error in {function_name}: {e}")
            self._audit_failure(function_name, user_id, str(e))
            raise FunctionCallError(
                error_code="EXECUTION_FAILED",
                message=str(e),
            ) from e

        except Exception as e:
            logger.exception(f"Unexpected error in {function_name}: {e}")
            self._audit_failure(function_name, user_id, str(e))
            raise FunctionCallError(
                error_code="INTERNAL_ERROR",
                message=f"Unexpected error: {e!s}",
            ) from e

    def _audit_success(
        self,
        function_name: str,
        user_id: str | None,
        execution_time_ms: float,
    ) -> None:
        """
        Log successful function call.

        Args:
            function_name: Function name
            user_id: User ID (if any)
            execution_time_ms: Execution time in milliseconds
        """
        logger.info(
            "Function call succeeded",
            extra={
                "event": "function_call_success",
                "function": function_name,
                "user_id": user_id or "anonymous",
                "execution_time_ms": execution_time_ms,
            },
        )

    def _audit_failure(
        self,
        function_name: str,
        user_id: str | None,
        error: str,
    ) -> None:
        """
        Log failed function call.

        Args:
            function_name: Function name
            user_id: User ID (if any)
            error: Error message
        """
        logger.error(
            "Function call failed",
            extra={
                "event": "function_call_failure",
                "function": function_name,
                "user_id": user_id or "anonymous",
                "error": error,
            },
        )

    async def reset_rate_limits(self, function_name: str | None = None) -> None:
        """
        Reset rate limits for testing or maintenance.

        Args:
            function_name: Specific function to reset, or None for all
        """
        await self.rate_limiter.reset(function_name)
        logger.info(f"Rate limits reset: {function_name or 'all functions'}")
