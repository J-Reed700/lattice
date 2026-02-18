"""Metrics tracking subscriber.

This subscriber listens to document events and updates metrics
for monitoring and observability.
"""

from __future__ import annotations

from typing import Any

import structlog

from src.events.types import DomainEvent

__all__ = ["setup_metrics"]

logger = structlog.get_logger(__name__)


async def increment_indexed_documents(event: DomainEvent) -> None:
    """Increment indexed documents counter.

    Args:
        event: DocumentIndexedEvent
    """
    logger.info(
        "metrics_document_indexed",
        event_id=str(event.event_id),
        document_id=event.data.get("document_id"),
        chunk_count=event.data.get("chunk_count"),
        indexing_duration_ms=event.metadata.get("indexing_duration_ms"),
    )


async def increment_failed_indexing(event: DomainEvent) -> None:
    """Increment failed indexing counter.

    Args:
        event: DocumentIndexingFailedEvent
    """
    logger.info(
        "metrics_indexing_failed",
        event_id=str(event.event_id),
        error_type=event.data.get("error_type"),
    )


async def increment_deleted_documents(event: DomainEvent) -> None:
    """Increment deleted documents counter.

    Args:
        event: DocumentDeletedEvent
    """
    logger.info(
        "metrics_document_deleted",
        event_id=str(event.event_id),
        document_id=event.data.get("document_id"),
        chunk_count=event.data.get("chunk_count"),
    )


def setup_metrics(event_bus: Any) -> None:
    """Register metrics tracking subscribers.

    Args:
        event_bus: EventBus instance to register with
    """
    event_bus.subscribe("document.indexed")(increment_indexed_documents)
    event_bus.subscribe("document.indexing_failed")(increment_failed_indexing)
    event_bus.subscribe("document.deleted")(increment_deleted_documents)

    logger.info("metrics_subscribers_registered")
