"""Search history service for query tracking and suggestions."""

from __future__ import annotations

from datetime import datetime, timedelta
import logging
from typing import TYPE_CHECKING, Any

from sqlalchemy import text
from sqlalchemy.ext.asyncio import AsyncSession

from src.events.domain.search_events import SearchQueryRecorded, SearchQueryRecordingFailed

if TYPE_CHECKING:
    from src.events.bus import EventBus

logger = logging.getLogger(__name__)


class SearchHistoryService:
    """Service for managing search history and suggestions."""

    def __init__(self, db: AsyncSession, event_bus: EventBus | None = None):
        """
        Initialize search history service.

        Args:
            db: Database session
            event_bus: Optional event bus for emitting events (backward compatible)
        """
        self.db = db
        self.event_bus = event_bus

    async def record_search(
        self,
        query: str,
        search_type: str,
        result_count: int,
        execution_time_ms: float,
        user_id: str = "default",
    ):
        """Record a search in history.

        Args:
            query: Search query text
            search_type: Type of search (semantic, keyword, hybrid)
            result_count: Number of results returned
            execution_time_ms: Execution time in milliseconds
            user_id: User ID (default: 'default')
        """
        try:
            await self.db.execute(
                text(
                    """
                    INSERT INTO search_history
                    (query, search_type, result_count, execution_time_ms, filters_json)
                    VALUES (:query, :type, :count, :time, '{}')
                """
                ),
                {
                    "query": query,
                    "type": search_type,
                    "count": result_count,
                    "time": execution_time_ms,
                },
            )
            await self.db.commit()

            # Emit success event (fire-and-forget)
            if self.event_bus:
                event = SearchQueryRecorded(
                    query=query,
                    search_type=search_type,
                    result_count=result_count,
                    execution_time_ms=execution_time_ms,
                    user_id=user_id,
                )
                await self.event_bus.emit(event)

        except Exception as e:
            logger.error(f"Failed to record search history: {e}")

            # Emit failure event (fire-and-forget)
            if self.event_bus:
                event = SearchQueryRecordingFailed(
                    query=query,
                    search_type=search_type,
                    result_count=result_count,
                    execution_time_ms=execution_time_ms,
                    user_id=user_id,
                    error=str(e),
                )
                await self.event_bus.emit(event)

    async def get_recent_queries(self, user_id: str = "default", limit: int = 5) -> list[str]:
        """Get recent unique queries.

        Args:
            user_id: User ID (default: 'default')
            limit: Maximum number of queries to return

        Returns:
            List of recent query strings
        """
        result = await self.db.execute(
            text(
                """
                SELECT DISTINCT query
                FROM search_history
                WHERE query != ''
                ORDER BY created_at DESC
                LIMIT :limit
            """
            ),
            {"limit": limit},
        )
        return [row[0] for row in result.fetchall()]

    async def get_popular_queries(
        self, user_id: str = "default", limit: int = 5, days: int = 30
    ) -> list[dict[str, Any]]:
        """Get most frequent queries in recent period.

        Args:
            user_id: User ID (default: 'default')
            limit: Maximum number of queries to return
            days: Number of days to look back

        Returns:
            List of dicts with 'query' and 'count' keys
        """
        cutoff = int((datetime.now() - timedelta(days=days)).timestamp())

        result = await self.db.execute(
            text(
                """
                SELECT query, COUNT(*) as count
                FROM search_history
                WHERE created_at > :cutoff AND query != ''
                GROUP BY query
                ORDER BY count DESC
                LIMIT :limit
            """
            ),
            {"cutoff": cutoff, "limit": limit},
        )
        return [{"query": row[0], "count": row[1]} for row in result.fetchall()]
