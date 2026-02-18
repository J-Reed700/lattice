"""Event bus implementation for domain events."""

from __future__ import annotations

import asyncio
from collections import defaultdict
from typing import TYPE_CHECKING, Any

import structlog

if TYPE_CHECKING:
    from collections.abc import Callable

from src.events.types import DomainEvent

__all__ = ["EventBus", "subscribe"]

logger = structlog.get_logger(__name__)


class EventBus:
    """Asynchronous event bus for domain events.

    The EventBus follows a publish-subscribe pattern where subscribers
    register interest in specific event types and are notified when
    those events are published.

    Features:
    - Async/await support
    - Multiple subscribers per event type
    - Concurrent subscriber execution
    - Error isolation (one subscriber failure doesn't affect others)
    - Structured logging

    Example:
        >>> bus = EventBus()
        >>> @bus.subscribe("document.indexed")
        >>> async def handle_indexed(event: DomainEvent) -> None:
        ...     print(f"Document indexed: {event.data['document_id']}")
        >>> await bus.publish(DocumentIndexedEvent(...))
    """

    def __init__(self) -> None:
        """Initialize the event bus."""
        self._subscribers: dict[str, list[Callable[[DomainEvent], Any]]] = defaultdict(list)

    def subscribe(
        self, event_type: str
    ) -> Callable[[Callable[[DomainEvent], Any]], Callable[[DomainEvent], Any]]:
        """Decorator to subscribe a handler to an event type.

        Args:
            event_type: The type of event to subscribe to (e.g., "document.indexed")

        Returns:
            Decorator function that registers the handler

        Example:
            >>> @bus.subscribe("document.indexed")
            >>> async def on_indexed(event: DomainEvent) -> None:
            ...     # Handle event
            ...     pass
        """

        def decorator(
            handler: Callable[[DomainEvent], Any]
        ) -> Callable[[DomainEvent], Any]:
            self._subscribers[event_type].append(handler)
            logger.info(
                "subscriber_registered",
                event_type=event_type,
                handler=handler.__name__,
            )
            return handler

        return decorator

    async def publish(self, event: DomainEvent) -> None:
        """Publish an event to all registered subscribers.

        Subscribers are executed concurrently using asyncio.gather.
        If a subscriber raises an exception, it is logged but does not
        affect other subscribers.

        Args:
            event: The domain event to publish

        Example:
            >>> event = DocumentIndexedEvent(...)
            >>> await bus.publish(event)
        """
        subscribers = self._subscribers.get(event.event_type, [])

        if not subscribers:
            logger.debug(
                "event_published_no_subscribers",
                event_type=event.event_type,
                event_id=str(event.event_id),
            )
            return

        logger.info(
            "event_publishing",
            event_type=event.event_type,
            event_id=str(event.event_id),
            subscriber_count=len(subscribers),
        )

        tasks = [self._execute_subscriber(subscriber, event) for subscriber in subscribers]

        await asyncio.gather(*tasks, return_exceptions=True)

        logger.info(
            "event_published",
            event_type=event.event_type,
            event_id=str(event.event_id),
        )

    async def _execute_subscriber(
        self,
        subscriber: Callable[[DomainEvent], Any],
        event: DomainEvent,
    ) -> None:
        """Execute a subscriber with error handling.

        Args:
            subscriber: The subscriber function to execute
            event: The event to pass to the subscriber
        """
        try:
            result = subscriber(event)
            if asyncio.iscoroutine(result):
                await result
            logger.debug(
                "subscriber_executed",
                subscriber=subscriber.__name__,
                event_type=event.event_type,
                event_id=str(event.event_id),
            )
        except Exception as e:
            logger.error(
                "subscriber_failed",
                subscriber=subscriber.__name__,
                event_type=event.event_type,
                event_id=str(event.event_id),
                error=str(e),
                exc_info=True,
            )


subscribe = EventBus().subscribe
