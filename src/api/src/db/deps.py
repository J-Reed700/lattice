"""FastAPI dependency injection for database and services."""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from fastapi import Request

    from src.events import EventBus

__all__ = ["get_event_bus"]


def get_event_bus(request: Request) -> EventBus:
    """Get EventBus instance from app state.

    Args:
        request: FastAPI request object

    Returns:
        EventBus instance

    Example:
        >>> @router.post("/index")
        >>> async def index_document(
        ...     event_bus: EventBus = Depends(get_event_bus),
        ... ):
        ...     service = IndexingService(event_bus=event_bus)
        ...     await service.index_file(...)
    """
    return request.app.state.event_bus
