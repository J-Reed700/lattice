"""Type definitions for the export module.

Defines data structures for export requests, results, and configuration.
"""

from datetime import UTC, datetime
from enum import Enum
from typing import Any
from uuid import UUID

from pydantic import BaseModel, Field


class ExportFormat(str, Enum):
    """Supported export formats.

    Attributes:
        JSON: Complete structured export with all metadata
        CSV: Tabular format for spreadsheet analysis
        MARKDOWN: Human-readable format with folder structure
        ZIP: Bundle of original files plus metadata
    """

    JSON = "json"
    CSV = "csv"
    MARKDOWN = "markdown"
    ZIP = "zip"


class ExportScope(str, Enum):
    """Export scope types.

    Attributes:
        FULL: Export entire knowledge base
        FILTERED: Export with filters (date range, file types, tags, folders)
        SEARCH_RESULTS: Export current search results
        SELECTED: Export user-selected files only
    """

    FULL = "full"
    FILTERED = "filtered"
    SEARCH_RESULTS = "search_results"
    SELECTED = "selected"


class ExportStatus(str, Enum):
    """Export job status.

    Attributes:
        PENDING: Job queued, not started
        IN_PROGRESS: Currently processing
        COMPLETED: Successfully completed
        FAILED: Failed with error
        CANCELLED: Cancelled by user
    """

    PENDING = "pending"
    IN_PROGRESS = "in_progress"
    COMPLETED = "completed"
    FAILED = "failed"
    CANCELLED = "cancelled"


class ExportFilters(BaseModel):
    """Filters for filtered exports.

    Attributes:
        date_from: Start date for filtering (inclusive)
        date_to: End date for filtering (inclusive)
        file_types: List of file extensions to include
        tags: List of tags to filter by
        folder_ids: List of watch folder IDs to include
        search_query: Optional search query to filter results

    Example:
        >>> filters = ExportFilters(
        ...     date_from=datetime(2024, 1, 1),
        ...     date_to=datetime(2024, 12, 31),
        ...     file_types=["pdf", "docx"],
        ...     tags=["important", "work"]
        ... )
    """

    date_from: datetime | None = Field(None, description="Filter files modified after this date")
    date_to: datetime | None = Field(None, description="Filter files modified before this date")
    file_types: list[str] | None = Field(None, description="List of file extensions to include")
    tags: list[str] | None = Field(None, description="Filter by these tags")
    folder_ids: list[UUID] | None = Field(None, description="Filter by watch folder IDs")
    search_query: str | None = Field(None, description="Search query to filter results")


class ExportRequest(BaseModel):
    """Export request model.

    Attributes:
        format: Export format to use
        scope: Export scope (full, filtered, etc.)
        filters: Optional filters for filtered exports
        file_ids: List of file IDs for selected exports
        include_embeddings: Whether to include vector embeddings
        include_original_files: Whether to include original files (ZIP only)
        compress: Whether to compress output (for JSON/CSV)

    Example:
        >>> request = ExportRequest(
        ...     format=ExportFormat.JSON,
        ...     scope=ExportScope.FILTERED,
        ...     filters=ExportFilters(file_types=["pdf"]),
        ...     include_embeddings=False
        ... )
    """

    format: ExportFormat = Field(..., description="Export format")
    scope: ExportScope = Field(..., description="Export scope")
    filters: ExportFilters | None = Field(None, description="Filters for filtered exports")
    file_ids: list[UUID] | None = Field(None, description="File IDs for selected exports")
    include_embeddings: bool = Field(False, description="Include vector embeddings in export")
    include_original_files: bool = Field(True, description="Include original files (ZIP only)")
    compress: bool = Field(True, description="Compress output files")


class ExportResult(BaseModel):
    """Export result model.

    Attributes:
        export_id: Unique identifier for this export
        status: Current status of the export
        format: Format used for export
        scope: Scope of the export
        file_count: Number of files exported
        total_size_bytes: Total size of exported data
        output_path: Path to the exported file
        created_at: When export was created
        completed_at: When export completed (if applicable)
        error_message: Error message if failed
        progress_percent: Progress percentage (0-100)
        estimated_time_remaining: Estimated seconds remaining

    Example:
        >>> result = ExportResult(
        ...     export_id=uuid4(),
        ...     status=ExportStatus.COMPLETED,
        ...     format=ExportFormat.JSON,
        ...     scope=ExportScope.FULL,
        ...     file_count=150,
        ...     total_size_bytes=10485760,
        ...     output_path="/tmp/export_123.json.gz"
        ... )
    """

    export_id: UUID = Field(..., description="Unique export identifier")
    status: ExportStatus = Field(..., description="Current export status")
    format: ExportFormat = Field(..., description="Export format")
    scope: ExportScope = Field(..., description="Export scope")
    file_count: int = Field(0, description="Number of files exported")
    total_size_bytes: int = Field(0, description="Total size in bytes")
    output_path: str | None = Field(None, description="Path to exported file")
    created_at: datetime = Field(
        default_factory=lambda: datetime.now(UTC), description="Creation timestamp"
    )
    completed_at: datetime | None = Field(None, description="Completion timestamp")
    error_message: str | None = Field(None, description="Error message if failed")
    progress_percent: float = Field(0.0, description="Progress percentage (0-100)")
    estimated_time_remaining: int | None = Field(None, description="Estimated seconds remaining")

    class Config:
        json_schema_extra = {
            "example": {
                "export_id": "123e4567-e89b-12d3-a456-426614174000",
                "status": "completed",
                "format": "json",
                "scope": "full",
                "file_count": 150,
                "total_size_bytes": 10485760,
                "output_path": "/tmp/exports/export_123.json.gz",
                "created_at": "2024-01-01T00:00:00Z",
                "completed_at": "2024-01-01T00:05:00Z",
                "progress_percent": 100.0,
            }
        }


class ExportJob(BaseModel):
    """Internal export job tracking model.

    Used for tracking export jobs in the system, including cleanup scheduling.

    Attributes:
        id: Job ID (same as export_id)
        request: Original export request
        result: Current export result
        cleanup_at: When to auto-cleanup this export (default: 24 hours)
    """

    id: UUID = Field(..., description="Job ID")
    request: ExportRequest = Field(..., description="Original request")
    result: ExportResult = Field(..., description="Current result")
    cleanup_at: datetime = Field(..., description="Auto-cleanup timestamp")


class ExportMetadata(BaseModel):
    """Metadata included in exports.

    Attributes:
        exported_at: When the export was created
        vault_version: Version of Vault that created the export
        total_files: Total number of files in export
        total_size_bytes: Total size of all files
        export_format: Format of this export
        filters_applied: Description of filters applied
    """

    exported_at: datetime = Field(default_factory=lambda: datetime.now(UTC))
    vault_version: str = Field("1.0.0", description="Vault version")
    total_files: int = Field(0, description="Total files exported")
    total_size_bytes: int = Field(0, description="Total size in bytes")
    export_format: ExportFormat = Field(..., description="Export format")
    filters_applied: dict[str, Any] | None = Field(None, description="Filters used")


class FileExportData(BaseModel):
    """Data structure for a single file in export.

    Attributes:
        id: File UUID
        path: Original file path
        filename: File name
        extension: File extension
        mime_type: MIME type
        size_bytes: File size in bytes
        hash_sha256: SHA-256 hash
        modified_at: Last modification time
        indexed_at: When file was indexed
        text_content: Extracted text content (if available)
        word_count: Word count
        char_count: Character count
        language: Detected language
        tags: Associated tags
        metadata: Additional metadata
        embeddings: Vector embeddings (if requested)
    """

    id: UUID
    path: str
    filename: str
    extension: str
    mime_type: str
    size_bytes: int
    hash_sha256: str
    modified_at: datetime
    indexed_at: datetime
    text_content: str | None = None
    word_count: int | None = None
    char_count: int | None = None
    language: str | None = None
    tags: list[str] = Field(default_factory=list)
    metadata: dict[str, Any] = Field(default_factory=dict)
    embeddings: list[float] | None = None

    class Config:
        json_schema_extra = {
            "example": {
                "id": "123e4567-e89b-12d3-a456-426614174000",
                "path": "/Users/john/Documents/report.pdf",
                "filename": "report.pdf",
                "extension": "pdf",
                "mime_type": "application/pdf",
                "size_bytes": 1024000,
                "hash_sha256": "abc123...",
                "modified_at": "2024-01-01T00:00:00Z",
                "indexed_at": "2024-01-01T00:05:00Z",
                "text_content": "This is the document content...",
                "word_count": 500,
                "char_count": 3000,
                "language": "en",
                "tags": ["important", "work"],
            }
        }
