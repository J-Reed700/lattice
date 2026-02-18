"""End-to-end integration tests for indexing events.

This test suite verifies that the event-driven architecture integrates
correctly with the IndexingService, ensuring events are published when
documents are indexed, fail, or are deleted.
"""

from __future__ import annotations

from pathlib import Path
import tempfile
from typing import TYPE_CHECKING
from unittest.mock import patch
from uuid import uuid4

import pytest
from sqlalchemy import select

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
from src.models import File
from src.services.indexing import IndexingService

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession


class TestIndexingServiceEventIntegration:
    """Test suite for IndexingService event integration."""

    @pytest.fixture()
    def event_bus(self) -> EventBus:
        """Create event bus with all subscribers registered."""
        bus = EventBus()
        setup_audit_logging(bus)
        setup_cache_invalidation(bus)
        setup_metrics(bus)
        return bus

    @pytest.fixture()
    def indexing_service(self, event_bus: EventBus) -> IndexingService:
        """Create IndexingService with event bus."""
        return IndexingService(event_bus=event_bus)

    @pytest.fixture()
    def indexing_service_no_events(self) -> IndexingService:
        """Create IndexingService without event bus (backward compatibility)."""
        return IndexingService()

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_index_file_success_emits_event(
        self,
        indexing_service: IndexingService,
        db_session: AsyncSession,
    ) -> None:
        """Test that successful indexing emits DocumentIndexedEvent."""
        # Create a test file
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".txt", delete=False
        ) as tmp_file:
            tmp_file.write("Test content for indexing")
            tmp_path = Path(tmp_file.name)

        try:
            # Create File record
            file = File(
                id=uuid4(),
                path=str(tmp_path),
                mime_type="text/plain",
                file_size=len("Test content for indexing"),
                processing_status="pending",
            )
            db_session.add(file)
            await db_session.commit()

            # Track published events
            published_events = []

            async def capture_event(event: DocumentIndexedEvent) -> None:
                published_events.append(event)

            indexing_service.event_bus.subscribe("document.indexed")(capture_event)

            # Index the file
            result = await indexing_service.index_file(file.id, db_session)

            assert result is True

            # Verify event was published
            assert len(published_events) == 1
            event = published_events[0]
            assert isinstance(event, DocumentIndexedEvent)
            assert event.data["document_id"] == int(file.id)
            assert event.data["file_path"] == str(tmp_path)
            assert event.data["chunk_count"] >= 1
            assert event.data["embedding_model"] is not None

        finally:
            # Cleanup
            tmp_path.unlink(missing_ok=True)

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_index_file_failure_emits_event(
        self,
        indexing_service: IndexingService,
        db_session: AsyncSession,
    ) -> None:
        """Test that indexing failure emits DocumentIndexingFailedEvent."""
        # Create File record with non-existent path
        file = File(
            id=uuid4(),
            path="/nonexistent/path/to/file.txt",
            mime_type="text/plain",
            file_size=100,
            processing_status="pending",
        )
        db_session.add(file)
        await db_session.commit()

        # Track published events
        published_events = []

        async def capture_event(event: DocumentIndexingFailedEvent) -> None:
            published_events.append(event)

        indexing_service.event_bus.subscribe("document.indexing_failed")(
            capture_event
        )

        # Attempt to index the file (should fail)
        with pytest.raises(FileNotFoundError):
            await indexing_service.index_file(file.id, db_session)

        # Verify event was published
        assert len(published_events) == 1
        event = published_events[0]
        assert isinstance(event, DocumentIndexingFailedEvent)
        assert event.data["file_path"] == "/nonexistent/path/to/file.txt"
        assert event.data["error_type"] == "FileNotFoundError"
        assert "does not exist" in event.data["error_message"]

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_delete_file_index_emits_event(
        self,
        indexing_service: IndexingService,
        db_session: AsyncSession,
    ) -> None:
        """Test that deleting file index emits DocumentDeletedEvent."""
        # Create a test file
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".txt", delete=False
        ) as tmp_file:
            tmp_file.write("Test content for deletion")
            tmp_path = Path(tmp_file.name)

        try:
            # Create and index file
            file = File(
                id=uuid4(),
                path=str(tmp_path),
                mime_type="text/plain",
                file_size=len("Test content for deletion"),
                processing_status="pending",
            )
            db_session.add(file)
            await db_session.commit()

            # Index the file first
            await indexing_service.index_file(file.id, db_session)

            # Track published events
            published_events = []

            async def capture_event(event: DocumentDeletedEvent) -> None:
                published_events.append(event)

            indexing_service.event_bus.subscribe("document.deleted")(capture_event)

            # Delete the index
            result = await indexing_service.delete_file_index(file.id, db_session)

            assert result is True

            # Verify event was published
            assert len(published_events) == 1
            event = published_events[0]
            assert isinstance(event, DocumentDeletedEvent)
            assert event.data["document_id"] == int(file.id)
            assert event.data["file_path"] == str(tmp_path)

        finally:
            # Cleanup
            tmp_path.unlink(missing_ok=True)

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_all_subscribers_receive_indexing_events(
        self,
        indexing_service: IndexingService,
        db_session: AsyncSession,
    ) -> None:
        """Test that all subscribers (audit, cache, metrics) receive events."""
        # Create a test file
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".txt", delete=False
        ) as tmp_file:
            tmp_file.write("Test content for subscribers")
            tmp_path = Path(tmp_file.name)

        try:
            # Create File record
            file = File(
                id=uuid4(),
                path=str(tmp_path),
                mime_type="text/plain",
                file_size=len("Test content for subscribers"),
                processing_status="pending",
            )
            db_session.add(file)
            await db_session.commit()

            # Patch all subscriber loggers to verify they're called
            with patch(
                "src.events.subscribers.audit_log.logger"
            ) as audit_logger, patch(
                "src.events.subscribers.cache_invalidation.logger"
            ) as cache_logger, patch(
                "src.events.subscribers.metrics.logger"
            ) as metrics_logger:
                # Index the file
                await indexing_service.index_file(file.id, db_session)

                # Verify all subscribers were invoked
                assert audit_logger.info.called
                assert cache_logger.info.called
                assert metrics_logger.info.called

        finally:
            # Cleanup
            tmp_path.unlink(missing_ok=True)

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_backward_compatibility_no_event_bus(
        self,
        indexing_service_no_events: IndexingService,
        db_session: AsyncSession,
    ) -> None:
        """Test that IndexingService works without event bus (backward compat)."""
        # Create a test file
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".txt", delete=False
        ) as tmp_file:
            tmp_file.write("Test backward compatibility")
            tmp_path = Path(tmp_file.name)

        try:
            # Create File record
            file = File(
                id=uuid4(),
                path=str(tmp_path),
                mime_type="text/plain",
                file_size=len("Test backward compatibility"),
                processing_status="pending",
            )
            db_session.add(file)
            await db_session.commit()

            # Index without event bus - should not crash
            result = await indexing_service_no_events.index_file(file.id, db_session)

            assert result is True

            # Verify file was indexed
            result = await db_session.execute(select(File).where(File.id == file.id))
            indexed_file = result.scalar_one()
            assert indexed_file.processing_status == "indexed"

        finally:
            # Cleanup
            tmp_path.unlink(missing_ok=True)

    @pytest.mark.integration()
    @pytest.mark.asyncio()
    async def test_subscriber_error_does_not_block_indexing(
        self,
        event_bus: EventBus,
        db_session: AsyncSession,
    ) -> None:
        """Test that subscriber errors don't prevent successful indexing."""
        # Add a failing subscriber
        @event_bus.subscribe("document.indexed")
        async def failing_subscriber(event: DocumentIndexedEvent) -> None:
            raise ValueError("Subscriber intentionally failed")

        indexing_service = IndexingService(event_bus=event_bus)

        # Create a test file
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".txt", delete=False
        ) as tmp_file:
            tmp_file.write("Test error isolation")
            tmp_path = Path(tmp_file.name)

        try:
            # Create File record
            file = File(
                id=uuid4(),
                path=str(tmp_path),
                mime_type="text/plain",
                file_size=len("Test error isolation"),
                processing_status="pending",
            )
            db_session.add(file)
            await db_session.commit()

            # Index should succeed despite failing subscriber
            result = await indexing_service.index_file(file.id, db_session)

            assert result is True

            # Verify file was indexed
            result = await db_session.execute(select(File).where(File.id == file.id))
            indexed_file = result.scalar_one()
            assert indexed_file.processing_status == "indexed"

        finally:
            # Cleanup
            tmp_path.unlink(missing_ok=True)


class TestEventBusDependencyInjection:
    """Test suite for EventBus dependency injection."""

    @pytest.mark.integration()
    async def test_get_event_bus_dependency(self) -> None:
        """Test that get_event_bus dependency works correctly."""
        from unittest.mock import MagicMock

        from src.db.deps import get_event_bus

        # Create mock request with event bus in state
        mock_request = MagicMock()
        mock_event_bus = EventBus()
        mock_request.app.state.event_bus = mock_event_bus

        # Get event bus via dependency
        result = get_event_bus(mock_request)

        assert result is mock_event_bus
