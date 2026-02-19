"""Pydantic schemas for sync API."""

from datetime import datetime
from enum import Enum
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field


class SyncActionEnum(str, Enum):
    """Sync action types."""

    CREATE = "create"
    UPDATE = "update"
    DELETE = "delete"


class ConflictStatusEnum(str, Enum):
    """Conflict resolution status."""

    PENDING = "pending"
    RESOLVED_LOCAL = "resolved_local"
    RESOLVED_REMOTE = "resolved_remote"
    RESOLVED_MERGE = "resolved_merge"


# Device schemas
class DeviceCreate(BaseModel):
    """Schema for device registration."""

    device_id: UUID = Field(..., description="Unique device identifier (UUID)")
    device_name: str = Field(
        ..., min_length=1, max_length=255, description="User-friendly device name"
    )

    model_config = ConfigDict(from_attributes=True)


class DeviceResponse(BaseModel):
    """Schema for device response."""

    id: int
    device_id: UUID
    device_name: str
    last_seen_at: datetime
    created_at: datetime

    model_config = ConfigDict(from_attributes=True)


# Document sync schemas
class DocumentChange(BaseModel):
    """Schema for a single document change."""

    id: int | None = None
    action: SyncActionEnum
    path: str = Field(..., min_length=1, max_length=1024)
    title: str | None = Field(None, max_length=512)
    content: str | None = None
    content_hash: str | None = Field(None, max_length=64)
    modified_at: datetime
    version: int = Field(default=1, ge=1)
    deleted_at: datetime | None = Field(
        None, description="When document was soft-deleted (null if not deleted)"
    )
    deleted_by_device_id: int | None = Field(None, description="Device that deleted the document")

    model_config = ConfigDict(from_attributes=True)


class PullRequest(BaseModel):
    """Schema for pull request (get changes from server)."""

    device_id: UUID = Field(..., description="Device requesting sync")
    since_timestamp: datetime | None = Field(
        None, description="Get changes since this timestamp (null for first sync)"
    )

    model_config = ConfigDict(from_attributes=True)


class ConflictResponse(BaseModel):
    """Schema for conflict information."""

    id: int
    document_id: int
    local_version: int
    remote_version: int
    local_modified_at: datetime
    remote_modified_at: datetime
    local_content_hash: str | None
    remote_content_hash: str | None
    status: ConflictStatusEnum
    created_at: datetime

    model_config = ConfigDict(from_attributes=True)


class PullResponse(BaseModel):
    """Schema for pull response (changes from server)."""

    changes: list[DocumentChange] = Field(default_factory=list)
    conflicts: list[ConflictResponse] = Field(default_factory=list)
    new_timestamp: datetime = Field(description="Timestamp to use for next pull")
    total_changes: int = Field(description="Total number of changes")

    model_config = ConfigDict(from_attributes=True)


class PushRequest(BaseModel):
    """Schema for push request (send changes to server)."""

    device_id: UUID = Field(..., description="Device sending changes")
    changes: list[DocumentChange] = Field(..., min_length=1)

    model_config = ConfigDict(from_attributes=True)


class PushResponse(BaseModel):
    """Schema for push response (results from server)."""

    accepted: list[str] = Field(
        default_factory=list, description="Paths of successfully synced documents"
    )
    conflicts: list[ConflictResponse] = Field(
        default_factory=list, description="Detected conflicts requiring resolution"
    )
    timestamp: datetime = Field(description="Server timestamp")
    total_accepted: int
    total_conflicts: int

    model_config = ConfigDict(from_attributes=True)


class ConflictResolution(BaseModel):
    """Schema for resolving a conflict."""

    conflict_id: int = Field(..., gt=0)
    resolution: ConflictStatusEnum = Field(
        ..., description="How to resolve: keep local, keep remote, or merge"
    )
    merged_content: str | None = Field(
        None, description="Required if resolution is 'resolved_merge'"
    )

    model_config = ConfigDict(from_attributes=True)


class SyncStatus(BaseModel):
    """Schema for overall sync status."""

    device_id: UUID
    last_pull_timestamp: datetime | None
    last_push_timestamp: datetime | None
    pending_conflicts: int
    synced_documents: int

    model_config = ConfigDict(from_attributes=True)


class CleanupResponse(BaseModel):
    """Schema for cleanup job response."""

    deleted_count: int = Field(..., description="Number of documents hard-deleted")
    cleanup_timestamp: datetime = Field(..., description="When cleanup was performed")

    model_config = ConfigDict(from_attributes=True)
