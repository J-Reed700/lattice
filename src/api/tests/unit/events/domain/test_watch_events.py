"""Unit tests for watch events."""

from __future__ import annotations

from src.events.domain.watch_events import (
    DirectoryWatchStarted,
    DirectoryWatchStopped,
    FileChangeDetected,
    WatchErrorOccurred,
)


class TestDirectoryWatchStarted:
    """Tests for DirectoryWatchStarted event."""

    def test_create_event(self):
        """Test creating a valid DirectoryWatchStarted event."""
        event = DirectoryWatchStarted(
            directory_path="/path/to/watch", user_id=123, recursive=True
        )

        assert event.event_type == "watch.directory.started"
        assert event.data["directory_path"] == "/path/to/watch"
        assert event.data["user_id"] == 123
        assert event.data["recursive"] is True
        assert event.event_id is not None
        assert event.timestamp is not None

    def test_event_with_metadata(self):
        """Test creating event with metadata."""
        metadata = {"source": "api", "request_id": "abc123"}
        event = DirectoryWatchStarted(
            directory_path="/path/to/watch",
            user_id=123,
            recursive=False,
            metadata=metadata,
        )

        assert event.metadata == metadata
        assert event.metadata["source"] == "api"

    def test_multiple_events_have_different_ids(self):
        """Test that multiple events get unique IDs."""
        event1 = DirectoryWatchStarted(
            directory_path="/path1", user_id=1, recursive=True
        )
        event2 = DirectoryWatchStarted(
            directory_path="/path2", user_id=2, recursive=True
        )

        assert event1.event_id != event2.event_id


class TestDirectoryWatchStopped:
    """Tests for DirectoryWatchStopped event."""

    def test_create_event(self):
        """Test creating a valid DirectoryWatchStopped event."""
        event = DirectoryWatchStopped(
            directory_path="/path/to/watch", user_id=123, reason="user_request"
        )

        assert event.event_type == "watch.directory.stopped"
        assert event.data["directory_path"] == "/path/to/watch"
        assert event.data["user_id"] == 123
        assert event.data["reason"] == "user_request"

    def test_various_stop_reasons(self):
        """Test different stop reasons."""
        reasons = ["user_request", "error", "shutdown"]

        for reason in reasons:
            event = DirectoryWatchStopped(
                directory_path="/path", user_id=123, reason=reason
            )
            assert event.data["reason"] == reason


class TestFileChangeDetected:
    """Tests for FileChangeDetected event."""

    def test_create_event_created(self):
        """Test creating a FileChangeDetected event for file creation."""
        event = FileChangeDetected(
            file_path="/path/to/file.txt",
            change_type="created",
            directory_path="/path/to",
            user_id=123,
        )

        assert event.event_type == "watch.file.changed"
        assert event.data["file_path"] == "/path/to/file.txt"
        assert event.data["change_type"] == "created"
        assert event.data["directory_path"] == "/path/to"
        assert event.data["user_id"] == 123
        assert event.timestamp is not None

    def test_create_event_modified(self):
        """Test creating a FileChangeDetected event for file modification."""
        event = FileChangeDetected(
            file_path="/path/to/file.txt",
            change_type="modified",
            directory_path="/path/to",
            user_id=123,
        )

        assert event.data["change_type"] == "modified"

    def test_create_event_deleted(self):
        """Test creating a FileChangeDetected event for file deletion."""
        event = FileChangeDetected(
            file_path="/path/to/file.txt",
            change_type="deleted",
            directory_path="/path/to",
            user_id=123,
        )

        assert event.data["change_type"] == "deleted"

    def test_event_is_immutable(self):
        """Test that events are immutable (frozen)."""
        event = FileChangeDetected(
            file_path="/path/to/file.txt",
            change_type="created",
            directory_path="/path/to",
            user_id=123,
        )

        # Attempt to modify should raise error (frozen=True in Config)
        try:
            event.event_type = "something.else"  # type: ignore
            assert False, "Should have raised validation error"
        except (AttributeError, ValueError):
            pass  # Expected


class TestWatchErrorOccurred:
    """Tests for WatchErrorOccurred event."""

    def test_create_event(self):
        """Test creating a valid WatchErrorOccurred event."""
        event = WatchErrorOccurred(
            directory_path="/path/to/watch",
            error_message="Permission denied",
            error_type="PermissionError",
            user_id=123,
        )

        assert event.event_type == "watch.error"
        assert event.data["directory_path"] == "/path/to/watch"
        assert event.data["error_message"] == "Permission denied"
        assert event.data["error_type"] == "PermissionError"
        assert event.data["user_id"] == 123

    def test_event_with_stack_trace(self):
        """Test creating event with stack trace in metadata."""
        metadata = {"stack_trace": "Traceback (most recent call last)..."}
        event = WatchErrorOccurred(
            directory_path="/path/to/watch",
            error_message="File not found",
            error_type="FileNotFoundError",
            user_id=123,
            metadata=metadata,
        )

        assert event.metadata["stack_trace"] == "Traceback (most recent call last)..."

    def test_event_serialization(self):
        """Test event can be converted to dict."""
        event = WatchErrorOccurred(
            directory_path="/path/to/watch",
            error_message="Error occurred",
            error_type="IOError",
            user_id=123,
        )

        event_dict = event.model_dump()

        assert event_dict["event_type"] == "watch.error"
        assert event_dict["data"]["directory_path"] == "/path/to/watch"
        assert event_dict["data"]["error_message"] == "Error occurred"
