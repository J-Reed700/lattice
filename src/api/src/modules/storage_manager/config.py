"""
Storage Manager Configuration

Configuration settings with defaults for the storage management module.
"""

from pydantic import BaseModel, Field


class StorageManagerConfig(BaseModel):
    """Storage manager configuration.

    Attributes:
        cache_ttl_seconds: How long to cache storage stats
        max_files_per_scan: Maximum files to scan in single operation
        cleanup_batch_size: Number of files to process per batch
        trend_retention_days: How many days of historical data to keep
        default_quota_bytes: Default storage quota
        warning_threshold_percent: Default warning threshold
        critical_threshold_percent: Default critical threshold
        auto_cleanup_enabled: Enable automatic cleanup by default
        backup_before_cleanup: Create backup before destructive operations
        audit_log_retention_days: How long to keep audit logs
        parallel_scan_workers: Number of parallel workers for scanning
        skip_patterns: File patterns to skip during scanning

    Example:
        >>> config = StorageManagerConfig()
        >>> print(f"Cache TTL: {config.cache_ttl_seconds}s")
    """

    cache_ttl_seconds: int = Field(default=300, ge=60, le=3600, description="Cache TTL in seconds")
    max_files_per_scan: int = Field(default=1000000, ge=1000, description="Max files per scan")
    cleanup_batch_size: int = Field(default=1000, ge=10, le=10000, description="Cleanup batch size")
    trend_retention_days: int = Field(
        default=365, ge=30, le=730, description="Trend data retention days"
    )
    default_quota_bytes: int = Field(
        default=50 * 1024 * 1024 * 1024, gt=0, description="Default quota (50GB)"
    )
    warning_threshold_percent: int = Field(
        default=80, ge=50, le=100, description="Warning threshold"
    )
    critical_threshold_percent: int = Field(
        default=95, ge=50, le=100, description="Critical threshold"
    )
    auto_cleanup_enabled: bool = Field(default=False, description="Auto cleanup")
    backup_before_cleanup: bool = Field(default=True, description="Backup before cleanup")
    audit_log_retention_days: int = Field(
        default=90, ge=30, le=365, description="Audit log retention"
    )
    parallel_scan_workers: int = Field(default=4, ge=1, le=16, description="Parallel scan workers")
    skip_patterns: list[str] = Field(
        default_factory=lambda: [
            ".git",
            ".git/*",
            "node_modules",
            "node_modules/*",
            "__pycache__",
            "__pycache__/*",
            "*.pyc",
            "*.pyo",
            "*.pyd",
            ".DS_Store",
            "Thumbs.db",
            "desktop.ini",
            "*.tmp",
            "*.temp",
            "*.swp",
            "*.swx",
            "~*",
            ".~*",
        ],
        description="Patterns to skip",
    )

    class Config:
        json_schema_extra = {
            "example": {
                "cache_ttl_seconds": 300,
                "max_files_per_scan": 1000000,
                "cleanup_batch_size": 1000,
                "trend_retention_days": 365,
                "default_quota_bytes": 53687091200,
                "warning_threshold_percent": 80,
                "critical_threshold_percent": 95,
                "auto_cleanup_enabled": False,
                "backup_before_cleanup": True,
                "audit_log_retention_days": 90,
                "parallel_scan_workers": 4,
                "skip_patterns": [".git", "node_modules", "__pycache__"],
            }
        }


DEFAULT_CONFIG = StorageManagerConfig()
