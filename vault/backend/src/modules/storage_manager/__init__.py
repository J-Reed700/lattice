"""
Storage Manager Module

A self-contained module for comprehensive disk usage tracking and cleanup operations.
See README.md for full contract specification.

Basic Usage:
    >>> from storage_manager import get_storage_stats, cleanup_orphaned_files
    >>> stats = await get_storage_stats()
    >>> print(f"Total: {stats.total_bytes / 1024**3:.2f} GB")
    >>> result = await cleanup_orphaned_files(dry_run=True)
"""

from .core import StorageAnalyzer, StorageCleanup, StorageQuotaManager
from .exceptions import (
    CleanupError,
    QuotaExceededError,
    StorageAnalysisError,
    StorageError,
)
from .models import (
    CleanupOperation,
    CleanupRequest,
    CleanupResult,
    FileInfo,
    QuotaStatus,
    StorageBreakdown,
    StorageQuota,
    StorageStats,
)
from .utils import (
    check_quota_status,
    cleanup_orphaned_files,
    get_storage_stats,
    set_storage_quota,
)

__all__ = [
    "CleanupError",
    "CleanupOperation",
    "CleanupRequest",
    "CleanupResult",
    "FileInfo",
    "QuotaExceededError",
    "QuotaStatus",
    "StorageAnalysisError",
    "StorageAnalyzer",
    "StorageBreakdown",
    "StorageCleanup",
    "StorageError",
    "StorageQuota",
    "StorageQuotaManager",
    "StorageStats",
    "check_quota_status",
    "cleanup_orphaned_files",
    "get_storage_stats",
    "set_storage_quota",
]
