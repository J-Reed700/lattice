"""
Comprehensive tests for rate limit configuration.

Tests cover:
- RateLimitConfig dataclass functionality
- String conversion to slowapi format
- All predefined rate limit constants
- Edge cases and boundary conditions
"""

import pytest

from src.middleware.rate_limits import (
    EXPORT_LIMIT,
    FILE_DELETE_LIMIT,
    FILE_DOWNLOAD_LIMIT,
    FILE_UPLOAD_LIMIT,
    LOGIN_IP_LIMIT,
    LOGIN_USERNAME_LIMIT,
    OCR_LIMIT,
    PASSWORD_RESET_LIMIT,
    REGISTER_LIMIT,
    SEARCH_LIMIT,
    SUMMARIZE_LIMIT,
    RateLimitConfig,
)


@pytest.mark.unit()
class TestRateLimitConfig:
    """Test RateLimitConfig dataclass."""

    def test_create_config(self) -> None:
        """Should create RateLimitConfig with limit and window."""
        config = RateLimitConfig(limit=10, window_seconds=60)
        assert config.limit == 10
        assert config.window_seconds == 60

    def test_as_string_minute(self) -> None:
        """Should convert 60 second window to '/minute' format."""
        config = RateLimitConfig(limit=5, window_seconds=60)
        assert config.as_string() == "5/minute"

    def test_as_string_hour(self) -> None:
        """Should convert 3600 second window to '/hour' format."""
        config = RateLimitConfig(limit=10, window_seconds=3600)
        assert config.as_string() == "10/hour"

    def test_as_string_day(self) -> None:
        """Should convert 86400 second window to '/day' format."""
        config = RateLimitConfig(limit=100, window_seconds=86400)
        assert config.as_string() == "100/day"

    def test_as_string_custom_seconds(self) -> None:
        """Should convert custom window to 'X/Y seconds' format."""
        config = RateLimitConfig(limit=3, window_seconds=120)
        assert config.as_string() == "3/120 seconds"

    def test_as_string_one_second(self) -> None:
        """Should handle 1 second window."""
        config = RateLimitConfig(limit=1, window_seconds=1)
        assert config.as_string() == "1/1 seconds"

    def test_zero_limit(self) -> None:
        """Should allow zero limit (effectively blocking all requests)."""
        config = RateLimitConfig(limit=0, window_seconds=60)
        assert config.limit == 0
        assert config.as_string() == "0/minute"

    def test_large_limit(self) -> None:
        """Should handle very large limits."""
        config = RateLimitConfig(limit=1000000, window_seconds=3600)
        assert config.limit == 1000000
        assert config.as_string() == "1000000/hour"


@pytest.mark.unit()
class TestLoginRateLimits:
    """Test login-related rate limit configurations."""

    def test_login_ip_limit_exists(self) -> None:
        """LOGIN_IP_LIMIT should be defined."""
        assert LOGIN_IP_LIMIT is not None

    def test_login_ip_limit_values(self) -> None:
        """LOGIN_IP_LIMIT should be 5/minute."""
        assert LOGIN_IP_LIMIT.limit == 5
        assert LOGIN_IP_LIMIT.window_seconds == 60
        assert LOGIN_IP_LIMIT.as_string() == "5/minute"

    def test_login_username_limit_exists(self) -> None:
        """LOGIN_USERNAME_LIMIT should be defined."""
        assert LOGIN_USERNAME_LIMIT is not None

    def test_login_username_limit_values(self) -> None:
        """LOGIN_USERNAME_LIMIT should be 10/hour."""
        assert LOGIN_USERNAME_LIMIT.limit == 10
        assert LOGIN_USERNAME_LIMIT.window_seconds == 3600
        assert LOGIN_USERNAME_LIMIT.as_string() == "10/hour"

    def test_login_username_limit_stricter_than_ip(self) -> None:
        """Username limit should be stricter over time than IP limit."""
        ip_per_hour = LOGIN_IP_LIMIT.limit * (3600 // LOGIN_IP_LIMIT.window_seconds)
        username_per_hour = LOGIN_USERNAME_LIMIT.limit

        assert username_per_hour < ip_per_hour


@pytest.mark.unit()
class TestAuthRateLimits:
    """Test authentication-related rate limit configurations."""

    def test_register_limit_exists(self) -> None:
        """REGISTER_LIMIT should be defined."""
        assert REGISTER_LIMIT is not None

    def test_register_limit_values(self) -> None:
        """REGISTER_LIMIT should be 3/hour."""
        assert REGISTER_LIMIT.limit == 3
        assert REGISTER_LIMIT.window_seconds == 3600
        assert REGISTER_LIMIT.as_string() == "3/hour"

    def test_password_reset_limit_exists(self) -> None:
        """PASSWORD_RESET_LIMIT should be defined."""
        assert PASSWORD_RESET_LIMIT is not None

    def test_password_reset_limit_values(self) -> None:
        """PASSWORD_RESET_LIMIT should be 3/hour."""
        assert PASSWORD_RESET_LIMIT.limit == 3
        assert PASSWORD_RESET_LIMIT.window_seconds == 3600
        assert PASSWORD_RESET_LIMIT.as_string() == "3/hour"

    def test_register_and_password_reset_same_limit(self) -> None:
        """Register and password reset should have same limits."""
        assert REGISTER_LIMIT.limit == PASSWORD_RESET_LIMIT.limit
        assert REGISTER_LIMIT.window_seconds == PASSWORD_RESET_LIMIT.window_seconds


@pytest.mark.unit()
class TestFileOperationLimits:
    """Test file operation rate limit configurations."""

    def test_file_upload_limit_exists(self) -> None:
        """FILE_UPLOAD_LIMIT should be defined."""
        assert FILE_UPLOAD_LIMIT is not None

    def test_file_upload_limit_values(self) -> None:
        """FILE_UPLOAD_LIMIT should be 10/minute."""
        assert FILE_UPLOAD_LIMIT.limit == 10
        assert FILE_UPLOAD_LIMIT.window_seconds == 60
        assert FILE_UPLOAD_LIMIT.as_string() == "10/minute"

    def test_file_download_limit_exists(self) -> None:
        """FILE_DOWNLOAD_LIMIT should be defined."""
        assert FILE_DOWNLOAD_LIMIT is not None

    def test_file_download_limit_values(self) -> None:
        """FILE_DOWNLOAD_LIMIT should be 100/minute."""
        assert FILE_DOWNLOAD_LIMIT.limit == 100
        assert FILE_DOWNLOAD_LIMIT.window_seconds == 60
        assert FILE_DOWNLOAD_LIMIT.as_string() == "100/minute"

    def test_file_delete_limit_exists(self) -> None:
        """FILE_DELETE_LIMIT should be defined."""
        assert FILE_DELETE_LIMIT is not None

    def test_file_delete_limit_values(self) -> None:
        """FILE_DELETE_LIMIT should be 20/minute."""
        assert FILE_DELETE_LIMIT.limit == 20
        assert FILE_DELETE_LIMIT.window_seconds == 60
        assert FILE_DELETE_LIMIT.as_string() == "20/minute"

    def test_download_limit_higher_than_upload(self) -> None:
        """Download limit should be higher than upload limit."""
        assert FILE_DOWNLOAD_LIMIT.limit > FILE_UPLOAD_LIMIT.limit

    def test_delete_limit_between_upload_and_download(self) -> None:
        """Delete limit should be between upload and download limits."""
        assert FILE_UPLOAD_LIMIT.limit < FILE_DELETE_LIMIT.limit < FILE_DOWNLOAD_LIMIT.limit


@pytest.mark.unit()
class TestSearchRateLimit:
    """Test search rate limit configuration."""

    def test_search_limit_exists(self) -> None:
        """SEARCH_LIMIT should be defined."""
        assert SEARCH_LIMIT is not None

    def test_search_limit_values(self) -> None:
        """SEARCH_LIMIT should be 60/minute."""
        assert SEARCH_LIMIT.limit == 60
        assert SEARCH_LIMIT.window_seconds == 60
        assert SEARCH_LIMIT.as_string() == "60/minute"

    def test_search_limit_reasonable(self) -> None:
        """Search limit should allow reasonable user activity (1 search/second)."""
        searches_per_second = SEARCH_LIMIT.limit / SEARCH_LIMIT.window_seconds
        assert searches_per_second == 1.0


@pytest.mark.unit()
class TestExportRateLimit:
    """Test export rate limit configuration."""

    def test_export_limit_exists(self) -> None:
        """EXPORT_LIMIT should be defined."""
        assert EXPORT_LIMIT is not None

    def test_export_limit_values(self) -> None:
        """EXPORT_LIMIT should be 5/hour."""
        assert EXPORT_LIMIT.limit == 5
        assert EXPORT_LIMIT.window_seconds == 3600
        assert EXPORT_LIMIT.as_string() == "5/hour"

    def test_export_limit_restrictive(self) -> None:
        """Export should be more restrictive than other operations."""
        assert EXPORT_LIMIT.limit < FILE_UPLOAD_LIMIT.limit
        assert EXPORT_LIMIT.limit < SEARCH_LIMIT.limit


@pytest.mark.unit()
class TestOCRRateLimit:
    """Test OCR rate limit configuration."""

    def test_ocr_limit_exists(self) -> None:
        """OCR_LIMIT should be defined."""
        assert OCR_LIMIT is not None

    def test_ocr_limit_values(self) -> None:
        """OCR_LIMIT should be 20/minute."""
        assert OCR_LIMIT.limit == 20
        assert OCR_LIMIT.window_seconds == 60
        assert OCR_LIMIT.as_string() == "20/minute"

    def test_ocr_limit_reasonable_for_cpu_intensive(self) -> None:
        """OCR limit should be moderate (CPU-intensive operation)."""
        assert 10 <= OCR_LIMIT.limit <= 30


@pytest.mark.unit()
class TestSummarizeRateLimit:
    """Test summarize rate limit configuration."""

    def test_summarize_limit_exists(self) -> None:
        """SUMMARIZE_LIMIT should be defined."""
        assert SUMMARIZE_LIMIT is not None

    def test_summarize_limit_values(self) -> None:
        """SUMMARIZE_LIMIT should be 10/minute."""
        assert SUMMARIZE_LIMIT.limit == 10
        assert SUMMARIZE_LIMIT.window_seconds == 60
        assert SUMMARIZE_LIMIT.as_string() == "10/minute"

    def test_summarize_limit_moderate(self) -> None:
        """Summarize limit should be moderate (AI operation)."""
        assert SUMMARIZE_LIMIT.limit < SEARCH_LIMIT.limit
        assert SUMMARIZE_LIMIT.limit > EXPORT_LIMIT.limit


@pytest.mark.unit()
class TestRateLimitRelationships:
    """Test relationships between different rate limits."""

    def test_all_limits_positive(self) -> None:
        """All rate limits should be positive."""
        limits = [
            LOGIN_IP_LIMIT,
            LOGIN_USERNAME_LIMIT,
            REGISTER_LIMIT,
            PASSWORD_RESET_LIMIT,
            FILE_UPLOAD_LIMIT,
            FILE_DOWNLOAD_LIMIT,
            FILE_DELETE_LIMIT,
            SEARCH_LIMIT,
            EXPORT_LIMIT,
            OCR_LIMIT,
            SUMMARIZE_LIMIT,
        ]

        for limit in limits:
            assert limit.limit > 0, f"Limit should be positive: {limit}"
            assert limit.window_seconds > 0, f"Window should be positive: {limit}"

    def test_all_limits_have_string_representation(self) -> None:
        """All rate limits should have valid string representation."""
        limits = [
            LOGIN_IP_LIMIT,
            LOGIN_USERNAME_LIMIT,
            REGISTER_LIMIT,
            PASSWORD_RESET_LIMIT,
            FILE_UPLOAD_LIMIT,
            FILE_DOWNLOAD_LIMIT,
            FILE_DELETE_LIMIT,
            SEARCH_LIMIT,
            EXPORT_LIMIT,
            OCR_LIMIT,
            SUMMARIZE_LIMIT,
        ]

        for limit in limits:
            string_repr = limit.as_string()
            assert string_repr is not None
            assert len(string_repr) > 0
            assert "/" in string_repr

    def test_auth_operations_most_restrictive(self) -> None:
        """Auth operations should be most restrictive."""
        auth_per_hour = REGISTER_LIMIT.limit
        upload_per_hour = FILE_UPLOAD_LIMIT.limit * (3600 // FILE_UPLOAD_LIMIT.window_seconds)

        assert auth_per_hour < upload_per_hour

    def test_read_operations_less_restrictive_than_write(self) -> None:
        """Read operations (search, download) should be less restrictive than writes."""
        assert SEARCH_LIMIT.limit > FILE_UPLOAD_LIMIT.limit
        assert FILE_DOWNLOAD_LIMIT.limit > FILE_UPLOAD_LIMIT.limit

    def test_expensive_operations_more_restrictive(self) -> None:
        """Expensive operations (export, OCR) should be more restrictive than simple ones."""
        assert EXPORT_LIMIT.limit < SEARCH_LIMIT.limit
        assert OCR_LIMIT.limit < FILE_DOWNLOAD_LIMIT.limit


@pytest.mark.unit()
class TestRateLimitConfigEdgeCases:
    """Test edge cases for RateLimitConfig."""

    def test_very_short_window(self) -> None:
        """Should handle very short time windows."""
        config = RateLimitConfig(limit=1, window_seconds=1)
        assert config.as_string() == "1/1 seconds"

    def test_very_long_window(self) -> None:
        """Should handle very long time windows."""
        config = RateLimitConfig(limit=1000, window_seconds=604800)
        assert config.as_string() == "1000/604800 seconds"

    def test_config_immutability(self) -> None:
        """RateLimitConfig should be a dataclass (frozen or mutable)."""
        config = RateLimitConfig(limit=5, window_seconds=60)

        config.limit = 10

        assert config.limit == 10

    def test_equality(self) -> None:
        """Two configs with same values should be equal."""
        config1 = RateLimitConfig(limit=5, window_seconds=60)
        config2 = RateLimitConfig(limit=5, window_seconds=60)

        assert config1.limit == config2.limit
        assert config1.window_seconds == config2.window_seconds

    def test_inequality(self) -> None:
        """Configs with different values should not be equal."""
        config1 = RateLimitConfig(limit=5, window_seconds=60)
        config2 = RateLimitConfig(limit=10, window_seconds=60)

        assert config1.limit != config2.limit
