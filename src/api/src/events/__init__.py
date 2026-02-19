"""Event bus system for domain events.

This module provides an event-driven architecture foundation with:
- DomainEvent base class for type-safe events
- EventBus for publish-subscribe pattern
- Domain events for document lifecycle
- Subscribers for cross-cutting concerns (caching, audit, metrics)

Example:
    >>> from src.events import EventBus, DomainEvent
    >>> bus = EventBus()
    >>>
    >>> @bus.subscribe("document.indexed")
    >>> async def handle_indexed(event: DomainEvent) -> None:
    ...     print(f"Document indexed: {event.data['document_id']}")
"""

from __future__ import annotations

from .bus import EventBus, subscribe
from .types import DomainEvent

__all__ = ["EventBus", "DomainEvent", "subscribe"]
