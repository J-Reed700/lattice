"""Integration tests for UploadService event emission."""

from __future__ import annotations

import pytest
from unittest.mock import AsyncMock, patch

from src.events.bus import EventBus
from src.events.domain.upload_events import (
    BatchUploadCompleted,
    FileUploadCompleted,
    FileUploadFailed,
    FileUploadProgressUpdated,
    FileUploadStarted,
)
from src.services.upload_service import UploadService


class TestUploadServiceEventIntegration:
    """Integration tests for UploadService event emission."""

    @pytest.fixture
    def mock_event_bus(self):
        """Create a mock EventBus."""
        bus = AsyncMock(spec=EventBus)
        bus.publish = AsyncMock()
        return bus

    @pytest.fixture
    def upload_service_with_events(self, mock_event_bus, tmp_path):
        """Create UploadService with event bus."""
        upload_dir = tmp_path / "uploads"
        return UploadService(upload_dir=str(upload_dir), event_bus=mock_event_bus)

    @pytest.fixture
    def upload_service_without_events(self, tmp_path):
        """Create UploadService without event bus (backward compatibility)."""
        upload_dir = tmp_path / "uploads"
        return UploadService(upload_dir=str(upload_dir))

    @pytest.mark.asyncio
    async def test_save_upload_emits_started_and_completed_events(
        self, upload_service_with_events, mock_event_bus
    ):
        """Test that save_upload emits FileUploadStarted and FileUploadCompleted events."""
        filename = "test_document.pdf"
        content = b"Test file content"
        user_id = 123
        file_id = "test-file-id"

        result_path = await upload_service_with_events.save_upload(
            filename=filename, content=content, user_id=user_id, file_id=file_id
        )

        # Verify file was saved
        assert result_path.exists()

        # Verify events were published
        assert mock_event_bus.publish.call_count == 2
        call_args = mock_event_bus.publish.call_args_list

        # First event should be FileUploadStarted
        started_event = call_args[0][0][0]
        assert isinstance(started_event, FileUploadStarted)
        assert started_event.file_id == file_id
        assert started_event.filename == filename
        assert started_event.size_bytes == len(content)
        assert started_event.user_id == user_id

        # Second event should be FileUploadCompleted
        completed_event = call_args[1][0][0]
        assert isinstance(completed_event, FileUploadCompleted)
        assert completed_event.file_id == file_id
        assert completed_event.filename == filename
        assert completed_event.size_bytes == len(content)
        assert completed_event.storage_path == str(result_path)
        assert completed_event.user_id == user_id
        assert completed_event.duration_seconds >= 0

    @pytest.mark.asyncio
    async def test_save_upload_emits_failed_event_on_error(
        self, upload_service_with_events, mock_event_bus
    ):
        """Test that save_upload emits FileUploadFailed event on error."""
        filename = "test.pdf"
        content = b"content"
        user_id = 456
        file_id = "error-file-id"

        # Mock open to raise exception
        with patch("builtins.open", side_effect=IOError("Disk full")):
            with pytest.raises(IOError, match="Disk full"):
                await upload_service_with_events.save_upload(
                    filename=filename, content=content, user_id=user_id, file_id=file_id
                )

            # Verify events were published
            assert mock_event_bus.publish.call_count == 2
            call_args = mock_event_bus.publish.call_args_list

            # First event should be FileUploadStarted
            started_event = call_args[0][0][0]
            assert isinstance(started_event, FileUploadStarted)

            # Second event should be FileUploadFailed
            failed_event = call_args[1][0][0]
            assert isinstance(failed_event, FileUploadFailed)
            assert failed_event.file_id == file_id
            assert failed_event.filename == filename
            assert "Disk full" in failed_event.error_message
            assert failed_event.error_type == "IOError"
            assert failed_event.user_id == user_id

    @pytest.mark.asyncio
    async def test_process_uploads_emits_progress_and_batch_events(
        self, upload_service_with_events, mock_event_bus, tmp_path
    ):
        """Test that process_uploads emits progress and batch completion events."""
        # Create test files
        file1 = tmp_path / "file1.txt"
        file2 = tmp_path / "file2.txt"
        file1.write_text("content 1")
        file2.write_text("content 2")

        file_paths = [file1, file2]
        task_id = upload_service_with_events.create_task(len(file_paths))
        user_id = 789

        # Mock indexing service
        mock_indexing_service = AsyncMock()
        mock_indexing_service.index_file = AsyncMock(return_value=(True, None))

        await upload_service_with_events.process_uploads(
            task_id=task_id,
            file_paths=file_paths,
            indexing_service=mock_indexing_service,
            user_id=user_id,
        )

        # Verify events were published
        assert mock_event_bus.publish.called
        call_args = mock_event_bus.publish.call_args_list

        # Check for FileUploadProgressUpdated events (2 files)
        progress_events = [
            call[0][0]
            for call in call_args
            if isinstance(call[0][0], FileUploadProgressUpdated)
        ]
        assert len(progress_events) == 2

        # Verify first progress event
        assert progress_events[0].user_id == user_id
        assert 0 <= progress_events[0].percentage <= 100

        # Check for BatchUploadCompleted event
        batch_events = [
            call[0][0] for call in call_args if isinstance(call[0][0], BatchUploadCompleted)
        ]
        assert len(batch_events) == 1

        batch_event = batch_events[0]
        assert batch_event.batch_id == task_id
        assert batch_event.total_files == 2
        assert batch_event.successful_uploads == 2
        assert batch_event.failed_uploads == 0
        assert batch_event.user_id == user_id
        assert batch_event.duration_seconds >= 0

    @pytest.mark.asyncio
    async def test_process_uploads_handles_partial_failures(
        self, upload_service_with_events, mock_event_bus, tmp_path
    ):
        """Test that process_uploads correctly tracks partial failures."""
        # Create test files
        file1 = tmp_path / "file1.txt"
        file2 = tmp_path / "file2.txt"
        file3 = tmp_path / "file3.txt"
        file1.write_text("content 1")
        file2.write_text("content 2")
        file3.write_text("content 3")

        file_paths = [file1, file2, file3]
        task_id = upload_service_with_events.create_task(len(file_paths))
        user_id = 999

        # Mock indexing service - first succeeds, second fails, third succeeds
        mock_indexing_service = AsyncMock()
        mock_indexing_service.index_file = AsyncMock(
            side_effect=[
                (True, None),  # Success
                (False, "Index error"),  # Failure
                (True, None),  # Success
            ]
        )

        await upload_service_with_events.process_uploads(
            task_id=task_id,
            file_paths=file_paths,
            indexing_service=mock_indexing_service,
            user_id=user_id,
        )

        # Check for BatchUploadCompleted event
        call_args = mock_event_bus.publish.call_args_list
        batch_events = [
            call[0][0] for call in call_args if isinstance(call[0][0], BatchUploadCompleted)
        ]
        assert len(batch_events) == 1

        batch_event = batch_events[0]
        assert batch_event.total_files == 3
        assert batch_event.successful_uploads == 2
        assert batch_event.failed_uploads == 1

    @pytest.mark.asyncio
    async def test_backward_compatibility_without_event_bus(
        self, upload_service_without_events
    ):
        """Test that UploadService works without event bus (backward compatibility)."""
        filename = "test.pdf"
        content = b"test content"

        # Should not raise error even without event bus
        result_path = await upload_service_without_events.save_upload(
            filename=filename, content=content
        )

        # Verify file was saved
        assert result_path.exists()

    @pytest.mark.asyncio
    async def test_process_uploads_without_event_bus(
        self, upload_service_without_events, tmp_path
    ):
        """Test that process_uploads works without event bus."""
        # Create test files
        file1 = tmp_path / "file1.txt"
        file1.write_text("content")

        task_id = upload_service_without_events.create_task(1)

        # Mock indexing service
        mock_indexing_service = AsyncMock()
        mock_indexing_service.index_file = AsyncMock(return_value=(True, None))

        # Should not raise error even without event bus
        await upload_service_without_events.process_uploads(
            task_id=task_id,
            file_paths=[file1],
            indexing_service=mock_indexing_service,
        )

        # Verify progress was tracked
        progress = upload_service_without_events.get_progress(task_id)
        assert progress["indexed_files"] == 1

    @pytest.mark.asyncio
    async def test_file_upload_generates_unique_file_id(
        self, upload_service_with_events, mock_event_bus
    ):
        """Test that file upload generates unique file_id if not provided."""
        filename = "test.pdf"
        content = b"content"

        await upload_service_with_events.save_upload(filename=filename, content=content)

        # Verify FileUploadStarted event has a file_id
        call_args = mock_event_bus.publish.call_args_list
        started_event = call_args[0][0][0]
        assert isinstance(started_event, FileUploadStarted)
        assert started_event.file_id is not None
        assert len(started_event.file_id) > 0

    @pytest.mark.asyncio
    async def test_batch_upload_all_failures(
        self, upload_service_with_events, mock_event_bus, tmp_path
    ):
        """Test batch upload with all failures."""
        # Create test files
        file1 = tmp_path / "file1.txt"
        file2 = tmp_path / "file2.txt"
        file1.write_text("content 1")
        file2.write_text("content 2")

        task_id = upload_service_with_events.create_task(2)
        user_id = 111

        # Mock indexing service to always fail
        mock_indexing_service = AsyncMock()
        mock_indexing_service.index_file = AsyncMock(
            return_value=(False, "Always fails")
        )

        await upload_service_with_events.process_uploads(
            task_id=task_id,
            file_paths=[file1, file2],
            indexing_service=mock_indexing_service,
            user_id=user_id,
        )

        # Check BatchUploadCompleted event
        call_args = mock_event_bus.publish.call_args_list
        batch_events = [
            call[0][0] for call in call_args if isinstance(call[0][0], BatchUploadCompleted)
        ]
        assert len(batch_events) == 1

        batch_event = batch_events[0]
        assert batch_event.total_files == 2
        assert batch_event.successful_uploads == 0
        assert batch_event.failed_uploads == 2
