import asyncio
import hashlib
import logging
from pathlib import Path
from queue import PriorityQueue

from .types import FileEvent, FileEventType

logger = logging.getLogger(__name__)


def _log_task_exception(task: asyncio.Task) -> None:
    if not task.cancelled() and (exc := task.exception()):
        logger.error(f"Background task failed: {exc}", exc_info=exc)


class DebouncedEventQueue:
    def __init__(self, debounce_seconds: float = 0.5, maxsize: int = 1000):
        self.debounce_seconds = debounce_seconds
        self.queue: PriorityQueue[FileEvent] = PriorityQueue(maxsize=maxsize)
        self.pending_events: dict[Path, FileEvent] = {}
        self.file_hashes: dict[Path, str] = {}
        self._lock = asyncio.Lock()
        self._background_tasks: set[asyncio.Task] = set()

    async def add_event(self, event: FileEvent) -> None:
        async with self._lock:
            if event.file_path in self.pending_events:
                existing = self.pending_events[event.file_path]
                existing.timestamp = event.timestamp
                existing.event_type = event.event_type
            else:
                self.pending_events[event.file_path] = event
                task = asyncio.create_task(self._flush_event_after_delay(event.file_path))
                self._background_tasks.add(task)
                task.add_done_callback(self._background_tasks.discard)
                task.add_done_callback(_log_task_exception)

    async def _flush_event_after_delay(self, file_path: Path) -> None:
        await asyncio.sleep(self.debounce_seconds)

        async with self._lock:
            if file_path in self.pending_events:
                event = self.pending_events.pop(file_path)

                if event.event_type != FileEventType.DELETED:
                    current_hash = await self._compute_file_hash(file_path)
                    if current_hash == self.file_hashes.get(file_path):
                        return
                    event.file_hash = current_hash
                    self.file_hashes[file_path] = current_hash

                self.queue.put(event)

    async def _compute_file_hash(self, file_path: Path) -> str | None:
        try:
            hasher = hashlib.sha256()
            with open(file_path, "rb") as f:
                while chunk := f.read(8192):
                    hasher.update(chunk)
            return hasher.hexdigest()
        except Exception as e:
            logger.debug(f"Failed to compute hash for {file_path}: {e}")
            return None

    async def get_event(self) -> FileEvent:
        loop = asyncio.get_event_loop()
        return await loop.run_in_executor(None, self.queue.get)

    def empty(self) -> bool:
        return self.queue.empty() and not self.pending_events
