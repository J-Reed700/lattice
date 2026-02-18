"""Storage Management Background Tasks

Celery tasks for periodic storage monitoring, cleanup, and quota management.
"""

import asyncio
from datetime import datetime, timedelta
from typing import Any

from celery import shared_task
from sqlalchemy import delete, insert, select

from src.config import get_settings, session_context
from src.models import CleanupAuditLog, StorageQuotaModel, StorageStat
from src.modules.storage_manager import (
    CleanupOperation,
    CleanupRequest,
    StorageAnalyzer,
    StorageCleanup,
    StorageQuotaManager,
)
from src.modules.storage_manager.config import DEFAULT_CONFIG


@shared_task(name="storage.calculate_daily_stats")
def calculate_daily_storage_stats() -> dict[str, Any]:
    """Calculate and store daily storage statistics.

    Runs daily at 2:00 AM to update storage trends.

    Returns:
        Dictionary with calculation results
    """

    async def _calculate() -> dict[str, Any]:
        settings = get_settings()

        async with session_context() as session:
            analyzer = StorageAnalyzer(settings, DEFAULT_CONFIG, session)
            stats = await analyzer.get_storage_stats(
                refresh=True,
                include_breakdown=True,
                include_trends=False,
                include_largest=False,
            )

            await session.execute(
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

            result = await session.execute(
                select(StorageStat).order_by(StorageStat.timestamp.desc()).limit(1)
            )
            latest_stat = result.scalar_one_or_none()

            await session.commit()

            return {
                "status": "success",
                "total_bytes": stats.total_bytes,
                "timestamp": stats.last_calculated.isoformat(),
                "stat_id": str(latest_stat.id) if latest_stat else None,
            }

    return asyncio.run(_calculate())


@shared_task(name="storage.detect_orphaned_files")
def detect_orphaned_files_task() -> dict[str, Any]:
    """Detect orphaned files (in storage but not in database).

    Runs weekly on Sunday at 3:00 AM.

    Returns:
        Dictionary with detection results
    """

    async def _detect() -> dict[str, Any]:
        settings = get_settings()

        async with session_context() as session:
            cleanup = StorageCleanup(settings, DEFAULT_CONFIG, session)

            request = CleanupRequest(
                operation=CleanupOperation.ORPHANED_FILES,
                dry_run=True,
                older_than_days=None,
            )

            result = await cleanup.cleanup(request)

            await session.execute(
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
            await session.commit()

            return {
                "status": "success",
                "orphaned_files_found": result.files_affected,
                "potential_space_reclaim_bytes": result.space_reclaimed_bytes,
                "timestamp": result.timestamp.isoformat(),
            }

    return asyncio.run(_detect())


@shared_task(name="storage.monthly_cleanup")
def monthly_cleanup_task() -> dict[str, Any]:
    """Perform monthly cleanup operations.

    Runs on the 1st of each month at 4:00 AM.
    - Vacuum database
    - Remove old cache files (>30 days)
    - Remove old log files (>90 days)
    - Clean up old storage stats (>1 year)

    Returns:
        Dictionary with cleanup results
    """

    async def _cleanup() -> dict[str, Any]:
        settings = get_settings()
        results = {}

        async with session_context() as session:
            cleanup = StorageCleanup(settings, DEFAULT_CONFIG, session)

            vacuum_request = CleanupRequest(
                operation=CleanupOperation.VACUUM_DB,
                dry_run=False,
            )
            vacuum_result = await cleanup.cleanup(vacuum_request)
            results["vacuum"] = {
                "space_reclaimed_bytes": vacuum_result.space_reclaimed_bytes,
                "duration_seconds": vacuum_result.duration_seconds,
            }

            cache_request = CleanupRequest(
                operation=CleanupOperation.CACHE,
                dry_run=False,
                older_than_days=30,
            )
            cache_result = await cleanup.cleanup(cache_request)
            results["cache"] = {
                "files_removed": cache_result.files_affected,
                "space_reclaimed_bytes": cache_result.space_reclaimed_bytes,
            }

            logs_request = CleanupRequest(
                operation=CleanupOperation.OLD_LOGS,
                dry_run=False,
                older_than_days=90,
            )
            logs_result = await cleanup.cleanup(logs_request)
            results["logs"] = {
                "files_removed": logs_result.files_affected,
                "space_reclaimed_bytes": logs_result.space_reclaimed_bytes,
            }

            one_year_ago = datetime.now() - timedelta(days=365)
            await session.execute(delete(StorageStat).where(StorageStat.timestamp < one_year_ago))

            ninety_days_ago = datetime.now() - timedelta(days=90)
            await session.execute(
                delete(CleanupAuditLog).where(CleanupAuditLog.timestamp < ninety_days_ago)
            )

            await session.commit()

            results["status"] = "success"
            results["timestamp"] = datetime.now().isoformat()

            return results

    return asyncio.run(_cleanup())


@shared_task(name="storage.check_quota")
def check_storage_quota() -> dict[str, Any]:
    """Check storage quota and trigger alerts if needed.

    Runs every hour to monitor quota status.

    Returns:
        Dictionary with quota status
    """

    async def _check() -> dict[str, Any]:
        settings = get_settings()

        async with session_context() as session:
            quota_mgr = StorageQuotaManager(settings, DEFAULT_CONFIG, session)
            status = await quota_mgr.get_quota_status()

            return {
                "status": status.status,
                "current_bytes": status.current_bytes,
                "percent_used": status.percent_used,
                "available_bytes": status.available_bytes,
                "needs_cleanup": status.needs_cleanup,
                "timestamp": datetime.now().isoformat(),
            }

    return asyncio.run(_check())


@shared_task(name="storage.auto_cleanup")
def auto_cleanup_task() -> dict[str, Any]:
    """Perform automatic cleanup when quota exceeded.

    Triggered when storage quota is exceeded and auto-cleanup is enabled.

    Returns:
        Dictionary with cleanup results
    """

    async def _auto_cleanup() -> dict[str, Any]:
        settings = get_settings()

        async with session_context() as session:
            result = await session.execute(
                select(StorageQuotaModel)
                .where(StorageQuotaModel.is_active == True)
                .order_by(StorageQuotaModel.created_at.desc())
                .limit(1)
            )
            quota_model = result.scalar_one_or_none()

            if not quota_model or not quota_model.auto_cleanup_enabled:
                return {
                    "status": "skipped",
                    "reason": "Auto-cleanup not enabled",
                }

            quota_mgr = StorageQuotaManager(settings, DEFAULT_CONFIG, session)
            status = await quota_mgr.get_quota_status()

            if not status.needs_cleanup:
                return {
                    "status": "skipped",
                    "reason": "No cleanup needed",
                    "percent_used": status.percent_used,
                }

            cleanup = StorageCleanup(settings, DEFAULT_CONFIG, session)
            results = {}

            if quota_model.auto_cleanup_policy == "oldest_first":
                cache_request = CleanupRequest(
                    operation=CleanupOperation.CACHE,
                    dry_run=False,
                    older_than_days=30,
                )
                cache_result = await cleanup.cleanup(cache_request)
                results["cache_cleanup"] = {
                    "files_removed": cache_result.files_affected,
                    "space_reclaimed_bytes": cache_result.space_reclaimed_bytes,
                }

                thumbnails_request = CleanupRequest(
                    operation=CleanupOperation.THUMBNAILS,
                    dry_run=False,
                )
                thumbnails_result = await cleanup.cleanup(thumbnails_request)
                results["thumbnails_cleanup"] = {
                    "files_removed": thumbnails_result.files_affected,
                    "space_reclaimed_bytes": thumbnails_result.space_reclaimed_bytes,
                }

            status_after = await quota_mgr.get_quota_status()

            results["status"] = "success"
            results["percent_used_before"] = status.percent_used
            results["percent_used_after"] = status_after.percent_used
            results["total_space_reclaimed"] = status.current_bytes - status_after.current_bytes
            results["timestamp"] = datetime.now().isoformat()

            await session.commit()

            return results

    return asyncio.run(_auto_cleanup())


@shared_task(name="storage.generate_report")
def generate_storage_report() -> dict[str, Any]:
    """Generate weekly storage report.

    Runs weekly on Monday at 9:00 AM to generate summary report.

    Returns:
        Dictionary with report data
    """

    async def _generate_report() -> dict[str, Any]:
        get_settings()

        async with session_context() as session:
            seven_days_ago = datetime.now() - timedelta(days=7)

            result = await session.execute(
                select(StorageStat)
                .where(StorageStat.timestamp >= seven_days_ago)
                .order_by(StorageStat.timestamp.asc())
            )
            stats = result.scalars().all()

            if not stats:
                return {
                    "status": "no_data",
                    "message": "No storage stats available for the past week",
                }

            first_stat = stats[0]
            last_stat = stats[-1]

            growth = last_stat.total_bytes - first_stat.total_bytes
            growth_percent = (
                (growth / first_stat.total_bytes * 100) if first_stat.total_bytes > 0 else 0
            )

            result_audit = await session.execute(
                select(CleanupAuditLog)
                .where(CleanupAuditLog.timestamp >= seven_days_ago)
                .where(CleanupAuditLog.dry_run == False)
            )
            cleanup_logs = result_audit.scalars().all()

            total_cleaned_files = sum(log.files_affected for log in cleanup_logs)
            total_space_reclaimed = sum(log.space_reclaimed_bytes for log in cleanup_logs)

            return {
                "status": "success",
                "period": {
                    "start": first_stat.timestamp.isoformat(),
                    "end": last_stat.timestamp.isoformat(),
                },
                "storage": {
                    "start_bytes": first_stat.total_bytes,
                    "end_bytes": last_stat.total_bytes,
                    "growth_bytes": growth,
                    "growth_percent": round(growth_percent, 2),
                },
                "cleanup_summary": {
                    "operations": len(cleanup_logs),
                    "files_cleaned": total_cleaned_files,
                    "space_reclaimed_bytes": total_space_reclaimed,
                },
                "current_breakdown": {
                    "original_files": last_stat.original_files_bytes,
                    "embeddings": last_stat.embeddings_bytes,
                    "database": last_stat.database_bytes,
                    "thumbnails": last_stat.thumbnails_bytes,
                    "cache": last_stat.cache_bytes,
                    "logs": last_stat.logs_bytes,
                },
                "timestamp": datetime.now().isoformat(),
            }

    return asyncio.run(_generate_report())
