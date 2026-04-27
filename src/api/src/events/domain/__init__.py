"""Domain events for business domain entities."""

from __future__ import annotations

from .document_events import (
    DocumentDeletedEvent,
    DocumentIndexedEvent,
    DocumentIndexingFailedEvent,
)

__all__ = [
    "DocumentIndexedEvent",
    "DocumentIndexingFailedEvent",
    "DocumentDeletedEvent",
]
