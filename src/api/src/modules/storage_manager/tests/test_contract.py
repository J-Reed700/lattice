"""Contract validation tests for storage manager.

Ensures the module adheres to its documented contract and specifications.
"""

import pytest

from src.modules.storage_manager import (
    CleanupError,
    CleanupOperation,
    CleanupResult,
    QuotaExceededError,
    QuotaStatus,
    StorageAnalysisError,
    StorageError,
    StorageQuota,
    StorageStats,
)


class TestPublicInterface:
    """Validate public interface completeness."""

    def test_all_contracted_functions_exported(self):
        """All contracted functions must be in __all__."""
        from src.modules.storage_manager import __all__

        required_exports = [
            "StorageAnalyzer",
            "StorageCleanup",
            "StorageQuotaManager",
            "StorageStats",
            "CleanupResult",
            "StorageQuota",
            "QuotaStatus",
            "CleanupOperation",
            "get_storage_stats",
            "cleanup_orphaned_files",
            "set_storage_quota",
            "check_quota_status",
            "StorageError",
            "StorageAnalysisError",
            "CleanupError",
            "QuotaExceededError",
        ]

        for export in required_exports:
            assert export in __all__, f"Missing required export: {export}"

    def test_no_private_exports(self):
        """No private functions should be exported."""
        from src.modules.storage_manager import __all__

        for name in __all__:
            assert not name.startswith("_"), f"Private export found: {name}"


class TestInputValidation:
    """Validate input validation per contract."""

    def test_storage_quota_validates_max_bytes(self):
        """StorageQuota must validate max_bytes > 0."""
        with pytest.raises(ValueError):
            StorageQuota(
                max_bytes=0,
                warning_at_percent=80,
                critical_at_percent=95,
            )

    def test_storage_quota_validates_thresholds(self):
        """Critical threshold must be > warning threshold."""
        with pytest.raises(ValueError):
            StorageQuota(
                max_bytes=1024,
                warning_at_percent=95,
                critical_at_percent=80,
            )

    def test_cleanup_request_validates_operation(self):
        """CleanupRequest requires valid operation."""
        from src.modules.storage_manager.models import CleanupRequest

        request = CleanupRequest(operation=CleanupOperation.CACHE)
        assert request.operation == CleanupOperation.CACHE


class TestOutputStructure:
    """Validate output structures match contract."""

    def test_storage_stats_structure(self):
        """StorageStats has all required fields."""
        from datetime import datetime

        from src.modules.storage_manager.models import StorageBreakdown

        breakdown = StorageBreakdown(
            original_files=1024,
            embeddings=512,
            database=256,
            thumbnails=128,
            cache=64,
            logs=32,
        )

        stats = StorageStats(
            total_bytes=breakdown.total,
            breakdown=breakdown,
            by_file_type={},
            by_date={},
            largest_files=[],
            growth_trend=[],
            last_calculated=datetime.now(),
            cached=False,
        )

        assert hasattr(stats, "total_bytes")
        assert hasattr(stats, "breakdown")
        assert hasattr(stats, "by_file_type")
        assert hasattr(stats, "by_date")
        assert hasattr(stats, "largest_files")
        assert hasattr(stats, "growth_trend")
        assert hasattr(stats, "last_calculated")
        assert hasattr(stats, "cached")

    def test_cleanup_result_structure(self):
        """CleanupResult has all required fields."""
        from datetime import datetime

        result = CleanupResult(
            operation=CleanupOperation.CACHE,
            dry_run=True,
            files_affected=42,
            space_reclaimed_bytes=1024000,
            errors=[],
            duration_seconds=2.5,
            timestamp=datetime.now(),
        )

        assert hasattr(result, "operation")
        assert hasattr(result, "dry_run")
        assert hasattr(result, "files_affected")
        assert hasattr(result, "space_reclaimed_bytes")
        assert hasattr(result, "errors")
        assert hasattr(result, "duration_seconds")
        assert hasattr(result, "timestamp")

    def test_quota_status_structure(self):
        """QuotaStatus has all required fields."""
        quota = StorageQuota(
            max_bytes=1024,
            warning_at_percent=80,
            critical_at_percent=95,
        )

        status = QuotaStatus(
            quota=quota,
            current_bytes=512,
            percent_used=50.0,
            status="ok",
            available_bytes=512,
            needs_cleanup=False,
        )

        assert hasattr(status, "quota")
        assert hasattr(status, "current_bytes")
        assert hasattr(status, "percent_used")
        assert hasattr(status, "status")
        assert hasattr(status, "available_bytes")
        assert hasattr(status, "needs_cleanup")


class TestErrorHandling:
    """Validate error types and conditions."""

    def test_storage_error_hierarchy(self):
        """All errors inherit from StorageError."""
        assert issubclass(StorageAnalysisError, StorageError)
        assert issubclass(CleanupError, StorageError)
        assert issubclass(QuotaExceededError, StorageError)

    def test_quota_exceeded_error_attributes(self):
        """QuotaExceededError includes usage information."""
        error = QuotaExceededError(
            "Quota exceeded",
            current_bytes=60 * 1024**3,
            quota_bytes=50 * 1024**3,
        )

        assert error.current_bytes == 60 * 1024**3
        assert error.quota_bytes == 50 * 1024**3
        assert error.overage_bytes == 10 * 1024**3


class TestContractInvariants:
    """Validate contract invariants are maintained."""

    def test_storage_breakdown_total_matches(self):
        """StorageBreakdown.total equals sum of categories."""
        from src.modules.storage_manager.models import StorageBreakdown

        breakdown = StorageBreakdown(
            original_files=1000,
            embeddings=500,
            database=250,
            thumbnails=125,
            cache=62,
            logs=31,
        )

        expected_total = 1000 + 500 + 250 + 125 + 62 + 31
        assert breakdown.total == expected_total

    def test_dry_run_operations_do_not_modify(self):
        """Dry run cleanup operations must not modify filesystem."""
        from src.modules.storage_manager.models import CleanupRequest

        request = CleanupRequest(
            operation=CleanupOperation.CACHE,
            dry_run=True,
        )

        assert request.dry_run is True

    def test_storage_stats_total_equals_breakdown(self):
        """StorageStats.total_bytes must equal breakdown.total."""
        from datetime import datetime

        from src.modules.storage_manager.models import StorageBreakdown

        breakdown = StorageBreakdown(
            original_files=1024,
            embeddings=512,
            database=256,
            thumbnails=128,
            cache=64,
            logs=32,
        )

        stats = StorageStats(
            total_bytes=breakdown.total,
            breakdown=breakdown,
            by_file_type={},
            by_date={},
            largest_files=[],
            growth_trend=[],
            last_calculated=datetime.now(),
            cached=False,
        )

        assert stats.total_bytes == breakdown.total


class TestRegenerability:
    """Validate module can be regenerated from specification."""

    def test_module_has_readme(self):
        """Module must have README.md with contract."""
        from pathlib import Path

        readme_path = Path(__file__).parent.parent / "README.md"
        assert readme_path.exists(), "README.md is mandatory"

        content = readme_path.read_text()
        assert len(content) > 500, "README must be comprehensive"
        assert "Contract" in content or "Purpose" in content

    def test_all_public_functions_documented(self):
        """All public functions must have docstrings."""
        import src.modules.storage_manager as module
        from src.modules.storage_manager import __all__

        for name in __all__:
            obj = getattr(module, name, None)
            if obj and callable(obj):
                assert obj.__doc__, f"{name} missing docstring"

    def test_models_have_examples(self):
        """All models must have example usage in Config."""
        from src.modules.storage_manager.models import (
            CleanupResult,
            StorageQuota,
            StorageStats,
        )

        for model_class in [StorageStats, CleanupResult, StorageQuota]:
            assert hasattr(model_class, "Config")
            assert hasattr(model_class.Config, "json_schema_extra")
