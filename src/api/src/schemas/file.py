from datetime import datetime
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field


class FileUploadResponse(BaseModel):
    """Response after file upload."""

    id: UUID
    filename: str
    size_bytes: int
    mime_type: str
    upload_status: str
    created_at: datetime

    model_config = ConfigDict(from_attributes=True)


class FileMetadataUpdate(BaseModel):
    """Request to update file metadata."""

    tags: list[str] | None = None
    description: str | None = Field(None, max_length=1000)
    custom_metadata: dict | None = None


class FileMetadata(BaseModel):
    """Complete file metadata response."""

    id: UUID
    filename: str
    original_path: str | None = None
    size_bytes: int
    mime_type: str
    tags: list[str] = []
    description: str | None = None
    custom_metadata: dict = {}
    upload_status: str
    indexed_at: datetime | None = None
    created_at: datetime
    updated_at: datetime

    model_config = ConfigDict(from_attributes=True)


class FileListResponse(BaseModel):
    """Paginated file list response."""

    items: list[FileMetadata]
    total: int
    page: int
    page_size: int
    total_pages: int


class FileListParams(BaseModel):
    """Query parameters for file list."""

    page: int = Field(1, ge=1)
    page_size: int = Field(20, ge=1, le=100)
    tags: list[str] | None = None
    mime_type: str | None = None
    search: str | None = None
    sort_by: str = Field("created_at", pattern="^(created_at|updated_at|filename|size_bytes)$")
    sort_order: str = Field("desc", pattern="^(asc|desc)$")
