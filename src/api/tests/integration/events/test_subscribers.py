"""Integration tests for event subscribers."""

from __future__ import annotations

from unittest.mock import patch

import pytest

from src.events import EventBus
from src.events.domain import (
    DocumentDeletedEvent,
    DocumentIndexedEvent,
    DocumentIndexingFailedEvent,
)
from src.events.subscribers import (
    setup_audit_logging,
    setup_cache_invalidation,
    setup_metrics,
)


class TestCacheInvalidationSubscriber:
    """Test suite for cache invalidation subscriber."""

    @pytest.fixture()
    def event_bus(self) -> EventBus:
        """Create event bus with cache invalidation subscribers."""
        bus = EventBus()
        setup_cache_invalidation(bus)
        return bus

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_invalidate_cache_on_document_indexed(
        self, event_bus: EventBus
    ) -> None:
        """Test that cache is invalidated when document is indexed."""
        event = DocumentIndexedEvent(
            document_id=123,
            file_path="/path/to/doc.pdf",
            chunk_count=10,
            embedding_model="all-MiniLM-L6-v2",
        )

        with patch(
            "src.events.subscribers.cache_invalidation.logger"
        ) as mock_logger:
            await event_bus.publish(event)

            assert mock_logger.info.called

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_invalidate_search_cache_on_document_deleted(
        self, event_bus: EventBus
    ) -> None:
        """Test that search cache is invalidated when document is deleted."""
        event = DocumentDeletedEvent(
            document_id=123,
            file_path="/path/to/doc.pdf",
            chunk_count=10,
        )

        with patch(
            "src.events.subscribers.cache_invalidation.logger"
        ) as mock_logger:
            await event_bus.publish(event)

            assert mock_logger.info.called


class TestAuditLoggingSubscriber:
    """Test suite for audit logging subscriber."""

    @pytest.fixture()
    def event_bus(self) -> EventBus:
        """Create event bus with audit logging subscribers."""
        bus = EventBus()
        setup_audit_logging(bus)
        return bus

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_log_document_indexed(self, event_bus: EventBus) -> None:
        """Test that document indexed event is logged."""
        event = DocumentIndexedEvent(
            document_id=123,
            file_path="/path/to/doc.pdf",
            chunk_count=10,
            embedding_model="all-MiniLM-L6-v2",
            metadata={"user_id": "user-123"},
        )

        with patch("src.events.subscribers.audit_log.logger") as mock_logger:
            await event_bus.publish(event)

            mock_logger.info.assert_called()
            call_args = mock_logger.info.call_args
            assert call_args[0][0] == "audit_log_document_indexed"
            assert call_args[1]["document_id"] == 123

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_log_document_indexing_failed(self, event_bus: EventBus) -> None:
        """Test that indexing failed event is logged."""
        event = DocumentIndexingFailedEvent(
            file_path="/path/to/doc.pdf",
            error_type="ExtractionError",
            error_message="Failed to extract text",
        )

        with patch("src.events.subscribers.audit_log.logger") as mock_logger:
            await event_bus.publish(event)

            mock_logger.warning.assert_called()
            call_args = mock_logger.warning.call_args
            assert call_args[0][0] == "audit_log_document_indexing_failed"

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_log_document_deleted(self, event_bus: EventBus) -> None:
        """Test that document deleted event is logged."""
        event = DocumentDeletedEvent(
            document_id=123,
            file_path="/path/to/doc.pdf",
            chunk_count=10,
            metadata={"reason": "manual_deletion"},
        )

        with patch("src.events.subscribers.audit_log.logger") as mock_logger:
            await event_bus.publish(event)

            mock_logger.info.assert_called()
            call_args = mock_logger.info.call_args
            assert call_args[0][0] == "audit_log_document_deleted"


class TestMetricsSubscriber:
    """Test suite for metrics tracking subscriber."""

    @pytest.fixture()
    def event_bus(self) -> EventBus:
        """Create event bus with metrics subscribers."""
        bus = EventBus()
        setup_metrics(bus)
        return bus

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_increment_indexed_documents(self, event_bus: EventBus) -> None:
        """Test that indexed documents metric is incremented."""
        event = DocumentIndexedEvent(
            document_id=123,
            file_path="/path/to/doc.pdf",
            chunk_count=10,
            embedding_model="all-MiniLM-L6-v2",
            metadata={"indexing_duration_ms": 1500},
        )

        with patch("src.events.subscribers.metrics.logger") as mock_logger:
            await event_bus.publish(event)

            mock_logger.info.assert_called()
            call_args = mock_logger.info.call_args
            assert call_args[0][0] == "metrics_document_indexed"

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_increment_failed_indexing(self, event_bus: EventBus) -> None:
        """Test that failed indexing metric is incremented."""
        event = DocumentIndexingFailedEvent(
            file_path="/path/to/doc.pdf",
            error_type="ValidationError",
            error_message="Document too large",
        )

        with patch("src.events.subscribers.metrics.logger") as mock_logger:
            await event_bus.publish(event)

            mock_logger.info.assert_called()
            call_args = mock_logger.info.call_args
            assert call_args[0][0] == "metrics_indexing_failed"

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_increment_deleted_documents(self, event_bus: EventBus) -> None:
        """Test that deleted documents metric is incremented."""
        event = DocumentDeletedEvent(
            document_id=123,
            file_path="/path/to/doc.pdf",
            chunk_count=10,
        )

        with patch("src.events.subscribers.metrics.logger") as mock_logger:
            await event_bus.publish(event)

            mock_logger.info.assert_called()
            call_args = mock_logger.info.call_args
            assert call_args[0][0] == "metrics_document_deleted"


class TestSubscriberIntegration:
    """Test suite for subscriber integration."""

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_all_subscribers_receive_event(self) -> None:
        """Test that all subscribers receive events when registered."""
        bus = EventBus()

        setup_cache_invalidation(bus)
        setup_audit_logging(bus)
        setup_metrics(bus)

        event = DocumentIndexedEvent(
            document_id=123,
            file_path="/path/to/doc.pdf",
            chunk_count=10,
            embedding_model="all-MiniLM-L6-v2",
        )

        with patch(
            "src.events.subscribers.cache_invalidation.logger"
        ) as cache_logger, patch(
            "src.events.subscribers.audit_log.logger"
        ) as audit_logger, patch(
            "src.events.subscribers.metrics.logger"
        ) as metrics_logger:
            await bus.publish(event)

            assert cache_logger.info.called
            assert audit_logger.info.called
            assert metrics_logger.info.called

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_subscriber_error_isolation(self) -> None:
        """Test that errors in one subscriber don't affect others."""
        bus = EventBus()

        @bus.subscribe("document.indexed")
        async def failing_subscriber(event: DocumentIndexedEvent) -> None:
            raise ValueError("Subscriber failed")

        @bus.subscribe("document.indexed")
        async def successful_subscriber(event: DocumentIndexedEvent) -> None:
            pass

        event = DocumentIndexedEvent(
            document_id=123,
            file_path="/path/to/doc.pdf",
            chunk_count=10,
            embedding_model="all-MiniLM-L6-v2",
        )

        await bus.publish(event)
