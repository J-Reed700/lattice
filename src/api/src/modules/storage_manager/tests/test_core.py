"""Unit tests for storage manager core functionality."""

import asyncio
from datetime import datetime
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from src.modules.storage_manager import (
    CleanupOperation,
    CleanupRequest,
    StorageAnalyzer,
    StorageCleanup,
    StorageQuota,
    StorageQuotaManager,
)
from src.modules.storage_manager.config import StorageManagerConfig
from src.modules.storage_manager.exceptions import (
    QuotaExceededError,
)


@pytest.fixture()
def mock_settings():
    """Mock application settings."""
    settings = MagicMock()
    settings.storage_documents_path = "/mock/storage/documents"
    settings.storage_screenshots_path = "/mock/storage/screenshots"
    settings.storage_thumbnails_path = "/mock/storage/thumbnails"
    settings.storage_chunks_path = "/mock/storage/chunks"
    settings.ml_cache_dir = "/mock/cache"
    settings.upload_dir = "/mock/uploads"
    settings.log_file = "/mock/logs/vault.log"
    settings.database_url = "postgresql://localhost/test"
    settings.text_embedding_dim = 768
    settings.image_embedding_dim = 512
    return settings


@pytest.fixture()
def mock_config():
    """Mock storage manager configuration."""
    return StorageManagerConfig(
        cache_ttl_seconds=300,
        max_files_per_scan=1000000,
        cleanup_batch_size=1000,
    )


@pytest.fixture()
async def mock_db_session():
    """Mock async database session."""
    session = AsyncMock()
    session.execute = AsyncMock()
    session.commit = AsyncMock()
    return session


class TestStorageAnalyzer:
    """Tests for StorageAnalyzer class."""

    @pytest.mark.asyncio()
    async def test_get_storage_stats_basic(self, mock_settings, mock_config, mock_db_session):
        """Test basic storage stats retrieval."""
        analyzer = StorageAnalyzer(mock_settings, mock_config, mock_db_session)

        with (
            patch.object(analyzer, "_calculate_directory_size", return_value=1024000),
            patch.object(analyzer, "_estimate_embeddings_size", return_value=512000),
            patch.object(analyzer, "_calculate_database_size", return_value=256000),
            patch.object(analyzer, "_calculate_cache_size", return_value=128000),
        ):
            stats = await analyzer.get_storage_stats(refresh=True)

            assert stats.total_bytes > 0
            assert stats.breakdown.original_files >= 0
            assert stats.breakdown.embeddings >= 0
            assert stats.breakdown.database >= 0
            assert not stats.cached

    @pytest.mark.asyncio()
    async def test_get_storage_stats_cached(self, mock_settings, mock_config, mock_db_session):
        """Test cached storage stats retrieval."""
        analyzer = StorageAnalyzer(mock_settings, mock_config, mock_db_session)

        with (
            patch.object(analyzer, "_calculate_directory_size", return_value=1024000),
            patch.object(analyzer, "_estimate_embeddings_size", return_value=512000),
        ):
            await analyzer.get_storage_stats(refresh=True)
            stats_cached = await analyzer.get_storage_stats(refresh=False)

            assert stats_cached is not None

    @pytest.mark.asyncio()
    async def test_calculate_by_file_type(self, mock_settings, mock_config, mock_db_session):
        """Test storage calculation by file type."""
        analyzer = StorageAnalyzer(mock_settings, mock_config, mock_db_session)

        mock_db_session.execute.return_value.all.return_value = [
            ("pdf", 1024000),
            ("png", 512000),
            ("txt", 256000),
        ]

        by_file_type = await analyzer._calculate_by_file_type()

        assert "pdf" in by_file_type
        assert "png" in by_file_type
        assert by_file_type["pdf"] == 1024000

    @pytest.mark.asyncio()
    async def test_cache_invalidation(self, mock_settings, mock_config, mock_db_session):
        """Test cache invalidation after TTL expires."""
        config = StorageManagerConfig(cache_ttl_seconds=1)
        analyzer = StorageAnalyzer(mock_settings, config, mock_db_session)

        with (
            patch.object(analyzer, "_calculate_directory_size", return_value=1024000),
            patch.object(analyzer, "_estimate_embeddings_size", return_value=512000),
        ):
            await analyzer.get_storage_stats(refresh=True)

            await asyncio.sleep(2)

            assert not analyzer._is_cache_valid()


class TestStorageCleanup:
    """Tests for StorageCleanup class."""

    @pytest.mark.asyncio()
    async def test_cleanup_orphaned_files_dry_run(
        self, mock_settings, mock_config, mock_db_session
    ):
        """Test dry run of orphaned files cleanup."""
        cleanup = StorageCleanup(mock_settings, mock_config, mock_db_session)

        mock_db_session.execute.return_value.all.return_value = [
            ("/storage/file1.pdf",),
            ("/storage/file2.pdf",),
        ]

        request = CleanupRequest(
            operation=CleanupOperation.ORPHANED_FILES,
            dry_run=True,
        )

        with (
            patch("pathlib.Path.exists", return_value=True),
            patch("pathlib.Path.is_file", return_value=True),
            patch("pathlib.Path.rglob", return_value=[]),
            patch("pathlib.Path.stat") as mock_stat,
        ):
            mock_stat.return_value.st_size = 1024
            mock_stat.return_value.st_mtime = datetime.now().timestamp()

            result = await cleanup.cleanup(request)

            assert result.dry_run is True
            assert result.files_affected >= 0

    @pytest.mark.asyncio()
    async def test_cleanup_cache(self, mock_settings, mock_config, mock_db_session):
        """Test cache cleanup operation."""
        cleanup = StorageCleanup(mock_settings, mock_config, mock_db_session)

        request = CleanupRequest(
            operation=CleanupOperation.CACHE,
            dry_run=True,
            older_than_days=30,
        )

        with (
            patch("pathlib.Path.exists", return_value=True),
            patch("pathlib.Path.rglob", return_value=[]),
        ):
            result = await cleanup.cleanup(request)

            assert result.operation == CleanupOperation.CACHE
            assert result.files_affected >= 0

    @pytest.mark.asyncio()
    async def test_vacuum_database_postgresql(self, mock_settings, mock_config, mock_db_session):
        """Test database vacuum for PostgreSQL."""
        mock_settings.database_url = "postgresql://localhost/test"
        cleanup = StorageCleanup(mock_settings, mock_config, mock_db_session)

        mock_db_session.execute.return_value.scalar.return_value = 1024000

        request = CleanupRequest(
            operation=CleanupOperation.VACUUM_DB,
            dry_run=True,
        )

        result = await cleanup.cleanup(request)

        assert result.operation == CleanupOperation.VACUUM_DB

    @pytest.mark.asyncio()
    async def test_cleanup_with_filters(self, mock_settings, mock_config, mock_db_session):
        """Test cleanup with age and size filters."""
        cleanup = StorageCleanup(mock_settings, mock_config, mock_db_session)

        request = CleanupRequest(
            operation=CleanupOperation.CACHE,
            dry_run=True,
            older_than_days=7,
            min_size_bytes=1048576,
        )

        with (
            patch("pathlib.Path.exists", return_value=True),
            patch("pathlib.Path.rglob", return_value=[]),
        ):
            result = await cleanup.cleanup(request)

            assert result is not None


class TestStorageQuotaManager:
    """Tests for StorageQuotaManager class."""

    @pytest.mark.asyncio()
    async def test_set_quota(self, mock_settings, mock_config, mock_db_session):
        """Test setting storage quota."""
        quota_mgr = StorageQuotaManager(mock_settings, mock_config, mock_db_session)

        quota = await quota_mgr.set_quota(
            max_bytes=50 * 1024**3,
            warning_at_percent=80,
            critical_at_percent=95,
        )

        assert quota.max_bytes == 50 * 1024**3
        assert quota.warning_at_percent == 80
        assert quota.critical_at_percent == 95

    @pytest.mark.asyncio()
    async def test_get_quota_status_ok(self, mock_settings, mock_config, mock_db_session):
        """Test quota status when usage is normal."""
        quota_mgr = StorageQuotaManager(mock_settings, mock_config, mock_db_session)

        await quota_mgr.set_quota(max_bytes=100 * 1024**3)

        with patch(
            "src.modules.storage_manager.core.StorageAnalyzer.get_storage_stats"
        ) as mock_stats:
            mock_stats.return_value = MagicMock(total_bytes=30 * 1024**3)

            status = await quota_mgr.get_quota_status()

            assert status.status == "ok"
            assert not status.needs_cleanup

    @pytest.mark.asyncio()
    async def test_get_quota_status_warning(self, mock_settings, mock_config, mock_db_session):
        """Test quota status when usage triggers warning."""
        quota_mgr = StorageQuotaManager(mock_settings, mock_config, mock_db_session)

        await quota_mgr.set_quota(max_bytes=100 * 1024**3, warning_at_percent=80)

        with patch(
            "src.modules.storage_manager.core.StorageAnalyzer.get_storage_stats"
        ) as mock_stats:
            mock_stats.return_value = MagicMock(total_bytes=85 * 1024**3)

            status = await quota_mgr.get_quota_status()

            assert status.status == "warning"
            assert status.needs_cleanup

    @pytest.mark.asyncio()
    async def test_get_quota_status_exceeded(self, mock_settings, mock_config, mock_db_session):
        """Test quota status when quota is exceeded."""
        quota_mgr = StorageQuotaManager(mock_settings, mock_config, mock_db_session)

        await quota_mgr.set_quota(max_bytes=100 * 1024**3)

        with patch(
            "src.modules.storage_manager.core.StorageAnalyzer.get_storage_stats"
        ) as mock_stats:
            mock_stats.return_value = MagicMock(total_bytes=110 * 1024**3)

            status = await quota_mgr.get_quota_status()

            assert status.status == "exceeded"
            assert status.needs_cleanup

    @pytest.mark.asyncio()
    async def test_check_quota_before_index_allowed(
        self, mock_settings, mock_config, mock_db_session
    ):
        """Test quota check allows indexing when space available."""
        quota_mgr = StorageQuotaManager(mock_settings, mock_config, mock_db_session)

        await quota_mgr.set_quota(max_bytes=100 * 1024**3)

        with patch(
            "src.modules.storage_manager.core.StorageAnalyzer.get_storage_stats"
        ) as mock_stats:
            mock_stats.return_value = MagicMock(total_bytes=50 * 1024**3)

            can_index = await quota_mgr.check_quota_before_index(1024000)

            assert can_index is True

    @pytest.mark.asyncio()
    async def test_check_quota_before_index_exceeded(
        self, mock_settings, mock_config, mock_db_session
    ):
        """Test quota check raises error when quota exceeded."""
        quota_mgr = StorageQuotaManager(mock_settings, mock_config, mock_db_session)

        await quota_mgr.set_quota(max_bytes=100 * 1024**3, auto_cleanup_enabled=False)

        with patch(
            "src.modules.storage_manager.core.StorageAnalyzer.get_storage_stats"
        ) as mock_stats:
            mock_stats.return_value = MagicMock(total_bytes=110 * 1024**3)

            with pytest.raises(QuotaExceededError):
                await quota_mgr.check_quota_before_index(1024000)


class TestCleanupRequest:
    """Tests for CleanupRequest model."""

    def test_cleanup_request_validation(self):
        """Test cleanup request validation."""
        request = CleanupRequest(
            operation=CleanupOperation.ORPHANED_FILES,
            dry_run=True,
            older_than_days=30,
        )

        assert request.operation == CleanupOperation.ORPHANED_FILES
        assert request.dry_run is True
        assert request.older_than_days == 30

    def test_cleanup_request_defaults(self):
        """Test cleanup request default values."""
        request = CleanupRequest(operation=CleanupOperation.CACHE)

        assert request.dry_run is True
        assert request.older_than_days is None
        assert request.file_types is None


class TestStorageQuota:
    """Tests for StorageQuota model."""

    def test_storage_quota_validation(self):
        """Test storage quota validation."""
        quota = StorageQuota(
            max_bytes=50 * 1024**3,
            warning_at_percent=80,
            critical_at_percent=95,
        )

        assert quota.max_bytes == 50 * 1024**3
        assert quota.warning_at_percent == 80
        assert quota.critical_at_percent == 95

    def test_storage_quota_threshold_validation(self):
        """Test quota thresholds are validated correctly."""
        with pytest.raises(ValueError):
            StorageQuota(
                max_bytes=50 * 1024**3,
                warning_at_percent=95,
                critical_at_percent=80,
            )


class TestIntegration:
    """Integration tests for storage manager."""

    @pytest.mark.asyncio()
    async def test_full_cleanup_workflow(self, mock_settings, mock_config, mock_db_session):
        """Test complete cleanup workflow from analysis to execution."""
        analyzer = StorageAnalyzer(mock_settings, mock_config, mock_db_session)
        cleanup = StorageCleanup(mock_settings, mock_config, mock_db_session)

        with (
            patch.object(analyzer, "_calculate_directory_size", return_value=1024000),
            patch.object(analyzer, "_estimate_embeddings_size", return_value=512000),
        ):
            await analyzer.get_storage_stats(refresh=True)

            request = CleanupRequest(
                operation=CleanupOperation.CACHE,
                dry_run=False,
                older_than_days=30,
            )

            with (
                patch("pathlib.Path.exists", return_value=True),
                patch("pathlib.Path.rglob", return_value=[]),
            ):
                result = await cleanup.cleanup(request)

                assert result.operation == CleanupOperation.CACHE

    @pytest.mark.asyncio()
    async def test_quota_enforcement_workflow(self, mock_settings, mock_config, mock_db_session):
        """Test quota enforcement across components."""
        quota_mgr = StorageQuotaManager(mock_settings, mock_config, mock_db_session)
        analyzer = StorageAnalyzer(mock_settings, mock_config, mock_db_session)

        await quota_mgr.set_quota(
            max_bytes=100 * 1024**3,
            warning_at_percent=80,
            auto_cleanup_enabled=True,
        )

        with (
            patch.object(analyzer, "get_storage_stats") as mock_stats,
            patch.object(analyzer, "_calculate_directory_size", return_value=85 * 1024**3),
        ):
            mock_stats.return_value = MagicMock(total_bytes=85 * 1024**3)

            status = await quota_mgr.get_quota_status()

            assert status.status == "warning"
            assert status.needs_cleanup
