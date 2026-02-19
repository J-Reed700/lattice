"""
Storage Manager Data Models

Pydantic models for storage management operations.
"""

from datetime import datetime
from enum import Enum
from typing import Any

from pydantic import BaseModel, Field, field_validator


class CleanupOperation(str, Enum):
    """Available cleanup operations.

    Attributes:
        ORPHANED_FILES: Remove files in storage but not in database
        CACHE: Clear temporary cache files
        THUMBNAILS: Delete generated thumbnails
        OLD_LOGS: Remove old log files
        VACUUM_DB: Reclaim database space
        OLD_EMBEDDINGS: Delete old embedding vectors
    """

    ORPHANED_FILES = "orphaned_files"
    CACHE = "cache"
    THUMBNAILS = "thumbnails"
    OLD_LOGS = "old_logs"
    VACUUM_DB = "vacuum_db"
    OLD_EMBEDDINGS = "old_embeddings"


class FileInfo(BaseModel):
    """Information about a single file.

    Attributes:
        path: Full path to the file
        size_bytes: File size in bytes
        modified_at: Last modification timestamp
        file_type: File extension/type
        category: Storage category (e.g., 'original_files', 'thumbnails')

    Example:
        >>> file = FileInfo(
        ...     path="/storage/documents/report.pdf",
        ...     size_bytes=1024000,
        ...     modified_at=datetime.now(),
        ...     file_type="pdf",
        ...     category="original_files"
        ... )
    """

    path: str = Field(description="Full file path")
    size_bytes: int = Field(ge=0, description="File size in bytes")
    modified_at: datetime = Field(description="Last modification timestamp")
    file_type: str = Field(description="File extension without dot")
    category: str = Field(description="Storage category")

    class Config:
        json_schema_extra = {
            "example": {
                "path": "/storage/documents/report.pdf",
                "size_bytes": 1024000,
                "modified_at": "2025-11-10T12:00:00Z",
                "file_type": "pdf",
                "category": "original_files",
            }
        }


class StorageBreakdown(BaseModel):
    """Storage breakdown by category.

    Attributes:
        original_files: Space used by original uploaded/indexed files
        embeddings: Space used by vector embeddings
        database: Database file size
        thumbnails: Generated thumbnails and previews
        cache: Temporary cache files
        logs: Log files

    Example:
        >>> breakdown = StorageBreakdown(
        ...     original_files=5368709120,
        ...     embeddings=2147483648,
        ...     database=1073741824,
        ...     thumbnails=536870912,
        ...     cache=268435456,
        ...     logs=104857600
        ... )
    """

    original_files: int = Field(ge=0, description="Original files in bytes")
    embeddings: int = Field(ge=0, description="Embedding vectors in bytes")
    database: int = Field(ge=0, description="Database size in bytes")
    thumbnails: int = Field(ge=0, description="Thumbnails in bytes")
    cache: int = Field(ge=0, description="Cache files in bytes")
    logs: int = Field(ge=0, description="Log files in bytes")

    @property
    def total(self) -> int:
        """Calculate total bytes across all categories."""
        return (
            self.original_files
            + self.embeddings
            + self.database
            + self.thumbnails
            + self.cache
            + self.logs
        )

    class Config:
        json_schema_extra = {
            "example": {
                "original_files": 5368709120,
                "embeddings": 2147483648,
                "database": 1073741824,
                "thumbnails": 536870912,
                "cache": 268435456,
                "logs": 104857600,
            }
        }


class TrendPoint(BaseModel):
    """Single point in storage growth trend.

    Attributes:
        date: Date of the measurement
        size_bytes: Total storage size at that date

    Example:
        >>> trend = TrendPoint(
        ...     date=datetime(2025, 11, 10).date(),
        ...     size_bytes=10737418240
        ... )
    """

    date: datetime = Field(description="Measurement date")
    size_bytes: int = Field(ge=0, description="Storage size in bytes")

    class Config:
        json_schema_extra = {"example": {"date": "2025-11-10", "size_bytes": 10737418240}}


class StorageStats(BaseModel):
    """Complete storage statistics.

    Attributes:
        total_bytes: Total storage used across all categories
        breakdown: Breakdown by storage category
        by_file_type: Storage grouped by file extension
        by_date: Storage grouped by indexing date ranges
        largest_files: Top files by size
        growth_trend: Historical storage growth data points
        last_calculated: When these stats were calculated
        cached: Whether stats are from cache

    Example:
        >>> stats = StorageStats(
        ...     total_bytes=10737418240,
        ...     breakdown=StorageBreakdown(...),
        ...     by_file_type={"pdf": 3221225472, "png": 2147483648},
        ...     by_date={"this_week": 1073741824, "this_month": 3221225472},
        ...     largest_files=[FileInfo(...), ...],
        ...     growth_trend=[TrendPoint(...), ...],
        ...     last_calculated=datetime.now(),
        ...     cached=False
        ... )
    """

    total_bytes: int = Field(ge=0, description="Total storage in bytes")
    breakdown: StorageBreakdown = Field(description="Category breakdown")
    by_file_type: dict[str, int] = Field(
        default_factory=dict, description="Storage by file extension"
    )
    by_date: dict[str, int] = Field(default_factory=dict, description="Storage by date range")
    largest_files: list[FileInfo] = Field(default_factory=list, description="Top files by size")
    growth_trend: list[TrendPoint] = Field(default_factory=list, description="Historical growth")
    last_calculated: datetime = Field(description="Calculation timestamp")
    cached: bool = Field(default=False, description="From cache?")

    @field_validator("total_bytes")
    @classmethod
    def validate_total_bytes(cls, v: int, info: Any) -> int:
        """Ensure total_bytes matches breakdown total if available."""
        if "breakdown" in info.data:
            breakdown_total = info.data["breakdown"].total
            if v != breakdown_total:
                raise ValueError(
                    f"total_bytes ({v}) must match breakdown total ({breakdown_total})"
                )
        return v

    class Config:
        json_schema_extra = {
            "example": {
                "total_bytes": 10737418240,
                "breakdown": {
                    "original_files": 5368709120,
                    "embeddings": 2147483648,
                    "database": 1073741824,
                    "thumbnails": 536870912,
                    "cache": 268435456,
                    "logs": 104857600,
                },
                "by_file_type": {"pdf": 3221225472, "png": 2147483648},
                "by_date": {"this_week": 1073741824, "this_month": 3221225472},
                "largest_files": [],
                "growth_trend": [],
                "last_calculated": "2025-11-10T12:00:00Z",
                "cached": False,
            }
        }


class CleanupRequest(BaseModel):
    """Request to perform cleanup operation.

    Attributes:
        operation: Type of cleanup to perform
        dry_run: If True, only preview without deleting
        older_than_days: Only clean files older than N days
        file_types: Only clean specific file types
        min_size_bytes: Only clean files larger than threshold

    Example:
        >>> request = CleanupRequest(
        ...     operation=CleanupOperation.ORPHANED_FILES,
        ...     dry_run=True,
        ...     older_than_days=30
        ... )
    """

    operation: CleanupOperation = Field(description="Cleanup operation type")
    dry_run: bool = Field(default=True, description="Preview mode")
    older_than_days: int | None = Field(default=None, ge=1, description="Age threshold in days")
    file_types: list[str] | None = Field(default=None, description="Filter by file extensions")
    min_size_bytes: int | None = Field(default=None, ge=0, description="Minimum file size")

    class Config:
        json_schema_extra = {
            "example": {
                "operation": "orphaned_files",
                "dry_run": True,
                "older_than_days": 30,
                "file_types": ["tmp", "cache"],
                "min_size_bytes": 1048576,
            }
        }


class CleanupResult(BaseModel):
    """Result of cleanup operation.

    Attributes:
        operation: The cleanup operation performed
        dry_run: Whether this was a preview
        files_affected: Number of files processed
        space_reclaimed_bytes: Space freed in bytes
        errors: List of errors encountered
        duration_seconds: Operation duration
        timestamp: When operation completed

    Example:
        >>> result = CleanupResult(
        ...     operation=CleanupOperation.ORPHANED_FILES,
        ...     dry_run=False,
        ...     files_affected=42,
        ...     space_reclaimed_bytes=536870912,
        ...     errors=[],
        ...     duration_seconds=2.5,
        ...     timestamp=datetime.now()
        ... )
    """

    operation: CleanupOperation = Field(description="Operation type")
    dry_run: bool = Field(description="Was this a preview")
    files_affected: int = Field(ge=0, description="Files processed")
    space_reclaimed_bytes: int = Field(ge=0, description="Space freed in bytes")
    errors: list[str] = Field(default_factory=list, description="Error messages")
    duration_seconds: float = Field(ge=0, description="Operation duration")
    timestamp: datetime = Field(description="Completion timestamp")
    files: list[FileInfo] | None = Field(default=None, description="Detailed file list (dry_run)")

    class Config:
        json_schema_extra = {
            "example": {
                "operation": "orphaned_files",
                "dry_run": False,
                "files_affected": 42,
                "space_reclaimed_bytes": 536870912,
                "errors": [],
                "duration_seconds": 2.5,
                "timestamp": "2025-11-10T12:00:00Z",
                "files": None,
            }
        }


class StorageQuota(BaseModel):
    """Storage quota configuration.

    Attributes:
        max_bytes: Maximum storage allowed in bytes
        warning_at_percent: Trigger warning at this percentage
        critical_at_percent: Trigger critical alert at this percentage
        auto_cleanup_enabled: Enable automatic cleanup
        auto_cleanup_policy: Policy for auto cleanup (oldest_first, largest_first)
        auto_cleanup_target_percent: Clean until usage is at this percentage

    Example:
        >>> quota = StorageQuota(
        ...     max_bytes=50 * 1024**3,
        ...     warning_at_percent=80,
        ...     critical_at_percent=95,
        ...     auto_cleanup_enabled=True,
        ...     auto_cleanup_policy="oldest_first",
        ...     auto_cleanup_target_percent=70
        ... )
    """

    max_bytes: int = Field(gt=0, description="Maximum storage in bytes")
    warning_at_percent: int = Field(ge=1, le=100, default=80, description="Warning threshold")
    critical_at_percent: int = Field(ge=1, le=100, default=95, description="Critical threshold")
    auto_cleanup_enabled: bool = Field(default=False, description="Enable auto cleanup")
    auto_cleanup_policy: str = Field(
        default="oldest_first",
        description="Cleanup policy (oldest_first, largest_first)",
    )
    auto_cleanup_target_percent: int = Field(
        ge=1, le=100, default=70, description="Target usage after cleanup"
    )

    @field_validator("critical_at_percent")
    @classmethod
    def validate_critical_threshold(cls, v: int, info: Any) -> int:
        """Ensure critical threshold is higher than warning."""
        if "warning_at_percent" in info.data and v <= info.data["warning_at_percent"]:
            raise ValueError("critical_at_percent must be > warning_at_percent")
        return v

    @field_validator("auto_cleanup_target_percent")
    @classmethod
    def validate_target_percent(cls, v: int, info: Any) -> int:
        """Ensure target is below warning threshold."""
        if "warning_at_percent" in info.data and v >= info.data["warning_at_percent"]:
            raise ValueError("auto_cleanup_target_percent must be < warning_at_percent")
        return v

    class Config:
        json_schema_extra = {
            "example": {
                "max_bytes": 53687091200,
                "warning_at_percent": 80,
                "critical_at_percent": 95,
                "auto_cleanup_enabled": True,
                "auto_cleanup_policy": "oldest_first",
                "auto_cleanup_target_percent": 70,
            }
        }


class QuotaStatus(BaseModel):
    """Current quota status.

    Attributes:
        quota: The configured quota settings
        current_bytes: Current storage usage
        percent_used: Percentage of quota used
        status: Current status (ok, warning, critical, exceeded)
        available_bytes: Space remaining
        needs_cleanup: Whether cleanup is recommended

    Example:
        >>> status = QuotaStatus(
        ...     quota=StorageQuota(...),
        ...     current_bytes=43046721536,
        ...     percent_used=80.2,
        ...     status="warning",
        ...     available_bytes=10640369664,
        ...     needs_cleanup=True
        ... )
    """

    quota: StorageQuota = Field(description="Quota configuration")
    current_bytes: int = Field(ge=0, description="Current usage in bytes")
    percent_used: float = Field(ge=0, description="Percentage used")
    status: str = Field(description="Status: ok, warning, critical, exceeded")
    available_bytes: int = Field(description="Space remaining in bytes")
    needs_cleanup: bool = Field(description="Cleanup recommended")

    class Config:
        json_schema_extra = {
            "example": {
                "quota": {
                    "max_bytes": 53687091200,
                    "warning_at_percent": 80,
                    "critical_at_percent": 95,
                },
                "current_bytes": 43046721536,
                "percent_used": 80.2,
                "status": "warning",
                "available_bytes": 10640369664,
                "needs_cleanup": True,
            }
        }
