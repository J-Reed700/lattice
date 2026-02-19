import asyncio
from datetime import datetime
import logging
from pathlib import Path

from watchdog.events import FileSystemEvent, FileSystemEventHandler

from .event_queue import DebouncedEventQueue
from .filters import PathFilter
from .types import EventPriority, FileEvent, FileEventType

logger = logging.getLogger(__name__)


def _log_task_exception(task: asyncio.Task) -> None:
    if not task.cancelled() and (exc := task.exception()):
        logger.error(f"Background task failed: {exc}", exc_info=exc)


class VaultFileEventHandler(FileSystemEventHandler):
    def __init__(self, event_queue: DebouncedEventQueue, path_filter: PathFilter):
        super().__init__()
        self.event_queue = event_queue
        self.path_filter = path_filter
        self._background_tasks: set[asyncio.Task] = set()

    def on_created(self, event: FileSystemEvent) -> None:
        if event.is_directory:
            return

        file_path = Path(event.src_path)
        if not self.path_filter.should_process(file_path):
            return

        logger.info(f"File created: {file_path}")
        file_event = FileEvent(
            event_type=FileEventType.CREATED,
            file_path=file_path,
            timestamp=datetime.now(),
            priority=EventPriority.HIGH,
        )
        task = asyncio.create_task(self.event_queue.add_event(file_event))
        self._background_tasks.add(task)
        task.add_done_callback(self._background_tasks.discard)
        task.add_done_callback(_log_task_exception)

    def on_modified(self, event: FileSystemEvent) -> None:
        if event.is_directory:
            return

        file_path = Path(event.src_path)
        if not self.path_filter.should_process(file_path):
            return

        logger.info(f"File modified: {file_path}")
        file_event = FileEvent(
            event_type=FileEventType.MODIFIED,
            file_path=file_path,
            timestamp=datetime.now(),
            priority=EventPriority.MEDIUM,
        )
        task = asyncio.create_task(self.event_queue.add_event(file_event))
        self._background_tasks.add(task)
        task.add_done_callback(self._background_tasks.discard)
        task.add_done_callback(_log_task_exception)

    def on_deleted(self, event: FileSystemEvent) -> None:
        if event.is_directory:
            return

        file_path = Path(event.src_path)

        logger.info(f"File deleted: {file_path}")
        file_event = FileEvent(
            event_type=FileEventType.DELETED,
            file_path=file_path,
            timestamp=datetime.now(),
            priority=EventPriority.LOW,
        )
        task = asyncio.create_task(self.event_queue.add_event(file_event))
        self._background_tasks.add(task)
        task.add_done_callback(self._background_tasks.discard)
        task.add_done_callback(_log_task_exception)

    def on_moved(self, event: FileSystemEvent) -> None:
        if event.is_directory:
            return

        self.on_deleted(event)

        if hasattr(event, "dest_path"):

            class NewEvent:
                src_path = event.dest_path
                is_directory = False

            self.on_created(NewEvent())
