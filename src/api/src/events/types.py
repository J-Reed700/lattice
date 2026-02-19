"""Event type definitions and base classes."""

from __future__ import annotations

from datetime import datetime
from typing import Any
from uuid import UUID, uuid4

from pydantic import BaseModel, Field

__all__ = ["DomainEvent"]


class DomainEvent(BaseModel):
    """Base class for all domain events.

    Domain events represent significant occurrences in the business domain.
    They are immutable and carry all necessary information about the event.

    Attributes:
        event_id: Unique identifier for this event instance
        event_type: Type/name of the event (e.g., "document.indexed")
        timestamp: When the event occurred
        data: Event-specific payload data
        metadata: Optional metadata (user_id, correlation_id, etc.)
    """

    event_id: UUID = Field(default_factory=uuid4, description="Unique event identifier")
    event_type: str = Field(..., description="Event type identifier")
    timestamp: datetime = Field(default_factory=datetime.utcnow, description="Event timestamp")
    data: dict[str, Any] = Field(default_factory=dict, description="Event payload")
    metadata: dict[str, Any] = Field(default_factory=dict, description="Event metadata")

    class Config:
        frozen = True
