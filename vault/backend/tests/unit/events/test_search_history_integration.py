"""Integration tests for SearchHistoryService event emission."""

from unittest.mock import AsyncMock, MagicMock

import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from src.events.domain.search_events import SearchQueryRecorded, SearchQueryRecordingFailed
from src.services.search_history import SearchHistoryService


class TestSearchHistoryServiceEventIntegration:
    """Test SearchHistoryService integration with EventBus."""

    @pytest.fixture
    def mock_event_bus(self) -> AsyncMock:
        """Create a mock EventBus."""
        event_bus = AsyncMock()
        event_bus.emit = AsyncMock()
        return event_bus

    @pytest.fixture
    def mock_db_session(self) -> MagicMock:
        """Create a mock database session."""
        session = MagicMock(spec=AsyncSession)
        session.execute = AsyncMock()
        session.commit = AsyncMock()
        return session

    @pytest.mark.asyncio
    async def test_record_search_emits_success_event(
        self, mock_db_session: MagicMock, mock_event_bus: AsyncMock
    ) -> None:
        """Test that successful search recording emits SearchQueryRecorded event."""
        service = SearchHistoryService(db=mock_db_session, event_bus=mock_event_bus)

        await service.record_search(
            query="machine learning",
            search_type="hybrid",
            result_count=15,
            execution_time_ms=125.5,
            user_id="user123",
        )

        # Verify database operations occurred
        mock_db_session.execute.assert_called_once()
        mock_db_session.commit.assert_called_once()

        # Verify event was emitted
        mock_event_bus.emit.assert_called_once()
        emitted_event = mock_event_bus.emit.call_args[0][0]

        assert isinstance(emitted_event, SearchQueryRecorded)
        assert emitted_event.query == "machine learning"
        assert emitted_event.search_type == "hybrid"
        assert emitted_event.result_count == 15
        assert emitted_event.execution_time_ms == 125.5
        assert emitted_event.user_id == "user123"

    @pytest.mark.asyncio
    async def test_record_search_emits_failure_event_on_error(
        self, mock_db_session: MagicMock, mock_event_bus: AsyncMock
    ) -> None:
        """Test that failed search recording emits SearchQueryRecordingFailed event."""
        # Configure mock to raise exception
        mock_db_session.execute.side_effect = Exception("Database connection timeout")

        service = SearchHistoryService(db=mock_db_session, event_bus=mock_event_bus)

        await service.record_search(
            query="test query",
            search_type="semantic",
            result_count=10,
            execution_time_ms=50.0,
            user_id="user456",
        )

        # Verify event was emitted
        mock_event_bus.emit.assert_called_once()
        emitted_event = mock_event_bus.emit.call_args[0][0]

        assert isinstance(emitted_event, SearchQueryRecordingFailed)
        assert emitted_event.query == "test query"
        assert emitted_event.search_type == "semantic"
        assert emitted_event.result_count == 10
        assert emitted_event.execution_time_ms == 50.0
        assert emitted_event.user_id == "user456"
        assert "Database connection timeout" in emitted_event.error

    @pytest.mark.asyncio
    async def test_record_search_without_event_bus(self, mock_db_session: MagicMock) -> None:
        """Test that service works without event_bus (backward compatibility)."""
        service = SearchHistoryService(db=mock_db_session, event_bus=None)

        # Should not raise exception
        await service.record_search(
            query="test",
            search_type="hybrid",
            result_count=5,
            execution_time_ms=100.0,
        )

        # Verify database operations occurred
        mock_db_session.execute.assert_called_once()
        mock_db_session.commit.assert_called_once()

    @pytest.mark.asyncio
    async def test_record_search_event_emission_errors_caught(
        self, mock_db_session: MagicMock, mock_event_bus: AsyncMock
    ) -> None:
        """Test that event emission errors are caught and don't break the service.

        Note: Event emission is awaited, so errors will propagate. This test verifies
        that the database commit happens before event emission.
        """
        # Configure event bus to raise exception ONLY on success event
        # This way we can test that commit happened before the error
        call_count = [0]

        async def emit_side_effect(event):
            call_count[0] += 1
            if call_count[0] == 1:  # First call (success event)
                raise Exception("Event bus error")

        mock_event_bus.emit.side_effect = emit_side_effect

        service = SearchHistoryService(db=mock_db_session, event_bus=mock_event_bus)

        # Should raise exception from event bus
        with pytest.raises(Exception, match="Event bus error"):
            await service.record_search(
                query="test",
                search_type="hybrid",
                result_count=5,
                execution_time_ms=100.0,
            )

        # Verify database operations occurred BEFORE event emission
        mock_db_session.execute.assert_called_once()
        mock_db_session.commit.assert_called_once()

    @pytest.mark.asyncio
    async def test_record_search_uses_default_user_id(
        self, mock_db_session: MagicMock, mock_event_bus: AsyncMock
    ) -> None:
        """Test that default user_id is used when not provided."""
        service = SearchHistoryService(db=mock_db_session, event_bus=mock_event_bus)

        await service.record_search(
            query="test",
            search_type="hybrid",
            result_count=5,
            execution_time_ms=100.0,
            # user_id not provided
        )

        # Verify event was emitted with default user_id
        mock_event_bus.emit.assert_called_once()
        emitted_event = mock_event_bus.emit.call_args[0][0]

        assert isinstance(emitted_event, SearchQueryRecorded)
        assert emitted_event.user_id == "default"

    @pytest.mark.asyncio
    async def test_record_search_all_search_types(
        self, mock_db_session: MagicMock, mock_event_bus: AsyncMock
    ) -> None:
        """Test that all valid search types emit correct events."""
        service = SearchHistoryService(db=mock_db_session, event_bus=mock_event_bus)

        for search_type in ["semantic", "keyword", "hybrid"]:
            mock_event_bus.reset_mock()

            await service.record_search(
                query="test",
                search_type=search_type,
                result_count=5,
                execution_time_ms=100.0,
            )

            emitted_event = mock_event_bus.emit.call_args[0][0]
            assert emitted_event.search_type == search_type

    @pytest.mark.asyncio
    async def test_get_recent_queries_does_not_emit_events(
        self, mock_db_session: MagicMock, mock_event_bus: AsyncMock
    ) -> None:
        """Test that read-only operations don't emit events."""
        # Configure mock properly - execute returns a result object with fetchall
        mock_result = MagicMock()
        mock_result.fetchall.return_value = [
            ("query1",),
            ("query2",),
        ]
        mock_db_session.execute.return_value = mock_result

        service = SearchHistoryService(db=mock_db_session, event_bus=mock_event_bus)

        await service.get_recent_queries(user_id="user123", limit=5)

        # Verify no events were emitted
        mock_event_bus.emit.assert_not_called()

    @pytest.mark.asyncio
    async def test_get_popular_queries_does_not_emit_events(
        self, mock_db_session: MagicMock, mock_event_bus: AsyncMock
    ) -> None:
        """Test that read-only operations don't emit events."""
        # Configure mock properly - execute returns a result object with fetchall
        mock_result = MagicMock()
        mock_result.fetchall.return_value = [
            ("query1", 10),
            ("query2", 5),
        ]
        mock_db_session.execute.return_value = mock_result

        service = SearchHistoryService(db=mock_db_session, event_bus=mock_event_bus)

        await service.get_popular_queries(user_id="user123", limit=5, days=30)

        # Verify no events were emitted
        mock_event_bus.emit.assert_not_called()
