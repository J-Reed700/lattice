"""Storage Management API Endpoints

FastAPI routes for storage analysis, cleanup operations, and quota management.
"""

from __future__ import annotations

from fastapi import APIRouter, Depends, HTTPException, Query, status
from sqlalchemy import insert, select
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.dependencies import get_db
from src.auth.dependencies import get_current_active_user
from src.auth.models import User
from src.config import Settings, get_settings
from src.middleware.csrf import csrf_protect
from src.models import CleanupAuditLog, StorageQuotaModel, StorageStat
from src.modules.storage_manager import (
    CleanupError,
    CleanupOperation,
    CleanupRequest,
    CleanupResult,
    QuotaStatus,
    StorageAnalyzer,
    StorageCleanup,
    StorageQuota,
    StorageQuotaManager,
    StorageStats,
)
from src.modules.storage_manager.config import DEFAULT_CONFIG

router = APIRouter(prefix="/storage", tags=["storage"])


@router.get("/stats", response_model=StorageStats)
async def get_storage_stats(
    refresh: bool = Query(False, description="Force refresh instead of cache"),
    include_breakdown: bool = Query(True, description="Include category breakdown"),
    include_trends: bool = Query(False, description="Include growth trends"),
    include_largest: bool = Query(True, description="Include largest files"),
    current_user: User = Depends(get_current_active_user),
    settings: Settings = Depends(get_settings),
    db: AsyncSession = Depends(get_db),
) -> StorageStats:
    """Get comprehensive storage statistics.

    Returns detailed storage usage across all categories including:
    - Total disk usage
    - Breakdown by category (files, embeddings, database, etc.)
    - Breakdown by file type
    - Breakdown by date ranges
    - Largest files
    - Growth trends (if requested)

    Args:
        refresh: Force recalculation instead of using cache
        include_breakdown: Include detailed category breakdown
        include_trends: Include historical growth data
        include_largest: Include list of largest files
        settings: Application settings
        db: Database session

    Returns:
        Complete storage statistics

    Example:
        ```
        GET /api/v1/storage/stats?refresh=true&include_trends=true
        ```
    """
    try:
        analyzer = StorageAnalyzer(settings, DEFAULT_CONFIG, db)
        stats = await analyzer.get_storage_stats(
            refresh=refresh,
            include_breakdown=include_breakdown,
            include_trends=include_trends,
            include_largest=include_largest,
        )

        await db.execute(
            insert(StorageStat).values(
                total_bytes=stats.total_bytes,
                original_files_bytes=stats.breakdown.original_files,
                embeddings_bytes=stats.breakdown.embeddings,
                database_bytes=stats.breakdown.database,
                thumbnails_bytes=stats.breakdown.thumbnails,
                cache_bytes=stats.breakdown.cache,
                logs_bytes=stats.breakdown.logs,
                by_file_type=stats.by_file_type,
                by_date_range=stats.by_date,
                file_count=0,
            )
        )
        await db.commit()

        return stats

    except Exception as e:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to get storage stats: {e!s}",
        ) from e


@router.get("/breakdown", response_model=StorageStats)
async def get_storage_breakdown(
    current_user: User = Depends(get_current_active_user),
    settings: Settings = Depends(get_settings),
    db: AsyncSession = Depends(get_db),
) -> StorageStats:
    """Get storage breakdown by category.

    Simplified endpoint that returns just the category breakdown.

    Example:
        ```
        GET /api/v1/storage/breakdown
        ```
    """
    try:
        analyzer = StorageAnalyzer(settings, DEFAULT_CONFIG, db)
        return await analyzer.get_storage_stats(include_breakdown=True)
    except Exception as e:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to get storage breakdown: {e!s}",
        ) from e


@router.get("/largest-files")
async def get_largest_files(
    limit: int = Query(100, ge=1, le=1000, description="Number of files to return"),
    current_user: User = Depends(get_current_active_user),
    settings: Settings = Depends(get_settings),
    db: AsyncSession = Depends(get_db),
):
    """Get largest files by size.

    Args:
        limit: Maximum number of files to return (1-1000)

    Returns:
        List of largest files with metadata

    Example:
        ```
        GET /api/v1/storage/largest-files?limit=50
        ```
    """
    try:
        analyzer = StorageAnalyzer(settings, DEFAULT_CONFIG, db)
        stats = await analyzer.get_storage_stats(include_largest=True)
        return {"files": stats.largest_files[:limit]}
    except Exception as e:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to get largest files: {e!s}",
        ) from e


@router.post("/cleanup", response_model=CleanupResult)
async def perform_cleanup(
    request: CleanupRequest,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    settings: Settings = Depends(get_settings),
    db: AsyncSession = Depends(get_db),
) -> CleanupResult:
    """Perform cleanup operation.

    Supports various cleanup operations:
    - `orphaned_files`: Remove files in storage but not in database
    - `cache`: Clear cache directory
    - `thumbnails`: Delete generated thumbnails
    - `old_logs`: Remove old log files
    - `vacuum_db`: Vacuum database to reclaim space
    - `old_embeddings`: Delete old embedding vectors

    Args:
        request: Cleanup request with operation type and filters

    Returns:
        Result with files affected and space reclaimed

    Example:
        ```json
        POST /api/v1/storage/cleanup
        {
            "operation": "orphaned_files",
            "dry_run": true,
            "older_than_days": 30
        }
        ```
    """
    try:
        cleanup = StorageCleanup(settings, DEFAULT_CONFIG, db)
        result = await cleanup.cleanup(request)

        await db.execute(
            insert(CleanupAuditLog).values(
                operation=result.operation.value,
                dry_run=result.dry_run,
                files_affected=result.files_affected,
                space_reclaimed_bytes=result.space_reclaimed_bytes,
                errors=result.errors,
                duration_seconds=result.duration_seconds,
                filters={
                    "older_than_days": request.older_than_days,
                    "file_types": request.file_types,
                    "min_size_bytes": request.min_size_bytes,
                },
            )
        )
        await db.commit()

        return result

    except CleanupError as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(e)) from e
    except Exception as e:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Cleanup failed: {e!s}",
        ) from e


@router.post("/vacuum", response_model=CleanupResult)
async def vacuum_database(
    dry_run: bool = Query(True, description="Preview mode"),
    current_user: User = Depends(get_current_active_user),
    settings: Settings = Depends(get_settings),
    db: AsyncSession = Depends(get_db),
) -> CleanupResult:
    """Vacuum database to reclaim space.

    Reclaims space from deleted records and optimizes database storage.

    Args:
        dry_run: If true, only estimate space to be reclaimed

    Example:
        ```
        POST /api/v1/storage/vacuum?dry_run=false
        ```
    """
    try:
        cleanup = StorageCleanup(settings, DEFAULT_CONFIG, db)
        request = CleanupRequest(operation=CleanupOperation.VACUUM_DB, dry_run=dry_run)
        result = await cleanup.cleanup(request)

        await db.execute(
            insert(CleanupAuditLog).values(
                operation=result.operation.value,
                dry_run=result.dry_run,
                files_affected=result.files_affected,
                space_reclaimed_bytes=result.space_reclaimed_bytes,
                errors=result.errors,
                duration_seconds=result.duration_seconds,
                filters={},
            )
        )
        await db.commit()

        return result

    except Exception as e:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Vacuum failed: {e!s}",
        ) from e


@router.post("/quota", response_model=QuotaStatus)
async def set_storage_quota(
    quota: StorageQuota,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    settings: Settings = Depends(get_settings),
    db: AsyncSession = Depends(get_db),
) -> QuotaStatus:
    """Set storage quota and get current status.

    Configures storage limits and auto-cleanup policies.

    Args:
        quota: Quota configuration

    Returns:
        Current quota status

    Example:
        ```json
        POST /api/v1/storage/quota
        {
            "max_bytes": 53687091200,
            "warning_at_percent": 80,
            "critical_at_percent": 95,
            "auto_cleanup_enabled": true,
            "auto_cleanup_policy": "oldest_first",
            "auto_cleanup_target_percent": 70
        }
        ```
    """
    try:
        await db.execute(
            insert(StorageQuotaModel).values(
                max_bytes=quota.max_bytes,
                warning_at_percent=quota.warning_at_percent,
                critical_at_percent=quota.critical_at_percent,
                auto_cleanup_enabled=quota.auto_cleanup_enabled,
                auto_cleanup_policy=quota.auto_cleanup_policy,
                auto_cleanup_target_percent=quota.auto_cleanup_target_percent,
                is_active=True,
            )
        )
        await db.commit()

        quota_mgr = StorageQuotaManager(settings, DEFAULT_CONFIG, db)
        await quota_mgr.set_quota(
            max_bytes=quota.max_bytes,
            warning_at_percent=quota.warning_at_percent,
            critical_at_percent=quota.critical_at_percent,
            auto_cleanup_enabled=quota.auto_cleanup_enabled,
            auto_cleanup_policy=quota.auto_cleanup_policy,
            auto_cleanup_target_percent=quota.auto_cleanup_target_percent,
        )

        return await quota_mgr.get_quota_status()

    except Exception as e:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to set quota: {e!s}",
        ) from e


@router.get("/quota/status", response_model=QuotaStatus)
async def get_quota_status(
    current_user: User = Depends(get_current_active_user),
    settings: Settings = Depends(get_settings),
    db: AsyncSession = Depends(get_db),
) -> QuotaStatus:
    """Get current quota status.

    Returns current storage usage relative to configured quota.

    Example:
        ```
        GET /api/v1/storage/quota/status
        ```
    """
    try:
        quota_mgr = StorageQuotaManager(settings, DEFAULT_CONFIG, db)
        return await quota_mgr.get_quota_status()
    except Exception as e:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to get quota status: {e!s}",
        ) from e


@router.get("/trends")
async def get_storage_trends(
    days: int = Query(30, ge=7, le=365, description="Number of days of history"),
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
):
    """Get historical storage growth trends.

    Args:
        days: Number of days of historical data to return

    Returns:
        List of historical storage data points

    Example:
        ```
        GET /api/v1/storage/trends?days=90
        ```
    """
    try:
        result = await db.execute(
            select(StorageStat).order_by(StorageStat.timestamp.desc()).limit(days)
        )
        stats = result.scalars().all()

        return {
            "trends": [
                {
                    "timestamp": stat.timestamp.isoformat(),
                    "total_bytes": stat.total_bytes,
                    "breakdown": {
                        "original_files": stat.original_files_bytes,
                        "embeddings": stat.embeddings_bytes,
                        "database": stat.database_bytes,
                        "thumbnails": stat.thumbnails_bytes,
                        "cache": stat.cache_bytes,
                        "logs": stat.logs_bytes,
                    },
                }
                for stat in reversed(stats)
            ]
        }

    except Exception as e:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to get storage trends: {e!s}",
        ) from e


@router.get("/audit-logs")
async def get_cleanup_audit_logs(
    limit: int = Query(100, ge=1, le=1000, description="Number of logs to return"),
    operation: str | None = Query(None, description="Filter by operation type"),
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
):
    """Get cleanup operation audit logs.

    Args:
        limit: Maximum number of logs to return
        operation: Filter by specific operation type

    Returns:
        List of cleanup audit log entries

    Example:
        ```
        GET /api/v1/storage/audit-logs?limit=50&operation=orphaned_files
        ```
    """
    try:
        query = select(CleanupAuditLog).order_by(CleanupAuditLog.timestamp.desc())

        if operation:
            query = query.where(CleanupAuditLog.operation == operation)

        query = query.limit(limit)

        result = await db.execute(query)
        logs = result.scalars().all()

        return {
            "logs": [
                {
                    "id": str(log.id),
                    "operation": log.operation,
                    "dry_run": log.dry_run,
                    "files_affected": log.files_affected,
                    "space_reclaimed_bytes": log.space_reclaimed_bytes,
                    "errors": log.errors,
                    "duration_seconds": log.duration_seconds,
                    "timestamp": log.timestamp.isoformat(),
                }
                for log in logs
            ]
        }

    except Exception as e:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to get audit logs: {e!s}",
        ) from e
