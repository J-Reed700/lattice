from dataclasses import dataclass
from datetime import datetime
from enum import Enum
from pathlib import Path


class FileEventType(str, Enum):
    CREATED = "created"
    MODIFIED = "modified"
    DELETED = "deleted"
    MOVED = "moved"


class EventPriority(int, Enum):
    HIGH = 1
    MEDIUM = 2
    LOW = 3


@dataclass
class FileEvent:
    event_type: FileEventType
    file_path: Path
    timestamp: datetime
    priority: EventPriority
    file_hash: str | None = None
    retry_count: int = 0

    def __lt__(self, other):
        if self.priority != other.priority:
            return self.priority < other.priority
        return self.timestamp < other.timestamp
