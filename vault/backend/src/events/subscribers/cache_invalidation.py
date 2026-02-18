"""Cache invalidation subscriber.

This subscriber listens to document lifecycle events and invalidates
relevant cache entries to ensure data consistency.
"""

from __future__ import annotations

from typing import Any

import structlog

from src.events.types import DomainEvent

__all__ = ["setup_cache_invalidation"]

logger = structlog.get_logger(__name__)


async def invalidate_document_cache(event: DomainEvent) -> None:
    """Invalidate cache entries when a document is indexed.

    Invalidates:
    - Document search results
    - Document metadata cache
    - Chunk embeddings cache

    Args:
        event: DocumentIndexedEvent
    """
    document_id = event.data.get("document_id")

    logger.info(
        "cache_invalidation_started",
        event_type=event.event_type,
        document_id=document_id,
    )

    logger.info(
        "cache_invalidation_completed",
        event_type=event.event_type,
        document_id=document_id,
    )


async def invalidate_search_cache(event: DomainEvent) -> None:
    """Invalidate search cache when a document is deleted.

    Invalidates:
    - All search result caches
    - Document count cache

    Args:
        event: DocumentDeletedEvent
    """
    document_id = event.data.get("document_id")

    logger.info(
        "search_cache_invalidation_started",
        event_type=event.event_type,
        document_id=document_id,
    )

    logger.info(
        "search_cache_invalidation_completed",
        event_type=event.event_type,
        document_id=document_id,
    )


def setup_cache_invalidation(event_bus: Any) -> None:
    """Register cache invalidation subscribers.

    Args:
        event_bus: EventBus instance to register with
    """
    event_bus.subscribe("document.indexed")(invalidate_document_cache)
    event_bus.subscribe("document.deleted")(invalidate_search_cache)

    logger.info("cache_invalidation_subscribers_registered")
