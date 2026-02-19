"""Audit logging subscriber.

This subscriber listens to all document events and creates audit log
entries for compliance and debugging purposes.
"""

from __future__ import annotations

from typing import Any

import structlog

from src.events.types import DomainEvent

__all__ = ["setup_audit_logging"]

logger = structlog.get_logger(__name__)


async def log_document_indexed(event: DomainEvent) -> None:
    """Create audit log entry for document indexing.

    Args:
        event: DocumentIndexedEvent
    """
    logger.info(
        "audit_log_document_indexed",
        event_id=str(event.event_id),
        document_id=event.data.get("document_id"),
        file_path=event.data.get("file_path"),
        chunk_count=event.data.get("chunk_count"),
        embedding_model=event.data.get("embedding_model"),
        user_id=event.metadata.get("user_id"),
        timestamp=event.timestamp.isoformat(),
    )


async def log_document_indexing_failed(event: DomainEvent) -> None:
    """Create audit log entry for indexing failures.

    Args:
        event: DocumentIndexingFailedEvent
    """
    logger.warning(
        "audit_log_document_indexing_failed",
        event_id=str(event.event_id),
        file_path=event.data.get("file_path"),
        error_type=event.data.get("error_type"),
        error_message=event.data.get("error_message"),
        user_id=event.metadata.get("user_id"),
        timestamp=event.timestamp.isoformat(),
    )


async def log_document_deleted(event: DomainEvent) -> None:
    """Create audit log entry for document deletion.

    Args:
        event: DocumentDeletedEvent
    """
    logger.info(
        "audit_log_document_deleted",
        event_id=str(event.event_id),
        document_id=event.data.get("document_id"),
        file_path=event.data.get("file_path"),
        chunk_count=event.data.get("chunk_count"),
        user_id=event.metadata.get("user_id"),
        reason=event.metadata.get("reason"),
        timestamp=event.timestamp.isoformat(),
    )


def setup_audit_logging(event_bus: Any) -> None:
    """Register audit logging subscribers.

    Args:
        event_bus: EventBus instance to register with
    """
    event_bus.subscribe("document.indexed")(log_document_indexed)
    event_bus.subscribe("document.indexing_failed")(log_document_indexing_failed)
    event_bus.subscribe("document.deleted")(log_document_deleted)

    logger.info("audit_logging_subscribers_registered")
