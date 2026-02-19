"""Unit tests for EventBus."""

from __future__ import annotations

import asyncio
from unittest.mock import AsyncMock, Mock

import pytest

from src.events import DomainEvent, EventBus


class TestEventBus:
    """Test suite for EventBus functionality."""

    @pytest.fixture()
    def event_bus(self) -> EventBus:
        """Create a fresh EventBus for each test."""
        return EventBus()

    @pytest.fixture()
    def sample_event(self) -> DomainEvent:
        """Create a sample domain event."""
        return DomainEvent(
            event_type="test.event",
            data={"key": "value"},
            metadata={"user_id": "test-user"},
        )

    @pytest.mark.unit()
    def test_subscribe_registers_handler(self, event_bus: EventBus) -> None:
        """Test that subscribe decorator registers handlers."""

        @event_bus.subscribe("test.event")
        def handler(event: DomainEvent) -> None:
            pass

        assert "test.event" in event_bus._subscribers
        assert handler in event_bus._subscribers["test.event"]

    @pytest.mark.unit()
    def test_subscribe_multiple_handlers(self, event_bus: EventBus) -> None:
        """Test that multiple handlers can subscribe to same event."""

        @event_bus.subscribe("test.event")
        def handler1(event: DomainEvent) -> None:
            pass

        @event_bus.subscribe("test.event")
        def handler2(event: DomainEvent) -> None:
            pass

        assert len(event_bus._subscribers["test.event"]) == 2

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_publish_calls_sync_handler(
        self, event_bus: EventBus, sample_event: DomainEvent
    ) -> None:
        """Test that publishing an event calls synchronous handlers."""
        mock_handler = Mock(__name__="mock_handler")
        event_bus.subscribe(sample_event.event_type)(mock_handler)

        await event_bus.publish(sample_event)

        mock_handler.assert_called_once_with(sample_event)

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_publish_calls_async_handler(
        self, event_bus: EventBus, sample_event: DomainEvent
    ) -> None:
        """Test that publishing an event calls asynchronous handlers."""
        mock_handler = AsyncMock()
        event_bus.subscribe(sample_event.event_type)(mock_handler)

        await event_bus.publish(sample_event)

        mock_handler.assert_called_once_with(sample_event)

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_publish_calls_all_handlers(
        self, event_bus: EventBus, sample_event: DomainEvent
    ) -> None:
        """Test that all subscribers receive the event."""
        mock1 = AsyncMock()
        mock2 = AsyncMock()

        event_bus.subscribe(sample_event.event_type)(mock1)
        event_bus.subscribe(sample_event.event_type)(mock2)

        await event_bus.publish(sample_event)

        mock1.assert_called_once_with(sample_event)
        mock2.assert_called_once_with(sample_event)

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_publish_isolates_handler_errors(
        self, event_bus: EventBus, sample_event: DomainEvent
    ) -> None:
        """Test that errors in one handler don't affect others."""
        successful_handler = AsyncMock()
        failing_handler = AsyncMock(side_effect=ValueError("Handler failed"))

        event_bus.subscribe(sample_event.event_type)(failing_handler)
        event_bus.subscribe(sample_event.event_type)(successful_handler)

        await event_bus.publish(sample_event)

        failing_handler.assert_called_once()
        successful_handler.assert_called_once()

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_publish_no_subscribers(
        self, event_bus: EventBus, sample_event: DomainEvent
    ) -> None:
        """Test that publishing with no subscribers is safe."""
        await event_bus.publish(sample_event)

    @pytest.mark.unit()
    @pytest.mark.asyncio()
    async def test_publish_concurrent_execution(self, event_bus: EventBus) -> None:
        """Test that handlers execute concurrently."""
        event = DomainEvent(event_type="test.concurrent", data={})
        execution_order: list[int] = []

        async def handler1(event: DomainEvent) -> None:
            await asyncio.sleep(0.02)
            execution_order.append(1)

        async def handler2(event: DomainEvent) -> None:
            await asyncio.sleep(0.01)
            execution_order.append(2)

        event_bus.subscribe("test.concurrent")(handler1)
        event_bus.subscribe("test.concurrent")(handler2)

        await event_bus.publish(event)

        assert execution_order == [2, 1]

    @pytest.mark.unit()
    def test_event_immutability(self, sample_event: DomainEvent) -> None:
        """Test that events are immutable."""
        with pytest.raises(
            (AttributeError, ValueError), match="(immutable|frozen|cannot assign)"
        ):
            sample_event.data = {"new": "data"}  # type: ignore[misc]

    @pytest.mark.unit()
    def test_subscribe_different_event_types(self, event_bus: EventBus) -> None:
        """Test subscribing to different event types."""

        @event_bus.subscribe("event.type1")
        def handler1(event: DomainEvent) -> None:
            pass

        @event_bus.subscribe("event.type2")
        def handler2(event: DomainEvent) -> None:
            pass

        assert handler1 in event_bus._subscribers["event.type1"]
        assert handler2 in event_bus._subscribers["event.type2"]
        assert handler1 not in event_bus._subscribers["event.type2"]
