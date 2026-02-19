"""
FileWatcher Module

Monitors directories for file changes and triggers indexing pipeline.

Example usage:
    from vault.backend.src.modules.file_watcher import FileWatcher, FileEvent

    async def process_file(event: FileEvent):
        print(f"Processing {event.file_path}")

    watcher = FileWatcher(process_callback=process_file)
    await watcher.start(["/path/to/watch"])
"""

from .filters import PathFilter
from .types import EventPriority, FileEvent, FileEventType
from .watcher import FileWatcher

__all__ = [
    "EventPriority",
    "FileEvent",
    "FileEventType",
    "FileWatcher",
    "PathFilter",
]
