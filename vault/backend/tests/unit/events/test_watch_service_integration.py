"""Integration tests for WatchService event emission."""

from __future__ import annotations

import pytest
from unittest.mock import AsyncMock, MagicMock, patch
from uuid import uuid4

from src.events.bus import EventBus
from src.events.domain.watch_events import (
    DirectoryWatchStarted,
    DirectoryWatchStopped,
    FileChangeDetected,
    WatchErrorOccurred,
)
from src.services.watch import WatchService


class TestWatchServiceEventIntegration:
    """Integration tests for WatchService event emission."""

    @pytest.fixture
    def mock_event_bus(self):
        """Create a mock EventBus."""
        bus = AsyncMock(spec=EventBus)
        bus.publish = AsyncMock()
        return bus

    @pytest.fixture
    def watch_service_with_events(self, mock_event_bus):
        """Create WatchService with event bus."""
        return WatchService(event_bus=mock_event_bus)

    @pytest.fixture
    def watch_service_without_events(self):
        """Create WatchService without event bus (backward compatibility)."""
        return WatchService()

    @pytest.mark.asyncio
    async def test_start_watching_emits_started_event(
        self, watch_service_with_events, mock_event_bus, tmp_path
    ):
        """Test that start_watching emits DirectoryWatchStarted event."""
        watch_folder_id = uuid4()
        directory_path = str(tmp_path)

        # Mock database session
        mock_session = AsyncMock()
        mock_session.execute = AsyncMock()
        mock_session.commit = AsyncMock()

        # Mock WatchFolder query result
        mock_result = MagicMock()
        mock_watch_folder = MagicMock()
        mock_watch_folder.electric_user_id = "123"
        mock_result.scalar_one_or_none.return_value = mock_watch_folder
        mock_session.execute.return_value = mock_result

        # Mock FileWatcher
        with patch("src.services.watch.FileWatcher") as mock_watcher_cls:
            mock_watcher = AsyncMock()
            mock_watcher.start = AsyncMock()
            mock_watcher.scan_directory = AsyncMock()
            mock_watcher_cls.return_value = mock_watcher

            await watch_service_with_events.start_watching(
                watch_folder_id=watch_folder_id,
                path=directory_path,
                recursive=True,
                db_session=mock_session,
            )

            # Verify DirectoryWatchStarted event was published
            assert mock_event_bus.publish.called
            call_args = mock_event_bus.publish.call_args_list

            # Find the DirectoryWatchStarted event
            started_events = [
                call[0][0]
                for call in call_args
                if isinstance(call[0][0], DirectoryWatchStarted)
            ]
            assert len(started_events) == 1

            event = started_events[0]
            assert event.directory_path == directory_path
            assert event.user_id == 123
            assert event.recursive is True

    @pytest.mark.asyncio
    async def test_start_watching_emits_error_event_on_failure(
        self, watch_service_with_events, mock_event_bus, tmp_path
    ):
        """Test that start_watching emits WatchErrorOccurred on failure."""
        watch_folder_id = uuid4()
        directory_path = str(tmp_path)

        # Mock database session
        mock_session = AsyncMock()
        mock_session.execute = AsyncMock()

        # Mock WatchFolder query result
        mock_result = MagicMock()
        mock_watch_folder = MagicMock()
        mock_watch_folder.electric_user_id = "456"
        mock_result.scalar_one_or_none.return_value = mock_watch_folder
        mock_session.execute.return_value = mock_result

        # Mock FileWatcher to raise exception
        with patch("src.services.watch.FileWatcher") as mock_watcher_cls:
            mock_watcher_cls.side_effect = Exception("Failed to start watcher")

            with pytest.raises(Exception, match="Failed to start watcher"):
                await watch_service_with_events.start_watching(
                    watch_folder_id=watch_folder_id,
                    path=directory_path,
                    recursive=True,
                    db_session=mock_session,
                )

            # Verify WatchErrorOccurred event was published
            assert mock_event_bus.publish.called
            call_args = mock_event_bus.publish.call_args_list

            error_events = [
                call[0][0] for call in call_args if isinstance(call[0][0], WatchErrorOccurred)
            ]
            assert len(error_events) == 1

            event = error_events[0]
            assert event.directory_path == directory_path
            assert event.user_id == 456
            assert "Failed to start watcher" in event.error_message
            assert event.error_type == "Exception"

    @pytest.mark.asyncio
    async def test_stop_watching_emits_stopped_event(
        self, watch_service_with_events, mock_event_bus
    ):
        """Test that stop_watching emits DirectoryWatchStopped event."""
        watch_folder_id = uuid4()

        # Setup watcher metadata
        watch_service_with_events.watcher_metadata[watch_folder_id] = {
            "path": "/test/path",
            "user_id": 789,
        }

        # Mock watcher
        mock_watcher = AsyncMock()
        mock_watcher.stop = AsyncMock()
        watch_service_with_events.watchers[watch_folder_id] = mock_watcher

        await watch_service_with_events.stop_watching(
            watch_folder_id, reason="user_request"
        )

        # Verify DirectoryWatchStopped event was published
        assert mock_event_bus.publish.called
        call_args = mock_event_bus.publish.call_args_list

        stopped_events = [
            call[0][0] for call in call_args if isinstance(call[0][0], DirectoryWatchStopped)
        ]
        assert len(stopped_events) == 1

        event = stopped_events[0]
        assert event.directory_path == "/test/path"
        assert event.user_id == 789
        assert event.reason == "user_request"

    @pytest.mark.asyncio
    async def test_stop_all_emits_stopped_events_with_shutdown_reason(
        self, watch_service_with_events, mock_event_bus
    ):
        """Test that stop_all emits DirectoryWatchStopped with shutdown reason."""
        watch_folder_id_1 = uuid4()
        watch_folder_id_2 = uuid4()

        # Setup watcher metadata
        watch_service_with_events.watcher_metadata[watch_folder_id_1] = {
            "path": "/path1",
            "user_id": 100,
        }
        watch_service_with_events.watcher_metadata[watch_folder_id_2] = {
            "path": "/path2",
            "user_id": 200,
        }

        # Mock watchers
        mock_watcher_1 = AsyncMock()
        mock_watcher_1.stop = AsyncMock()
        mock_watcher_2 = AsyncMock()
        mock_watcher_2.stop = AsyncMock()
        watch_service_with_events.watchers[watch_folder_id_1] = mock_watcher_1
        watch_service_with_events.watchers[watch_folder_id_2] = mock_watcher_2

        await watch_service_with_events.stop_all()

        # Verify DirectoryWatchStopped events were published
        assert mock_event_bus.publish.called
        call_args = mock_event_bus.publish.call_args_list

        stopped_events = [
            call[0][0] for call in call_args if isinstance(call[0][0], DirectoryWatchStopped)
        ]
        assert len(stopped_events) == 2

        # All events should have "shutdown" reason
        for event in stopped_events:
            assert event.reason == "shutdown"

    @pytest.mark.asyncio
    async def test_backward_compatibility_without_event_bus(
        self, watch_service_without_events, tmp_path
    ):
        """Test that WatchService works without event bus (backward compatibility)."""
        watch_folder_id = uuid4()
        directory_path = str(tmp_path)

        # Mock database session
        mock_session = AsyncMock()
        mock_session.execute = AsyncMock()
        mock_session.commit = AsyncMock()

        # Mock WatchFolder query result
        mock_result = MagicMock()
        mock_watch_folder = MagicMock()
        mock_watch_folder.electric_user_id = "123"
        mock_result.scalar_one_or_none.return_value = mock_watch_folder
        mock_session.execute.return_value = mock_result

        # Mock FileWatcher
        with patch("src.services.watch.FileWatcher") as mock_watcher_cls:
            mock_watcher = AsyncMock()
            mock_watcher.start = AsyncMock()
            mock_watcher.scan_directory = AsyncMock()
            mock_watcher_cls.return_value = mock_watcher

            # Should not raise error even without event bus
            await watch_service_without_events.start_watching(
                watch_folder_id=watch_folder_id,
                path=directory_path,
                recursive=True,
                db_session=mock_session,
            )

            # Verify watcher was created
            assert watch_folder_id in watch_service_without_events.watchers

    @pytest.mark.asyncio
    async def test_file_change_event_emission(
        self, watch_service_with_events, mock_event_bus
    ):
        """Test that file change events are emitted."""
        watch_folder_id = uuid4()

        # Setup watcher metadata
        watch_service_with_events.watcher_metadata[watch_folder_id] = {
            "path": "/test/path",
            "user_id": 999,
        }

        # Mock session factory
        mock_session = AsyncMock()
        mock_session.execute = AsyncMock()
        mock_session.commit = AsyncMock()
        mock_session.rollback = AsyncMock()

        async def mock_session_factory():
            return mock_session

        watch_service_with_events._session_factory = mock_session_factory

        # Mock file event
        from pathlib import Path
        from src.modules.file_watcher import FileEvent, FileEventType

        test_file_path = Path("/test/path/document.txt")
        file_event = FileEvent(
            event_type=FileEventType.CREATED,
            file_path=test_file_path,
            file_hash="abc123",
        )

        # Mock _should_process_file to return True
        watch_service_with_events._should_process_file = MagicMock(return_value=True)

        # Mock _handle_created to not do database operations
        with patch.object(
            watch_service_with_events, "_handle_created", new=AsyncMock()
        ):
            await watch_service_with_events._handle_file_event(file_event, watch_folder_id)

            # Verify FileChangeDetected event was published
            assert mock_event_bus.publish.called
            call_args = mock_event_bus.publish.call_args_list

            change_events = [
                call[0][0] for call in call_args if isinstance(call[0][0], FileChangeDetected)
            ]
            assert len(change_events) == 1

            event = change_events[0]
            assert event.file_path == str(test_file_path)
            assert event.change_type == "created"
            assert event.directory_path == "/test/path"
            assert event.user_id == 999
