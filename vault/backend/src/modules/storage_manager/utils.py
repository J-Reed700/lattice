"""
Storage Manager Utility Functions

Convenience functions for common storage management operations.
"""

from sqlalchemy.ext.asyncio import AsyncSession

from src.config import Settings, get_settings

from .config import DEFAULT_CONFIG, StorageManagerConfig
from .core import StorageAnalyzer, StorageCleanup, StorageQuotaManager
from .models import (
    CleanupOperation,
    CleanupRequest,
    CleanupResult,
    QuotaStatus,
    StorageStats,
)


async def get_storage_stats(
    settings: Settings | None = None,
    config: StorageManagerConfig | None = None,
    db_session: AsyncSession | None = None,
    refresh: bool = False,
) -> StorageStats:
    """Get storage statistics.

    Convenience function for quick storage analysis.

    Args:
        settings: Application settings (uses default if not provided)
        config: Storage manager config (uses default if not provided)
        db_session: Database session
        refresh: Force refresh instead of using cache

    Returns:
        Complete storage statistics

    Example:
        >>> stats = await get_storage_stats()
        >>> print(f"Total: {stats.total_bytes / 1024**3:.2f} GB")
    """
    settings = settings or get_settings()
    config = config or DEFAULT_CONFIG

    analyzer = StorageAnalyzer(settings, config, db_session)
    return await analyzer.get_storage_stats(refresh=refresh)


async def cleanup_orphaned_files(
    settings: Settings | None = None,
    config: StorageManagerConfig | None = None,
    db_session: AsyncSession | None = None,
    dry_run: bool = True,
    older_than_days: int | None = None,
) -> CleanupResult:
    """Remove orphaned files (in storage but not in database).

    Args:
        settings: Application settings
        config: Storage manager config
        db_session: Database session
        dry_run: If True, only preview without deleting
        older_than_days: Only remove files older than N days

    Returns:
        Cleanup result with files affected and space reclaimed

    Example:
        >>> result = await cleanup_orphaned_files(dry_run=True)
        >>> print(f"Would remove {result.files_affected} files")
        >>> print(f"Would reclaim {result.space_reclaimed_bytes / 1024**2:.2f} MB")
    """
    settings = settings or get_settings()
    config = config or DEFAULT_CONFIG

    cleanup = StorageCleanup(settings, config, db_session)
    request = CleanupRequest(
        operation=CleanupOperation.ORPHANED_FILES,
        dry_run=dry_run,
        older_than_days=older_than_days,
    )

    return await cleanup.cleanup(request)


async def cleanup_cache(
    settings: Settings | None = None,
    config: StorageManagerConfig | None = None,
    db_session: AsyncSession | None = None,
    dry_run: bool = True,
    older_than_days: int | None = None,
) -> CleanupResult:
    """Clear cache directory.

    Args:
        settings: Application settings
        config: Storage manager config
        db_session: Database session
        dry_run: If True, only preview
        older_than_days: Only remove files older than N days

    Returns:
        Cleanup result

    Example:
        >>> result = await cleanup_cache(dry_run=False, older_than_days=7)
    """
    settings = settings or get_settings()
    config = config or DEFAULT_CONFIG

    cleanup = StorageCleanup(settings, config, db_session)
    request = CleanupRequest(
        operation=CleanupOperation.CACHE,
        dry_run=dry_run,
        older_than_days=older_than_days,
    )

    return await cleanup.cleanup(request)


async def cleanup_thumbnails(
    settings: Settings | None = None,
    config: StorageManagerConfig | None = None,
    db_session: AsyncSession | None = None,
    dry_run: bool = True,
) -> CleanupResult:
    """Delete generated thumbnails.

    Args:
        settings: Application settings
        config: Storage manager config
        db_session: Database session
        dry_run: If True, only preview

    Returns:
        Cleanup result

    Example:
        >>> result = await cleanup_thumbnails(dry_run=False)
    """
    settings = settings or get_settings()
    config = config or DEFAULT_CONFIG

    cleanup = StorageCleanup(settings, config, db_session)
    request = CleanupRequest(
        operation=CleanupOperation.THUMBNAILS,
        dry_run=dry_run,
    )

    return await cleanup.cleanup(request)


async def vacuum_database(
    settings: Settings | None = None,
    config: StorageManagerConfig | None = None,
    db_session: AsyncSession | None = None,
    dry_run: bool = True,
) -> CleanupResult:
    """Vacuum database to reclaim space.

    Args:
        settings: Application settings
        config: Storage manager config
        db_session: Database session
        dry_run: If True, only estimate

    Returns:
        Cleanup result

    Example:
        >>> result = await vacuum_database(dry_run=False)
    """
    settings = settings or get_settings()
    config = config or DEFAULT_CONFIG

    cleanup = StorageCleanup(settings, config, db_session)
    request = CleanupRequest(
        operation=CleanupOperation.VACUUM_DB,
        dry_run=dry_run,
    )

    return await cleanup.cleanup(request)


async def set_storage_quota(
    max_bytes: int,
    settings: Settings | None = None,
    config: StorageManagerConfig | None = None,
    db_session: AsyncSession | None = None,
    warning_at_percent: int = 80,
    critical_at_percent: int = 95,
    auto_cleanup_enabled: bool = False,
) -> QuotaStatus:
    """Set storage quota and get current status.

    Args:
        max_bytes: Maximum storage allowed in bytes
        settings: Application settings
        config: Storage manager config
        db_session: Database session
        warning_at_percent: Warning threshold
        critical_at_percent: Critical threshold
        auto_cleanup_enabled: Enable automatic cleanup

    Returns:
        Current quota status

    Example:
        >>> status = await set_storage_quota(
        ...     max_bytes=50 * 1024**3,  # 50GB
        ...     warning_at_percent=80
        ... )
        >>> print(status.status)  # "ok", "warning", "critical", "exceeded"
    """
    settings = settings or get_settings()
    config = config or DEFAULT_CONFIG

    quota_mgr = StorageQuotaManager(settings, config, db_session)
    await quota_mgr.set_quota(
        max_bytes=max_bytes,
        warning_at_percent=warning_at_percent,
        critical_at_percent=critical_at_percent,
        auto_cleanup_enabled=auto_cleanup_enabled,
    )

    return await quota_mgr.get_quota_status()


async def check_quota_status(
    settings: Settings | None = None,
    config: StorageManagerConfig | None = None,
    db_session: AsyncSession | None = None,
) -> QuotaStatus:
    """Check current quota status.

    Args:
        settings: Application settings
        config: Storage manager config
        db_session: Database session

    Returns:
        Current quota status

    Example:
        >>> status = await check_quota_status()
        >>> if status.needs_cleanup:
        ...     print(f"Storage at {status.percent_used:.1f}% - cleanup needed")
    """
    settings = settings or get_settings()
    config = config or DEFAULT_CONFIG

    quota_mgr = StorageQuotaManager(settings, config, db_session)
    return await quota_mgr.get_quota_status()


def format_bytes(bytes_value: int) -> str:
    """Format bytes as human-readable string.

    Args:
        bytes_value: Number of bytes

    Returns:
        Formatted string (e.g., "1.5 GB", "500 MB")

    Example:
        >>> format_bytes(1536000000)
        '1.43 GB'
        >>> format_bytes(500000)
        '488.28 KB'
    """
    for unit in ["B", "KB", "MB", "GB", "TB"]:
        if bytes_value < 1024.0:
            return f"{bytes_value:.2f} {unit}"
        bytes_value /= 1024.0
    return f"{bytes_value:.2f} PB"
