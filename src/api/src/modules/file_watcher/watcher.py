from collections.abc import Awaitable, Callable
from datetime import datetime
import logging
from pathlib import Path

from watchdog.observers import Observer

from .event_handler import VaultFileEventHandler
from .event_queue import DebouncedEventQueue
from .filters import PathFilter
from .types import EventPriority, FileEvent, FileEventType
from .worker_pool import FileProcessorWorkerPool

logger = logging.getLogger(__name__)


class FileWatcher:
    def __init__(
        self,
        process_callback: Callable[[FileEvent], Awaitable[None]],
        ignore_patterns: list[str] | None = None,
        num_workers: int = 3,
        debounce_seconds: float = 0.5,
    ):
        self.process_callback = process_callback
        self.path_filter = PathFilter(ignore_patterns)
        self.event_queue = DebouncedEventQueue(debounce_seconds=debounce_seconds, maxsize=1000)
        self.worker_pool = FileProcessorWorkerPool(
            num_workers=num_workers, process_callback=process_callback
        )
        self.observer = Observer()
        self.event_handler = VaultFileEventHandler(
            event_queue=self.event_queue, path_filter=self.path_filter
        )
        self.watch_paths: list[Path] = []
        self.running = False

    async def start(self, watch_directories: list[str | Path]) -> None:
        if self.running:
            logger.warning("FileWatcher already running")
            return

        self.watch_paths = [Path(p) for p in watch_directories]

        for path in self.watch_paths:
            if not path.exists():
                raise ValueError(f"Directory does not exist: {path}")
            if not path.is_dir():
                raise ValueError(f"Path is not a directory: {path}")

        for path in self.watch_paths:
            self.observer.schedule(self.event_handler, str(path), recursive=True)
            logger.info(f"Watching directory: {path}")

        self.observer.start()

        await self.worker_pool.start(self.event_queue)

        self.running = True
        logger.info("FileWatcher started")

    async def stop(self) -> None:
        if not self.running:
            return

        logger.info("Stopping FileWatcher...")

        self.observer.stop()
        self.observer.join(timeout=5)

        await self.worker_pool.stop()

        self.running = False
        logger.info("FileWatcher stopped")

    async def scan_directory(self, directory: Path) -> None:
        logger.info(f"Scanning directory: {directory}")

        count = 0
        for file_path in directory.rglob("*"):
            if file_path.is_file() and self.path_filter.should_process(file_path):
                event = FileEvent(
                    event_type=FileEventType.CREATED,
                    file_path=file_path,
                    timestamp=datetime.now(),
                    priority=EventPriority.HIGH,
                )
                await self.event_queue.add_event(event)
                count += 1

        logger.info(f"Queued {count} files from initial scan")
