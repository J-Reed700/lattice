"""MFA rate limiting to prevent brute-force attacks on temp tokens."""

from __future__ import annotations

import asyncio
from datetime import UTC, datetime, timedelta

import structlog

logger = structlog.get_logger(__name__)


class MFARateLimiter:
    """In-memory rate limiter for MFA attempts per temp token.

    Prevents brute-force attacks by limiting failed MFA attempts per temp token.
    Automatically cleans up expired entries.

    Security:
        - Max 3 failed attempts per temp token
        - Temp tokens are blacklisted after max attempts
        - Automatic cleanup every 5 minutes
        - Prevents distributed brute-force attacks
    """

    # Maximum failed attempts before blacklisting the temp token
    MAX_ATTEMPTS = 3

    # Time window for rate limiting (5 minutes = temp token lifetime)
    RATE_LIMIT_WINDOW = timedelta(minutes=5)

    def __init__(self):
        """Initialize the MFA rate limiter."""
        # temp_token_id -> (attempt_count, first_attempt_time)
        self._attempts: dict[str, tuple[int, datetime]] = {}
        self._cleanup_task: asyncio.Task | None = None
        self._lock = asyncio.Lock()

    async def start(self) -> None:
        """Start the background cleanup task."""
        if self._cleanup_task is None or self._cleanup_task.done():
            self._cleanup_task = asyncio.create_task(self._cleanup_loop())
            logger.info("mfa_rate_limiter_started")

    async def stop(self) -> None:
        """Stop the background cleanup task."""
        if self._cleanup_task and not self._cleanup_task.done():
            self._cleanup_task.cancel()
            try:
                await self._cleanup_task
            except asyncio.CancelledError:
                pass
            logger.info("mfa_rate_limiter_stopped")

    async def record_attempt(self, temp_token_id: str, success: bool) -> None:
        """Record an MFA verification attempt.

        Args:
            temp_token_id: The temp token JTI
            success: Whether the MFA verification succeeded

        Note:
            If success=True, the entry is removed from tracking.
            If success=False, the failure count is incremented.
        """
        async with self._lock:
            if success:
                # Successful attempt - remove from tracking
                if temp_token_id in self._attempts:
                    del self._attempts[temp_token_id]
                    logger.info(
                        "mfa_attempt_success_recorded",
                        temp_token_id=temp_token_id,
                    )
            else:
                # Failed attempt - increment counter
                if temp_token_id in self._attempts:
                    count, first_time = self._attempts[temp_token_id]
                    self._attempts[temp_token_id] = (count + 1, first_time)
                else:
                    self._attempts[temp_token_id] = (1, datetime.now(UTC))

                new_count = self._attempts[temp_token_id][0]
                logger.warning(
                    "mfa_attempt_failure_recorded",
                    temp_token_id=temp_token_id,
                    attempt_count=new_count,
                    max_attempts=self.MAX_ATTEMPTS,
                )

    async def is_rate_limited(self, temp_token_id: str) -> bool:
        """Check if a temp token has exceeded the rate limit.

        Args:
            temp_token_id: The temp token JTI

        Returns:
            True if the temp token should be blocked due to rate limiting

        Note:
            Rate limiting is based on:
            - Number of failed attempts (MAX_ATTEMPTS)
            - Time window (RATE_LIMIT_WINDOW)
        """
        async with self._lock:
            if temp_token_id not in self._attempts:
                return False

            count, first_time = self._attempts[temp_token_id]

            # Check if rate limit window has expired
            if datetime.now(UTC) - first_time > self.RATE_LIMIT_WINDOW:
                # Window expired - reset
                del self._attempts[temp_token_id]
                return False

            # Check if max attempts exceeded
            if count >= self.MAX_ATTEMPTS:
                logger.warning(
                    "mfa_rate_limit_exceeded",
                    temp_token_id=temp_token_id,
                    attempt_count=count,
                    max_attempts=self.MAX_ATTEMPTS,
                )
                return True

            return False

    async def get_remaining_attempts(self, temp_token_id: str) -> int:
        """Get remaining MFA attempts for a temp token.

        Args:
            temp_token_id: The temp token JTI

        Returns:
            Number of remaining attempts before rate limiting kicks in
        """
        async with self._lock:
            if temp_token_id not in self._attempts:
                return self.MAX_ATTEMPTS

            count, first_time = self._attempts[temp_token_id]

            # Check if window expired
            if datetime.now(UTC) - first_time > self.RATE_LIMIT_WINDOW:
                return self.MAX_ATTEMPTS

            return max(0, self.MAX_ATTEMPTS - count)

    async def reset(self, temp_token_id: str) -> None:
        """Reset rate limiting for a temp token.

        Args:
            temp_token_id: The temp token JTI
        """
        async with self._lock:
            if temp_token_id in self._attempts:
                del self._attempts[temp_token_id]
                logger.info("mfa_rate_limit_reset", temp_token_id=temp_token_id)

    async def _cleanup_loop(self) -> None:
        """Background task to clean up expired rate limit entries."""
        while True:
            try:
                await asyncio.sleep(300)  # Run every 5 minutes
                await self._cleanup_expired()
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error("mfa_rate_limiter_cleanup_error", error=str(e))

    async def _cleanup_expired(self) -> None:
        """Remove expired rate limit entries."""
        async with self._lock:
            now = datetime.now(UTC)
            expired_tokens = [
                token_id
                for token_id, (_, first_time) in self._attempts.items()
                if now - first_time > self.RATE_LIMIT_WINDOW
            ]

            for token_id in expired_tokens:
                del self._attempts[token_id]

            if expired_tokens:
                logger.info(
                    "mfa_rate_limiter_cleanup",
                    expired_count=len(expired_tokens),
                    remaining_count=len(self._attempts),
                )

    async def get_stats(self) -> dict[str, int]:
        """Get current rate limiter statistics.

        Returns:
            Dictionary with statistics
        """
        async with self._lock:
            return {
                "total_tracked_tokens": len(self._attempts),
                "max_attempts_per_token": self.MAX_ATTEMPTS,
                "rate_limit_window_minutes": int(self.RATE_LIMIT_WINDOW.total_seconds() / 60),
            }


# Global singleton instance
mfa_rate_limiter = MFARateLimiter()
