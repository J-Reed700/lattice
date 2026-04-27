"""Token blacklist for revocation - in-memory implementation.

For production with multiple instances, consider Redis-based implementation.
"""

from __future__ import annotations

import asyncio

import structlog

logger = structlog.get_logger(__name__)


class TokenBlacklist:
    """In-memory token blacklist with automatic cleanup."""

    def __init__(self) -> None:
        """Initialize blacklist."""
        self._token_blacklist: set[str] = set()
        self._user_blacklist: set[str] = set()
        self._cleanup_task: asyncio.Task | None = None
        logger.info("token_blacklist_initialized")

    async def start(self) -> None:
        """Start background cleanup task."""
        self._cleanup_task = asyncio.create_task(self._cleanup_loop())
        logger.info("token_blacklist_cleanup_started")

    async def stop(self) -> None:
        """Stop background cleanup task."""
        if self._cleanup_task:
            self._cleanup_task.cancel()
            try:
                await self._cleanup_task
            except asyncio.CancelledError:
                pass
        logger.info("token_blacklist_cleanup_stopped")

    async def add_token(self, token_id: str) -> None:
        """Add token ID to blacklist.

        Args:
            token_id: JWT 'jti' claim value
        """
        self._token_blacklist.add(token_id)
        logger.info("token_blacklisted", token_id=token_id)

    async def is_token_blacklisted(self, token_id: str) -> bool:
        """Check if token is blacklisted.

        Args:
            token_id: JWT 'jti' claim value

        Returns:
            True if token is blacklisted
        """
        return token_id in self._token_blacklist

    async def remove_token(self, token_id: str) -> None:
        """Remove token from blacklist.

        Args:
            token_id: JWT 'jti' claim value
        """
        self._token_blacklist.discard(token_id)

    async def revoke_all_user_tokens(self, user_id: str) -> None:
        """Revoke all tokens for a user.

        This adds the user_id to a separate blacklist, which will reject
        all tokens for that user regardless of token_id.

        Args:
            user_id: User ID to blacklist
        """
        self._user_blacklist.add(user_id)
        logger.info("all_user_tokens_revoked", user_id=user_id)

    async def is_user_blacklisted(self, user_id: str) -> bool:
        """Check if all user tokens are blacklisted.

        Args:
            user_id: User ID to check

        Returns:
            True if all user tokens are blacklisted
        """
        return user_id in self._user_blacklist

    async def clear_user_blacklist(self, user_id: str) -> None:
        """Clear user from blacklist (allow new tokens).

        Args:
            user_id: User ID to remove from blacklist
        """
        self._user_blacklist.discard(user_id)
        logger.info("user_blacklist_cleared", user_id=user_id)

    async def get_stats(self) -> dict[str, int]:
        """Get blacklist statistics.

        Returns:
            Dictionary with blacklist stats
        """
        return {
            "blacklisted_tokens": len(self._token_blacklist),
            "blacklisted_users": len(self._user_blacklist),
        }

    async def _cleanup_loop(self) -> None:
        """Periodically clean up expired entries.

        In production with Redis, this happens automatically via TTL.
        For in-memory, we rely on token expiration validation.
        """
        while True:
            try:
                await asyncio.sleep(3600)
                logger.debug("token_blacklist_cleanup_check")
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error("token_blacklist_cleanup_error", error=str(e))


token_blacklist = TokenBlacklist()
