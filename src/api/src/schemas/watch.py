from datetime import datetime
from pathlib import Path
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field, field_validator


class WatchDirectoryCreate(BaseModel):
    """Request to add watch directory."""

    path: str = Field(..., min_length=1)
    recursive: bool = True
    file_patterns: list[str] = ["*"]
    ignore_patterns: list[str] = []
    auto_index: bool = True

    @field_validator("path")
    @classmethod
    def validate_path_exists(cls, v: str) -> str:
        path = Path(v)
        if not path.exists():
            raise ValueError(f"Path does not exist: {v}")
        if not path.is_dir():
            raise ValueError(f"Path is not a directory: {v}")
        return str(path.absolute())


class WatchDirectory(BaseModel):
    """Watch directory response."""

    id: UUID
    path: str
    recursive: bool
    file_patterns: list[str]
    ignore_patterns: list[str]
    auto_index: bool
    status: str
    files_watched: int
    last_scan: datetime | None = None
    created_at: datetime

    model_config = ConfigDict(from_attributes=True)


class WatchDirectoryList(BaseModel):
    """List of watch directories."""

    items: list[WatchDirectory]
    total: int


class WatchStatus(BaseModel):
    """Watch system status."""

    total_directories: int
    active_watchers: int
    total_files_watched: int
    pending_index_queue: int
    last_event: datetime | None = None


class ReindexRequest(BaseModel):
    """Request to trigger manual reindex."""

    watch_id: UUID | None = None
    force: bool = False
