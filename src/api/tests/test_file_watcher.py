import asyncio
from datetime import datetime
from pathlib import Path
from unittest.mock import AsyncMock

import pytest

from src.modules.file_watcher import (
    EventPriority,
    FileEvent,
    FileEventType,
    FileWatcher,
    PathFilter,
)
from src.modules.file_watcher.event_queue import DebouncedEventQueue
from src.modules.file_watcher.worker_pool import FileProcessorWorkerPool


class TestPathFilter:
    """Test PathFilter functionality."""

    def test_default_ignore_patterns(self):
        filter = PathFilter()
        assert ".git" in filter.ignore_patterns
        assert "node_modules" in filter.ignore_patterns
        assert "__pycache__" in filter.ignore_patterns

    def test_custom_ignore_patterns(self):
        custom_patterns = ["*.tmp", "*.log"]
        filter = PathFilter(ignore_patterns=custom_patterns)
        assert filter.ignore_patterns == custom_patterns

    def test_should_process_valid_file(self, tmp_path):
        filter = PathFilter()
        test_file = tmp_path / "document.txt"
        test_file.write_text("content")
        assert filter.should_process(test_file) is True

    def test_should_not_process_ignored_file(self, tmp_path):
        filter = PathFilter(ignore_patterns=["*.tmp"])
        test_file = tmp_path / "temp.tmp"
        test_file.write_text("content")
        assert filter.should_process(test_file) is False

    def test_should_not_process_ignored_directory(self, tmp_path):
        filter = PathFilter()
        git_dir = tmp_path / ".git"
        git_dir.mkdir()
        test_file = git_dir / "config"
        test_file.write_text("content")
        assert filter.should_process(test_file) is False

    def test_should_not_process_nonexistent_file(self):
        filter = PathFilter()
        assert filter.should_process(Path("/nonexistent/file.txt")) is False

    def test_should_not_process_directory(self, tmp_path):
        filter = PathFilter()
        test_dir = tmp_path / "documents"
        test_dir.mkdir()
        assert filter.should_process(test_dir) is False


class TestFileEvent:
    """Test FileEvent data model."""

    def test_file_event_creation(self):
        event = FileEvent(
            event_type=FileEventType.CREATED,
            file_path=Path("/test/file.txt"),
            timestamp=datetime.now(),
            priority=EventPriority.HIGH,
        )
        assert event.event_type == FileEventType.CREATED
        assert event.file_path == Path("/test/file.txt")
        assert event.priority == EventPriority.HIGH
        assert event.retry_count == 0

    def test_file_event_with_hash(self):
        event = FileEvent(
            event_type=FileEventType.MODIFIED,
            file_path=Path("/test/file.txt"),
            timestamp=datetime.now(),
            priority=EventPriority.MEDIUM,
            file_hash="abc123",
        )
        assert event.file_hash == "abc123"

    def test_event_comparison_by_priority(self):
        high_event = FileEvent(
            event_type=FileEventType.CREATED,
            file_path=Path("/test/1.txt"),
            timestamp=datetime.now(),
            priority=EventPriority.HIGH,
        )
        low_event = FileEvent(
            event_type=FileEventType.DELETED,
            file_path=Path("/test/2.txt"),
            timestamp=datetime.now(),
            priority=EventPriority.LOW,
        )
        assert high_event < low_event

    def test_event_comparison_by_timestamp(self):
        from datetime import timedelta

        now = datetime.now()
        earlier = FileEvent(
            event_type=FileEventType.CREATED,
            file_path=Path("/test/1.txt"),
            timestamp=now - timedelta(seconds=10),
            priority=EventPriority.HIGH,
        )
        later = FileEvent(
            event_type=FileEventType.CREATED,
            file_path=Path("/test/2.txt"),
            timestamp=now,
            priority=EventPriority.HIGH,
        )
        assert earlier < later


class TestDebouncedEventQueue:
    """Test DebouncedEventQueue functionality."""

    @pytest.mark.asyncio()
    async def test_add_event(self):
        queue = DebouncedEventQueue(debounce_seconds=0.1)
        event = FileEvent(
            event_type=FileEventType.CREATED,
            file_path=Path("/test/file.txt"),
            timestamp=datetime.now(),
            priority=EventPriority.HIGH,
        )
        await queue.add_event(event)
        assert event.file_path in queue.pending_events

    @pytest.mark.asyncio()
    async def test_debouncing_merges_events(self):
        queue = DebouncedEventQueue(debounce_seconds=0.2)
        file_path = Path("/test/file.txt")

        event1 = FileEvent(
            event_type=FileEventType.CREATED,
            file_path=file_path,
            timestamp=datetime.now(),
            priority=EventPriority.HIGH,
        )
        await queue.add_event(event1)

        await asyncio.sleep(0.05)

        event2 = FileEvent(
            event_type=FileEventType.MODIFIED,
            file_path=file_path,
            timestamp=datetime.now(),
            priority=EventPriority.MEDIUM,
        )
        await queue.add_event(event2)

        assert file_path in queue.pending_events
        assert queue.pending_events[file_path].event_type == FileEventType.MODIFIED

    @pytest.mark.asyncio()
    async def test_event_flushed_after_delay(self, tmp_path):
        test_file = tmp_path / "test.txt"
        test_file.write_text("content")

        queue = DebouncedEventQueue(debounce_seconds=0.1)
        event = FileEvent(
            event_type=FileEventType.CREATED,
            file_path=test_file,
            timestamp=datetime.now(),
            priority=EventPriority.HIGH,
        )

        await queue.add_event(event)
        await asyncio.sleep(0.2)

        assert test_file not in queue.pending_events
        assert not queue.queue.empty()

    @pytest.mark.asyncio()
    async def test_get_event(self, tmp_path):
        test_file = tmp_path / "test.txt"
        test_file.write_text("content")

        queue = DebouncedEventQueue(debounce_seconds=0.1)
        event = FileEvent(
            event_type=FileEventType.CREATED,
            file_path=test_file,
            timestamp=datetime.now(),
            priority=EventPriority.HIGH,
        )

        await queue.add_event(event)
        await asyncio.sleep(0.2)

        retrieved_event = await queue.get_event()
        assert retrieved_event.file_path == test_file
        assert retrieved_event.event_type == FileEventType.CREATED


class TestFileProcessorWorkerPool:
    """Test FileProcessorWorkerPool functionality."""

    @pytest.mark.asyncio()
    async def test_worker_pool_start(self):
        callback = AsyncMock()
        queue = DebouncedEventQueue()
        pool = FileProcessorWorkerPool(num_workers=2, process_callback=callback)

        await pool.start(queue)
        assert pool.running is True
        assert len(pool.workers) == 2

        await pool.stop()

    @pytest.mark.asyncio()
    async def test_worker_pool_stop(self):
        callback = AsyncMock()
        queue = DebouncedEventQueue()
        pool = FileProcessorWorkerPool(num_workers=2, process_callback=callback)

        await pool.start(queue)
        await pool.stop()

        assert pool.running is False
        for worker in pool.workers:
            assert worker.cancelled() or worker.done()

    @pytest.mark.asyncio()
    async def test_worker_processes_events(self, tmp_path):
        test_file = tmp_path / "test.txt"
        test_file.write_text("content")

        callback = AsyncMock()
        queue = DebouncedEventQueue(debounce_seconds=0.05)
        pool = FileProcessorWorkerPool(num_workers=1, process_callback=callback)

        event = FileEvent(
            event_type=FileEventType.CREATED,
            file_path=test_file,
            timestamp=datetime.now(),
            priority=EventPriority.HIGH,
        )

        await pool.start(queue)
        await queue.add_event(event)
        await asyncio.sleep(0.2)

        callback.assert_called_once()
        await pool.stop()

    @pytest.mark.asyncio()
    async def test_worker_retries_on_failure(self, tmp_path):
        test_file = tmp_path / "test.txt"
        test_file.write_text("content")

        callback = AsyncMock(side_effect=[Exception("Test error"), None])
        queue = DebouncedEventQueue(debounce_seconds=0.05)
        pool = FileProcessorWorkerPool(num_workers=1, process_callback=callback)

        event = FileEvent(
            event_type=FileEventType.CREATED,
            file_path=test_file,
            timestamp=datetime.now(),
            priority=EventPriority.HIGH,
        )

        await pool.start(queue)
        await queue.add_event(event)
        await asyncio.sleep(0.3)

        assert callback.call_count == 2
        await pool.stop()


class TestFileWatcher:
    """Test FileWatcher main class."""

    @pytest.mark.asyncio()
    async def test_file_watcher_initialization(self):
        callback = AsyncMock()
        watcher = FileWatcher(process_callback=callback, num_workers=3, debounce_seconds=0.5)

        assert watcher.process_callback == callback
        assert watcher.worker_pool.num_workers == 3
        assert watcher.event_queue.debounce_seconds == 0.5

    @pytest.mark.asyncio()
    async def test_start_with_valid_directory(self, tmp_path):
        callback = AsyncMock()
        watcher = FileWatcher(process_callback=callback)

        test_dir = tmp_path / "watch_dir"
        test_dir.mkdir()

        await watcher.start([test_dir])
        assert watcher.running is True
        assert len(watcher.watch_paths) == 1
        assert watcher.watch_paths[0] == test_dir

        await watcher.stop()

    @pytest.mark.asyncio()
    async def test_start_with_invalid_directory(self):
        callback = AsyncMock()
        watcher = FileWatcher(process_callback=callback)

        with pytest.raises(ValueError, match="Directory does not exist"):
            await watcher.start(["/nonexistent/path"])

    @pytest.mark.asyncio()
    async def test_start_with_file_instead_of_directory(self, tmp_path):
        callback = AsyncMock()
        watcher = FileWatcher(process_callback=callback)

        test_file = tmp_path / "file.txt"
        test_file.write_text("content")

        with pytest.raises(ValueError, match="Path is not a directory"):
            await watcher.start([test_file])

    @pytest.mark.asyncio()
    async def test_stop_watcher(self, tmp_path):
        callback = AsyncMock()
        watcher = FileWatcher(process_callback=callback)

        test_dir = tmp_path / "watch_dir"
        test_dir.mkdir()

        await watcher.start([test_dir])
        await watcher.stop()

        assert watcher.running is False

    @pytest.mark.asyncio()
    async def test_scan_directory(self, tmp_path):
        callback = AsyncMock()
        watcher = FileWatcher(process_callback=callback, debounce_seconds=0.05)

        test_dir = tmp_path / "watch_dir"
        test_dir.mkdir()

        (test_dir / "file1.txt").write_text("content1")
        (test_dir / "file2.txt").write_text("content2")

        sub_dir = test_dir / "subdir"
        sub_dir.mkdir()
        (sub_dir / "file3.txt").write_text("content3")

        await watcher.start([test_dir])
        await watcher.scan_directory(test_dir)

        await asyncio.sleep(0.2)

        assert callback.call_count >= 3
        await watcher.stop()

    @pytest.mark.asyncio()
    async def test_watcher_filters_ignored_files(self, tmp_path):
        callback = AsyncMock()
        watcher = FileWatcher(
            process_callback=callback, ignore_patterns=["*.tmp"], debounce_seconds=0.05
        )

        test_dir = tmp_path / "watch_dir"
        test_dir.mkdir()

        (test_dir / "file.txt").write_text("content")
        (test_dir / "temp.tmp").write_text("temp")

        await watcher.start([test_dir])
        await watcher.scan_directory(test_dir)

        await asyncio.sleep(0.2)

        assert callback.call_count == 1
        await watcher.stop()

    @pytest.mark.asyncio()
    async def test_watcher_already_running(self, tmp_path):
        callback = AsyncMock()
        watcher = FileWatcher(process_callback=callback)

        test_dir = tmp_path / "watch_dir"
        test_dir.mkdir()

        await watcher.start([test_dir])
        await watcher.start([test_dir])

        assert watcher.running is True
        await watcher.stop()


@pytest.mark.asyncio()
async def test_integration_file_created_event(tmp_path):
    """Integration test: File creation triggers callback."""
    callback = AsyncMock()
    watcher = FileWatcher(process_callback=callback, debounce_seconds=0.1)

    test_dir = tmp_path / "watch_dir"
    test_dir.mkdir()

    await watcher.start([test_dir])
    await asyncio.sleep(0.1)

    test_file = test_dir / "new_file.txt"
    test_file.write_text("Hello World")

    await asyncio.sleep(0.5)

    assert callback.call_count >= 1
    call_args = callback.call_args[0][0]
    assert call_args.event_type in [FileEventType.CREATED, FileEventType.MODIFIED]
    assert "new_file.txt" in str(call_args.file_path)

    await watcher.stop()


@pytest.mark.asyncio()
async def test_integration_file_modified_event(tmp_path):
    """Integration test: File modification triggers callback."""
    callback = AsyncMock()
    watcher = FileWatcher(process_callback=callback, debounce_seconds=0.1)

    test_dir = tmp_path / "watch_dir"
    test_dir.mkdir()
    test_file = test_dir / "existing_file.txt"
    test_file.write_text("Original content")

    await watcher.start([test_dir])
    await asyncio.sleep(0.2)

    callback.reset_mock()

    test_file.write_text("Modified content")
    await asyncio.sleep(0.5)

    assert callback.call_count >= 1

    await watcher.stop()


@pytest.mark.asyncio()
async def test_integration_multiple_rapid_changes(tmp_path):
    """Integration test: Rapid changes are debounced."""
    callback = AsyncMock()
    watcher = FileWatcher(process_callback=callback, debounce_seconds=0.3)

    test_dir = tmp_path / "watch_dir"
    test_dir.mkdir()
    test_file = test_dir / "rapid_file.txt"
    test_file.write_text("Version 1")

    await watcher.start([test_dir])
    await asyncio.sleep(0.2)

    callback.reset_mock()

    for i in range(2, 6):
        test_file.write_text(f"Version {i}")
        await asyncio.sleep(0.05)

    await asyncio.sleep(0.5)

    assert callback.call_count < 4

    await watcher.stop()
