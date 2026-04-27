"""
Storage Manager Core Implementation

Main implementation of storage analysis, cleanup, and quota management.
"""

from datetime import datetime, timedelta
from pathlib import Path
import time

from sqlalchemy import func, select
from sqlalchemy.ext.asyncio import AsyncSession

from src.config import Settings
from src.models import File, ImageEmbedding, TextEmbedding

from .config import DEFAULT_CONFIG, StorageManagerConfig
from .exceptions import (
    CleanupError,
    PermissionDeniedError,
    QuotaExceededError,
    StorageAnalysisError,
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
    TrendPoint,
)


class StorageAnalyzer:
    """Analyzes disk usage and provides detailed storage statistics.

    This class handles all storage analysis operations including:
    - Total disk usage calculation
    - Breakdown by category (files, embeddings, database, etc.)
    - Breakdown by file type
    - Breakdown by date ranges
    - Identification of largest files
    - Historical growth trends

    Args:
        settings: Application settings
        config: Storage manager configuration
        db_session: Database session for querying metadata

    Example:
        >>> analyzer = StorageAnalyzer(settings, config, db_session)
        >>> stats = await analyzer.get_storage_stats(refresh=True)
        >>> print(f"Total: {stats.total_bytes / 1024**3:.2f} GB")
    """

    def __init__(
        self,
        settings: Settings,
        config: StorageManagerConfig | None = None,
        db_session: AsyncSession | None = None,
    ) -> None:
        self.settings = settings
        self.config = config or DEFAULT_CONFIG
        self.db_session = db_session
        self._cache: StorageStats | None = None
        self._cache_time: float = 0

    async def get_storage_stats(
        self,
        refresh: bool = False,
        include_breakdown: bool = True,
        include_trends: bool = False,
        include_largest: bool = True,
    ) -> StorageStats:
        """Get comprehensive storage statistics.

        Args:
            refresh: Force recalculation instead of using cache
            include_breakdown: Include category breakdown
            include_trends: Include historical growth trends
            include_largest: Include list of largest files

        Returns:
            Complete storage statistics

        Raises:
            StorageAnalysisError: If analysis fails

        Example:
            >>> stats = await analyzer.get_storage_stats(refresh=True)
            >>> print(stats.breakdown.original_files)
        """
        if not refresh and self._is_cache_valid():
            if self._cache:
                return self._cache

        try:
            breakdown = await self._calculate_breakdown() if include_breakdown else None
            by_file_type = await self._calculate_by_file_type()
            by_date = await self._calculate_by_date()
            largest_files = await self._get_largest_files(100) if include_largest else []
            growth_trend = await self._get_growth_trend() if include_trends else []

            if not breakdown:
                breakdown = StorageBreakdown(
                    original_files=0,
                    embeddings=0,
                    database=0,
                    thumbnails=0,
                    cache=0,
                    logs=0,
                )

            stats = StorageStats(
                total_bytes=breakdown.total,
                breakdown=breakdown,
                by_file_type=by_file_type,
                by_date=by_date,
                largest_files=largest_files,
                growth_trend=growth_trend,
                last_calculated=datetime.now(),
                cached=False,
            )

            self._cache = stats
            self._cache_time = time.time()

            return stats

        except Exception as e:
            raise StorageAnalysisError(f"Failed to analyze storage: {e}") from e

    async def _calculate_breakdown(self) -> StorageBreakdown:
        """Calculate storage breakdown by category."""
        breakdown = StorageBreakdown(
            original_files=await self._calculate_directory_size(
                self.settings.storage_documents_path
            )
            + await self._calculate_directory_size(self.settings.storage_screenshots_path),
            embeddings=await self._estimate_embeddings_size(),
            database=await self._calculate_database_size(),
            thumbnails=await self._calculate_directory_size(self.settings.storage_thumbnails_path),
            cache=await self._calculate_cache_size(),
            logs=await self._calculate_directory_size(Path(self.settings.log_file).parent),
        )
        return breakdown

    async def _calculate_directory_size(self, path: str | Path) -> int:
        """Calculate total size of all files in a directory recursively."""
        path = Path(path)
        if not path.exists():
            return 0

        try:
            total_size = 0
            for entry in path.rglob("*"):
                if entry.is_file():
                    try:
                        if not any(entry.match(pattern) for pattern in self.config.skip_patterns):
                            total_size += entry.stat().st_size
                    except (OSError, PermissionError):
                        continue
            return total_size
        except PermissionError as e:
            raise PermissionDeniedError(f"Cannot access directory {path}") from e

    async def _estimate_embeddings_size(self) -> int:
        """Estimate size of embedding vectors in database."""
        if not self.db_session:
            return 0

        try:
            text_count_result = await self.db_session.execute(
                select(func.count()).select_from(TextEmbedding)
            )
            text_count = text_count_result.scalar() or 0

            image_count_result = await self.db_session.execute(
                select(func.count()).select_from(ImageEmbedding)
            )
            image_count = image_count_result.scalar() or 0

            text_embedding_size = text_count * self.settings.text_embedding_dim * 4
            image_embedding_size = image_count * self.settings.image_embedding_dim * 4

            return text_embedding_size + image_embedding_size

        except Exception:
            return 0

    async def _calculate_database_size(self) -> int:
        """Calculate database file size."""
        db_url = self.settings.database_url

        if "postgresql" in db_url:
            if self.db_session:
                try:
                    result = await self.db_session.execute(
                        select(func.pg_database_size(func.current_database()))
                    )
                    return result.scalar() or 0
                except Exception:
                    return 0
            return 0
        if "sqlite" in db_url:
            db_path = db_url.replace("sqlite:///", "")
            path = Path(db_path)
            return path.stat().st_size if path.exists() else 0
        return 0

    async def _calculate_cache_size(self) -> int:
        """Calculate cache directory size."""
        cache_paths = [
            Path(self.settings.ml_cache_dir),
            Path(self.settings.upload_dir),
            Path(self.settings.storage_chunks_path),
        ]

        total = 0
        for cache_path in cache_paths:
            total += await self._calculate_directory_size(cache_path)
        return total

    async def _calculate_by_file_type(self) -> dict[str, int]:
        """Calculate storage grouped by file extension."""
        if not self.db_session:
            return {}

        try:
            result = await self.db_session.execute(
                select(File.extension, func.sum(File.size_bytes))
                .group_by(File.extension)
                .order_by(func.sum(File.size_bytes).desc())
            )

            return {ext: size for ext, size in result.all() if size}

        except Exception:
            return {}

    async def _calculate_by_date(self) -> dict[str, int]:
        """Calculate storage grouped by date ranges."""
        if not self.db_session:
            return {}

        now = datetime.now()
        ranges = {
            "this_week": now - timedelta(days=7),
            "this_month": now - timedelta(days=30),
            "this_quarter": now - timedelta(days=90),
            "this_year": now - timedelta(days=365),
            "older": now - timedelta(days=365 * 10),
        }

        result_dict: dict[str, int] = {}

        try:
            for range_name, since_date in ranges.items():
                result = await self.db_session.execute(
                    select(func.sum(File.size_bytes)).where(File.indexed_at >= since_date)
                )
                size = result.scalar() or 0
                result_dict[range_name] = size

            return result_dict

        except Exception:
            return {}

    async def _get_largest_files(self, limit: int = 100) -> list[FileInfo]:
        """Get the largest files by size."""
        if not self.db_session:
            return []

        try:
            result = await self.db_session.execute(
                select(File).order_by(File.size_bytes.desc()).limit(limit)
            )
            files = result.scalars().all()

            return [
                FileInfo(
                    path=file.path,
                    size_bytes=file.size_bytes,
                    modified_at=file.modified_at,
                    file_type=file.extension,
                    category=self._categorize_file(file.path),
                )
                for file in files
            ]

        except Exception:
            return []

    async def _get_growth_trend(self, days: int = 30) -> list[TrendPoint]:
        """Get historical storage growth trend."""
        return []

    def _categorize_file(self, path: str) -> str:
        """Categorize a file based on its path."""
        Path(path)

        if self.settings.storage_documents_path in str(
            path
        ) or self.settings.storage_screenshots_path in str(path):
            return "original_files"
        if self.settings.storage_thumbnails_path in str(path):
            return "thumbnails"
        if self.settings.storage_chunks_path in str(path):
            return "cache"
        return "other"

    def _is_cache_valid(self) -> bool:
        """Check if cached stats are still valid."""
        if not self._cache or not self._cache_time:
            return False

        age = time.time() - self._cache_time
        return age < self.config.cache_ttl_seconds


class StorageCleanup:
    """Performs cleanup operations to reclaim disk space.

    Supports various cleanup operations:
    - Remove orphaned files (in storage but not in DB)
    - Clear cache files
    - Delete old thumbnails
    - Remove old logs
    - Vacuum database
    - Delete old embeddings

    Args:
        settings: Application settings
        config: Storage manager configuration
        db_session: Database session

    Example:
        >>> cleanup = StorageCleanup(settings, config, db_session)
        >>> result = await cleanup.cleanup(CleanupRequest(
        ...     operation=CleanupOperation.ORPHANED_FILES,
        ...     dry_run=True
        ... ))
    """

    def __init__(
        self,
        settings: Settings,
        config: StorageManagerConfig | None = None,
        db_session: AsyncSession | None = None,
    ) -> None:
        self.settings = settings
        self.config = config or DEFAULT_CONFIG
        self.db_session = db_session

    async def cleanup(self, request: CleanupRequest) -> CleanupResult:
        """Perform cleanup operation.

        Args:
            request: Cleanup request with operation type and filters

        Returns:
            Result with files affected and space reclaimed

        Raises:
            CleanupError: If cleanup operation fails

        Example:
            >>> result = await cleanup.cleanup(CleanupRequest(
            ...     operation=CleanupOperation.CACHE,
            ...     dry_run=False
            ... ))
        """
        start_time = time.time()

        try:
            if request.operation == CleanupOperation.ORPHANED_FILES:
                result = await self._cleanup_orphaned_files(request)
            elif request.operation == CleanupOperation.CACHE:
                result = await self._cleanup_cache(request)
            elif request.operation == CleanupOperation.THUMBNAILS:
                result = await self._cleanup_thumbnails(request)
            elif request.operation == CleanupOperation.OLD_LOGS:
                result = await self._cleanup_old_logs(request)
            elif request.operation == CleanupOperation.VACUUM_DB:
                result = await self._vacuum_database(request)
            elif request.operation == CleanupOperation.OLD_EMBEDDINGS:
                result = await self._cleanup_old_embeddings(request)
            else:
                raise CleanupError(f"Unknown cleanup operation: {request.operation}")

            result.duration_seconds = time.time() - start_time
            result.timestamp = datetime.now()

            return result

        except Exception as e:
            raise CleanupError(f"Cleanup operation failed: {e}") from e

    async def _cleanup_orphaned_files(self, request: CleanupRequest) -> CleanupResult:
        """Remove files that exist in storage but not in database."""
        if not self.db_session:
            raise CleanupError("Database session required for orphan detection")

        orphaned_files: list[FileInfo] = []
        space_reclaimed = 0

        result = await self.db_session.execute(select(File.path))
        db_paths = {row[0] for row in result.all()}

        storage_dirs = [
            Path(self.settings.storage_documents_path),
            Path(self.settings.storage_screenshots_path),
        ]

        for storage_dir in storage_dirs:
            if not storage_dir.exists():
                continue

            for file_path in storage_dir.rglob("*"):
                if not file_path.is_file():
                    continue

                if str(file_path) not in db_paths:
                    try:
                        stat = file_path.stat()

                        if request.older_than_days:
                            file_age = datetime.now() - datetime.fromtimestamp(stat.st_mtime)
                            if file_age.days < request.older_than_days:
                                continue

                        if request.min_size_bytes and stat.st_size < request.min_size_bytes:
                            continue

                        file_info = FileInfo(
                            path=str(file_path),
                            size_bytes=stat.st_size,
                            modified_at=datetime.fromtimestamp(stat.st_mtime),
                            file_type=file_path.suffix.lstrip("."),
                            category="orphaned",
                        )

                        orphaned_files.append(file_info)
                        space_reclaimed += stat.st_size

                        if not request.dry_run:
                            file_path.unlink()

                    except (OSError, PermissionError):
                        continue

        return CleanupResult(
            operation=request.operation,
            dry_run=request.dry_run,
            files_affected=len(orphaned_files),
            space_reclaimed_bytes=space_reclaimed,
            errors=[],
            duration_seconds=0,
            timestamp=datetime.now(),
            files=orphaned_files if request.dry_run else None,
        )

    async def _cleanup_cache(self, request: CleanupRequest) -> CleanupResult:
        """Clear cache directory."""
        cache_paths = [
            Path(self.settings.upload_dir),
            Path(self.settings.storage_chunks_path),
        ]

        files_affected = 0
        space_reclaimed = 0
        errors: list[str] = []

        for cache_path in cache_paths:
            if not cache_path.exists():
                continue

            for file_path in cache_path.rglob("*"):
                if not file_path.is_file():
                    continue

                try:
                    size = file_path.stat().st_size

                    if request.older_than_days:
                        file_age = datetime.now() - datetime.fromtimestamp(
                            file_path.stat().st_mtime
                        )
                        if file_age.days < request.older_than_days:
                            continue

                    files_affected += 1
                    space_reclaimed += size

                    if not request.dry_run:
                        file_path.unlink()

                except (OSError, PermissionError) as e:
                    errors.append(f"Cannot delete {file_path}: {e}")

        return CleanupResult(
            operation=request.operation,
            dry_run=request.dry_run,
            files_affected=files_affected,
            space_reclaimed_bytes=space_reclaimed,
            errors=errors,
            duration_seconds=0,
            timestamp=datetime.now(),
        )

    async def _cleanup_thumbnails(self, request: CleanupRequest) -> CleanupResult:
        """Delete generated thumbnails."""
        thumbnail_path = Path(self.settings.storage_thumbnails_path)

        files_affected = 0
        space_reclaimed = 0

        if thumbnail_path.exists():
            for file_path in thumbnail_path.rglob("*"):
                if file_path.is_file():
                    try:
                        size = file_path.stat().st_size
                        files_affected += 1
                        space_reclaimed += size

                        if not request.dry_run:
                            file_path.unlink()
                    except (OSError, PermissionError):
                        continue

        return CleanupResult(
            operation=request.operation,
            dry_run=request.dry_run,
            files_affected=files_affected,
            space_reclaimed_bytes=space_reclaimed,
            errors=[],
            duration_seconds=0,
            timestamp=datetime.now(),
        )

    async def _cleanup_old_logs(self, request: CleanupRequest) -> CleanupResult:
        """Remove old log files."""
        log_dir = Path(self.settings.log_file).parent
        older_than_days = request.older_than_days or 30

        files_affected = 0
        space_reclaimed = 0

        if log_dir.exists():
            cutoff_date = datetime.now() - timedelta(days=older_than_days)

            for file_path in log_dir.rglob("*.log*"):
                if file_path.is_file():
                    try:
                        mtime = datetime.fromtimestamp(file_path.stat().st_mtime)

                        if mtime < cutoff_date:
                            size = file_path.stat().st_size
                            files_affected += 1
                            space_reclaimed += size

                            if not request.dry_run:
                                file_path.unlink()
                    except (OSError, PermissionError):
                        continue

        return CleanupResult(
            operation=request.operation,
            dry_run=request.dry_run,
            files_affected=files_affected,
            space_reclaimed_bytes=space_reclaimed,
            errors=[],
            duration_seconds=0,
            timestamp=datetime.now(),
        )

    async def _vacuum_database(self, request: CleanupRequest) -> CleanupResult:
        """Vacuum database to reclaim space."""
        if not self.db_session:
            raise CleanupError("Database session required for vacuum")

        space_before = 0
        space_after = 0

        db_url = self.settings.database_url

        try:
            if "postgresql" in db_url:
                result = await self.db_session.execute(
                    select(func.pg_database_size(func.current_database()))
                )
                space_before = result.scalar() or 0

                if not request.dry_run:
                    await self.db_session.execute("VACUUM ANALYZE")

                result = await self.db_session.execute(
                    select(func.pg_database_size(func.current_database()))
                )
                space_after = result.scalar() or 0

            elif "sqlite" in db_url:
                db_path = Path(db_url.replace("sqlite:///", ""))
                if db_path.exists():
                    space_before = db_path.stat().st_size

                    if not request.dry_run:
                        await self.db_session.execute("VACUUM")

                    space_after = db_path.stat().st_size if db_path.exists() else 0

            space_reclaimed = max(0, space_before - space_after)

            return CleanupResult(
                operation=request.operation,
                dry_run=request.dry_run,
                files_affected=1,
                space_reclaimed_bytes=space_reclaimed,
                errors=[],
                duration_seconds=0,
                timestamp=datetime.now(),
            )

        except Exception as e:
            raise CleanupError(f"Database vacuum failed: {e}") from e

    async def _cleanup_old_embeddings(self, request: CleanupRequest) -> CleanupResult:
        """Delete old embedding vectors."""
        return CleanupResult(
            operation=request.operation,
            dry_run=request.dry_run,
            files_affected=0,
            space_reclaimed_bytes=0,
            errors=["Not implemented yet"],
            duration_seconds=0,
            timestamp=datetime.now(),
        )


class StorageQuotaManager:
    """Manages storage quotas and alerts.

    Handles:
    - Setting and updating quota limits
    - Checking current quota status
    - Triggering alerts at warning/critical thresholds
    - Auto-cleanup when quota exceeded

    Args:
        settings: Application settings
        config: Storage manager configuration
        db_session: Database session

    Example:
        >>> quota_mgr = StorageQuotaManager(settings, config, db_session)
        >>> await quota_mgr.set_quota(
        ...     max_bytes=50 * 1024**3,
        ...     warning_at_percent=80
        ... )
    """

    def __init__(
        self,
        settings: Settings,
        config: StorageManagerConfig | None = None,
        db_session: AsyncSession | None = None,
    ) -> None:
        self.settings = settings
        self.config = config or DEFAULT_CONFIG
        self.db_session = db_session
        self._quota: StorageQuota | None = None

    async def set_quota(
        self,
        max_bytes: int,
        warning_at_percent: int = 80,
        critical_at_percent: int = 95,
        auto_cleanup_enabled: bool = False,
        auto_cleanup_policy: str = "oldest_first",
        auto_cleanup_target_percent: int = 70,
    ) -> StorageQuota:
        """Set storage quota.

        Args:
            max_bytes: Maximum storage allowed
            warning_at_percent: Warning threshold percentage
            critical_at_percent: Critical threshold percentage
            auto_cleanup_enabled: Enable automatic cleanup
            auto_cleanup_policy: Cleanup policy (oldest_first, largest_first)
            auto_cleanup_target_percent: Target usage after auto cleanup

        Returns:
            Configured storage quota

        Example:
            >>> quota = await quota_mgr.set_quota(
            ...     max_bytes=50 * 1024**3,
            ...     warning_at_percent=80
            ... )
        """
        quota = StorageQuota(
            max_bytes=max_bytes,
            warning_at_percent=warning_at_percent,
            critical_at_percent=critical_at_percent,
            auto_cleanup_enabled=auto_cleanup_enabled,
            auto_cleanup_policy=auto_cleanup_policy,
            auto_cleanup_target_percent=auto_cleanup_target_percent,
        )

        self._quota = quota
        return quota

    async def get_quota_status(self, current_bytes: int | None = None) -> QuotaStatus:
        """Get current quota status.

        Args:
            current_bytes: Current storage usage (calculated if not provided)

        Returns:
            Quota status with usage and recommendations

        Example:
            >>> status = await quota_mgr.get_quota_status()
            >>> print(status.status)  # "ok", "warning", "critical", "exceeded"
        """
        if not self._quota:
            self._quota = StorageQuota(
                max_bytes=self.config.default_quota_bytes,
                warning_at_percent=self.config.warning_threshold_percent,
                critical_at_percent=self.config.critical_threshold_percent,
            )

        if current_bytes is None:
            analyzer = StorageAnalyzer(self.settings, self.config, self.db_session)
            stats = await analyzer.get_storage_stats()
            current_bytes = stats.total_bytes

        percent_used = (current_bytes / self._quota.max_bytes) * 100
        available_bytes = max(0, self._quota.max_bytes - current_bytes)

        if percent_used >= 100:
            status = "exceeded"
            needs_cleanup = True
        elif percent_used >= self._quota.critical_at_percent:
            status = "critical"
            needs_cleanup = True
        elif percent_used >= self._quota.warning_at_percent:
            status = "warning"
            needs_cleanup = True
        else:
            status = "ok"
            needs_cleanup = False

        return QuotaStatus(
            quota=self._quota,
            current_bytes=current_bytes,
            percent_used=percent_used,
            status=status,
            available_bytes=available_bytes,
            needs_cleanup=needs_cleanup,
        )

    async def check_quota_before_index(self, file_size: int) -> bool:
        """Check if indexing a file would exceed quota.

        Args:
            file_size: Size of file to be indexed

        Returns:
            True if file can be indexed, False if would exceed quota

        Raises:
            QuotaExceededError: If quota already exceeded and auto-cleanup disabled

        Example:
            >>> can_index = await quota_mgr.check_quota_before_index(1024000)
        """
        status = await self.get_quota_status()

        if status.status == "exceeded" and not (self._quota and self._quota.auto_cleanup_enabled):
            raise QuotaExceededError(
                "Storage quota exceeded",
                current_bytes=status.current_bytes,
                quota_bytes=status.quota.max_bytes,
            )

        would_exceed = (status.current_bytes + file_size) > status.quota.max_bytes

        if would_exceed and self._quota and self._quota.auto_cleanup_enabled:
            return True

        return not would_exceed
