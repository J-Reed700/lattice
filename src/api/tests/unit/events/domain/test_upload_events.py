"""Unit tests for upload events."""

from __future__ import annotations

from src.events.domain.upload_events import (
    BatchUploadCompleted,
    FileUploadCompleted,
    FileUploadFailed,
    FileUploadProgressUpdated,
    FileUploadStarted,
)


class TestFileUploadStarted:
    """Tests for FileUploadStarted event."""

    def test_create_event(self):
        """Test creating a valid FileUploadStarted event."""
        event = FileUploadStarted(
            file_id="abc123", filename="document.pdf", size_bytes=1024, user_id=123
        )

        assert event.event_type == "upload.file.started"
        assert event.data["file_id"] == "abc123"
        assert event.data["filename"] == "document.pdf"
        assert event.data["size_bytes"] == 1024
        assert event.data["user_id"] == 123

    def test_event_with_zero_size(self):
        """Test creating event with zero size."""
        event = FileUploadStarted(
            file_id="abc123", filename="empty.txt", size_bytes=0, user_id=123
        )

        assert event.data["size_bytes"] == 0


class TestFileUploadProgressUpdated:
    """Tests for FileUploadProgressUpdated event."""

    def test_create_event(self):
        """Test creating a valid FileUploadProgressUpdated event."""
        event = FileUploadProgressUpdated(
            file_id="abc123",
            bytes_uploaded=512,
            total_bytes=1024,
            percentage=50.0,
            user_id=123,
        )

        assert event.event_type == "upload.file.progress"
        assert event.data["file_id"] == "abc123"
        assert event.data["bytes_uploaded"] == 512
        assert event.data["total_bytes"] == 1024
        assert event.data["percentage"] == 50.0
        assert event.data["user_id"] == 123

    def test_percentage_boundaries(self):
        """Test that 0 and 100 percentage work."""
        event_0 = FileUploadProgressUpdated(
            file_id="abc123", bytes_uploaded=0, total_bytes=1024, percentage=0.0, user_id=123
        )
        assert event_0.data["percentage"] == 0.0

        event_100 = FileUploadProgressUpdated(
            file_id="abc123", bytes_uploaded=1024, total_bytes=1024, percentage=100.0, user_id=123
        )
        assert event_100.data["percentage"] == 100.0


class TestFileUploadCompleted:
    """Tests for FileUploadCompleted event."""

    def test_create_event(self):
        """Test creating a valid FileUploadCompleted event."""
        event = FileUploadCompleted(
            file_id="abc123",
            filename="document.pdf",
            size_bytes=1024,
            storage_path="/uploads/document.pdf",
            user_id=123,
            duration_seconds=1.5,
        )

        assert event.event_type == "upload.file.completed"
        assert event.data["file_id"] == "abc123"
        assert event.data["filename"] == "document.pdf"
        assert event.data["size_bytes"] == 1024
        assert event.data["storage_path"] == "/uploads/document.pdf"
        assert event.data["user_id"] == 123
        assert event.data["duration_seconds"] == 1.5

    def test_zero_duration(self):
        """Test that zero duration is valid."""
        event = FileUploadCompleted(
            file_id="abc123",
            filename="test.pdf",
            size_bytes=1024,
            storage_path="/uploads/test.pdf",
            user_id=123,
            duration_seconds=0.0,
        )

        assert event.data["duration_seconds"] == 0.0


class TestFileUploadFailed:
    """Tests for FileUploadFailed event."""

    def test_create_event(self):
        """Test creating a valid FileUploadFailed event."""
        event = FileUploadFailed(
            file_id="abc123",
            filename="document.pdf",
            error_message="Network timeout",
            error_type="TimeoutError",
            user_id=123,
        )

        assert event.event_type == "upload.file.failed"
        assert event.data["file_id"] == "abc123"
        assert event.data["filename"] == "document.pdf"
        assert event.data["error_message"] == "Network timeout"
        assert event.data["error_type"] == "TimeoutError"
        assert event.data["user_id"] == 123

    def test_event_with_stack_trace(self):
        """Test creating event with stack trace in metadata."""
        metadata = {"stack_trace": "Full traceback..."}
        event = FileUploadFailed(
            file_id="abc123",
            filename="test.pdf",
            error_message="Error",
            error_type="Error",
            user_id=123,
            metadata=metadata,
        )

        assert event.metadata["stack_trace"] == "Full traceback..."


class TestBatchUploadCompleted:
    """Tests for BatchUploadCompleted event."""

    def test_create_event(self):
        """Test creating a valid BatchUploadCompleted event."""
        event = BatchUploadCompleted(
            batch_id="batch123",
            total_files=10,
            successful_uploads=7,
            failed_uploads=3,
            user_id=123,
            duration_seconds=30.5,
        )

        assert event.event_type == "upload.batch.completed"
        assert event.data["batch_id"] == "batch123"
        assert event.data["total_files"] == 10
        assert event.data["successful_uploads"] == 7
        assert event.data["failed_uploads"] == 3
        assert event.data["user_id"] == 123
        assert event.data["duration_seconds"] == 30.5

    def test_all_successful(self):
        """Test batch with all successful uploads."""
        event = BatchUploadCompleted(
            batch_id="batch123",
            total_files=10,
            successful_uploads=10,
            failed_uploads=0,
            user_id=123,
            duration_seconds=15.0,
        )

        assert event.data["successful_uploads"] == 10
        assert event.data["failed_uploads"] == 0

    def test_all_failed(self):
        """Test batch with all failed uploads."""
        event = BatchUploadCompleted(
            batch_id="batch123",
            total_files=10,
            successful_uploads=0,
            failed_uploads=10,
            user_id=123,
            duration_seconds=5.0,
        )

        assert event.data["successful_uploads"] == 0
        assert event.data["failed_uploads"] == 10

    def test_event_serialization(self):
        """Test event can be converted to dict."""
        event = BatchUploadCompleted(
            batch_id="batch123",
            total_files=5,
            successful_uploads=3,
            failed_uploads=2,
            user_id=123,
            duration_seconds=10.0,
        )

        event_dict = event.model_dump()

        assert event_dict["event_type"] == "upload.batch.completed"
        assert event_dict["data"]["batch_id"] == "batch123"
        assert event_dict["data"]["total_files"] == 5
