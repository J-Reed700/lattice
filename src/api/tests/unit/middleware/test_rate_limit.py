"""
Comprehensive tests for rate limiting middleware.

Tests cover:
- Redis client initialization and connection
- Rate limit key generation
- Rate limit checking (success, exceed, cleanup)
- Rate limit decorators (username, user_id)
- Login rate limiting (dual limits: IP + username)
- Rate limit reset functionality
- Error handling and edge cases
"""

from unittest.mock import AsyncMock, Mock, patch

from fastapi import Request, status
import pytest
import redis.asyncio as redis

from src.middleware.rate_limit import (
    RateLimitExceededError,
    check_login_rate_limit,
    check_rate_limit_by_key,
    get_rate_limit_key,
    get_redis,
    limiter,
    rate_limit_by_user_id,
    rate_limit_by_username,
    reset_login_rate_limit,
)


@pytest.fixture()
def mock_redis():
    """Mock Redis client."""
    mock = AsyncMock(spec=redis.Redis)
    mock.incr = AsyncMock(return_value=1)
    mock.expire = AsyncMock()
    mock.ttl = AsyncMock(return_value=60)
    mock.delete = AsyncMock()
    return mock


@pytest.fixture()
def mock_request():
    """Mock FastAPI request."""
    request = Mock(spec=Request)
    request.client = Mock()
    request.client.host = "192.168.1.1"
    return request


@pytest.fixture()
def mock_user():
    """Mock authenticated user."""
    user = Mock()
    user.id = 123
    return user


@pytest.mark.unit()
class TestGetRedis:
    """Test Redis client initialization."""

    @pytest.mark.asyncio()
    async def test_redis_client_created(self, mock_redis) -> None:
        """Redis client should be created on first access."""
        mock_settings = Mock()
        mock_settings.redis_url = "redis://localhost:6379"

        mock_from_url = AsyncMock(return_value=mock_redis)

        with patch("src.middleware.rate_limit.redis.from_url", mock_from_url):
            with patch("src.middleware.rate_limit.redis_client", None):
                with patch("src.config.settings.get_settings", return_value=mock_settings):
                    client = await get_redis()
                    assert client is not None
                    assert client == mock_redis

    @pytest.mark.asyncio()
    async def test_redis_client_reused(self, mock_redis) -> None:
        """Redis client should be reused after first creation."""
        with patch("src.middleware.rate_limit.redis_client", mock_redis):
            client1 = await get_redis()
            client2 = await get_redis()
            assert client1 is client2


@pytest.mark.unit()
class TestGetRateLimitKey:
    """Test rate limit key generation."""

    def test_key_with_ip_only(self, mock_request) -> None:
        """Key should use IP address only when no suffix."""
        key = get_rate_limit_key(mock_request)
        assert key == "192.168.1.1"

    def test_key_with_suffix(self, mock_request) -> None:
        """Key should combine IP and suffix when provided."""
        key = get_rate_limit_key(mock_request, suffix="testuser")
        assert key == "192.168.1.1:testuser"

    def test_key_with_empty_suffix(self, mock_request) -> None:
        """Empty suffix should return IP only."""
        key = get_rate_limit_key(mock_request, suffix="")
        assert key == "192.168.1.1"


@pytest.mark.unit()
class TestRateLimitExceededError:
    """Test RateLimitExceededError exception."""

    def test_error_status_code(self) -> None:
        """Error should have 429 status code."""
        error = RateLimitExceededError(retry_after=60)
        assert error.status_code == status.HTTP_429_TOO_MANY_REQUESTS

    def test_error_detail_message(self) -> None:
        """Error should include retry_after in detail."""
        error = RateLimitExceededError(retry_after=120)
        assert "120 seconds" in error.detail

    def test_error_headers(self) -> None:
        """Error should include Retry-After header."""
        error = RateLimitExceededError(retry_after=90)
        assert error.headers == {"Retry-After": "90"}


@pytest.mark.unit()
class TestCheckRateLimitByKey:
    """Test check_rate_limit_by_key function."""

    @pytest.mark.asyncio()
    async def test_first_request_allowed(self, mock_redis) -> None:
        """First request should be allowed and set expiry."""
        mock_redis.incr.return_value = 1

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            allowed, retry_after = await check_rate_limit_by_key(
                "test:key", limit=5, window_seconds=60
            )

        assert allowed is True
        assert retry_after == 0
        mock_redis.incr.assert_called_once_with("test:key")
        mock_redis.expire.assert_called_once_with("test:key", 60)

    @pytest.mark.asyncio()
    async def test_within_limit_allowed(self, mock_redis) -> None:
        """Request within limit should be allowed."""
        mock_redis.incr.return_value = 3

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            allowed, retry_after = await check_rate_limit_by_key(
                "test:key", limit=5, window_seconds=60
            )

        assert allowed is True
        assert retry_after == 0

    @pytest.mark.asyncio()
    async def test_at_limit_allowed(self, mock_redis) -> None:
        """Request at exactly the limit should be allowed."""
        mock_redis.incr.return_value = 5

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            allowed, retry_after = await check_rate_limit_by_key(
                "test:key", limit=5, window_seconds=60
            )

        assert allowed is True
        assert retry_after == 0

    @pytest.mark.asyncio()
    async def test_exceed_limit_blocked(self, mock_redis) -> None:
        """Request exceeding limit should be blocked."""
        mock_redis.incr.return_value = 6
        mock_redis.ttl.return_value = 45

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            allowed, retry_after = await check_rate_limit_by_key(
                "test:key", limit=5, window_seconds=60
            )

        assert allowed is False
        assert retry_after == 45
        mock_redis.ttl.assert_called_once_with("test:key")

    @pytest.mark.asyncio()
    async def test_ttl_fallback_to_window(self, mock_redis) -> None:
        """If TTL is invalid, should fallback to window_seconds."""
        mock_redis.incr.return_value = 10
        mock_redis.ttl.return_value = -1

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            allowed, retry_after = await check_rate_limit_by_key(
                "test:key", limit=5, window_seconds=120
            )

        assert allowed is False
        assert retry_after == 120

    @pytest.mark.asyncio()
    async def test_redis_error_allows_request(self, mock_redis) -> None:
        """Redis errors should allow request (fail open)."""
        mock_redis.incr.side_effect = Exception("Redis connection error")

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            allowed, retry_after = await check_rate_limit_by_key(
                "test:key", limit=5, window_seconds=60
            )

        assert allowed is True
        assert retry_after == 0

    @pytest.mark.asyncio()
    async def test_second_request_no_expire(self, mock_redis) -> None:
        """Second request should not set expiry again."""
        mock_redis.incr.return_value = 2

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            allowed, retry_after = await check_rate_limit_by_key(
                "test:key", limit=5, window_seconds=60
            )

        assert allowed is True
        mock_redis.expire.assert_not_called()


@pytest.mark.unit()
class TestRateLimitByUsername:
    """Test rate_limit_by_username decorator."""

    @pytest.mark.asyncio()
    async def test_allows_request_within_limit(self, mock_redis) -> None:
        """Should allow request within rate limit."""
        mock_redis.incr.return_value = 1

        @rate_limit_by_username(limit=10, window_seconds=3600)
        async def test_func(username: str):
            return f"Success for {username}"

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            result = await test_func(username="testuser")

        assert result == "Success for testuser"
        mock_redis.incr.assert_called_once_with("auth:login:username:testuser")

    @pytest.mark.asyncio()
    async def test_blocks_request_exceeding_limit(self, mock_redis) -> None:
        """Should block request exceeding rate limit."""
        mock_redis.incr.return_value = 11
        mock_redis.ttl.return_value = 1800

        @rate_limit_by_username(limit=10, window_seconds=3600)
        async def test_func(username: str):
            return "Should not reach here"

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            with pytest.raises(RateLimitExceededError) as exc_info:
                await test_func(username="testuser")

        assert exc_info.value.status_code == status.HTTP_429_TOO_MANY_REQUESTS

    @pytest.mark.asyncio()
    async def test_no_username_bypasses_limit(self, mock_redis) -> None:
        """Should bypass rate limit if username is not provided."""

        @rate_limit_by_username(limit=10, window_seconds=3600)
        async def test_func(username: str = None):
            return "Success without username"

        result = await test_func()

        assert result == "Success without username"
        mock_redis.incr.assert_not_called()

    @pytest.mark.asyncio()
    async def test_extracts_username_from_form_data(self, mock_redis) -> None:
        """Should extract username from form_data kwarg."""
        mock_redis.incr.return_value = 1
        form_data = Mock()
        form_data.username = "formuser"

        @rate_limit_by_username(limit=10, window_seconds=3600)
        async def test_func(form_data=None):
            return "Success"

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            result = await test_func(form_data=form_data)

        assert result == "Success"
        mock_redis.incr.assert_called_once_with("auth:login:username:formuser")


@pytest.mark.unit()
class TestRateLimitByUserId:
    """Test rate_limit_by_user_id decorator."""

    @pytest.mark.asyncio()
    async def test_allows_request_within_limit(self, mock_redis, mock_user) -> None:
        """Should allow request within rate limit."""
        mock_redis.incr.return_value = 1

        @rate_limit_by_user_id(limit=10, window_seconds=60)
        async def upload_file(current_user):
            return "File uploaded"

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            result = await upload_file(current_user=mock_user)

        assert result == "File uploaded"
        mock_redis.incr.assert_called_once_with("user:123:upload_file")

    @pytest.mark.asyncio()
    async def test_blocks_request_exceeding_limit(self, mock_redis, mock_user) -> None:
        """Should block request exceeding rate limit."""
        mock_redis.incr.return_value = 11
        mock_redis.ttl.return_value = 30

        @rate_limit_by_user_id(limit=10, window_seconds=60)
        async def upload_file(current_user):
            return "Should not reach here"

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            with pytest.raises(RateLimitExceededError) as exc_info:
                await upload_file(current_user=mock_user)

        assert exc_info.value.status_code == status.HTTP_429_TOO_MANY_REQUESTS

    @pytest.mark.asyncio()
    async def test_no_user_bypasses_limit(self, mock_redis) -> None:
        """Should bypass rate limit if user is not authenticated."""

        @rate_limit_by_user_id(limit=10, window_seconds=60)
        async def upload_file(current_user=None):
            return "Success without user"

        result = await upload_file()

        assert result == "Success without user"
        mock_redis.incr.assert_not_called()

    @pytest.mark.asyncio()
    async def test_uses_function_name_in_key(self, mock_redis, mock_user) -> None:
        """Should use function name in rate limit key."""
        mock_redis.incr.return_value = 1

        @rate_limit_by_user_id(limit=5, window_seconds=60)
        async def export_data(current_user):
            return "Exported"

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            await export_data(current_user=mock_user)

        mock_redis.incr.assert_called_once_with("user:123:export_data")


@pytest.mark.unit()
class TestCheckLoginRateLimit:
    """Test check_login_rate_limit function."""

    @pytest.mark.asyncio()
    async def test_allows_first_login_attempt(self, mock_redis, mock_request) -> None:
        """Should allow first login attempt."""
        mock_redis.incr.return_value = 1

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            await check_login_rate_limit(mock_request, "testuser")

    @pytest.mark.asyncio()
    async def test_blocks_ip_exceeding_limit(self, mock_redis, mock_request) -> None:
        """Should block IP exceeding 5 attempts/minute."""
        call_count = 0

        def mock_incr(key):
            nonlocal call_count
            call_count += 1
            if "ip" in key:
                return 6
            return 1

        mock_redis.incr.side_effect = mock_incr
        mock_redis.ttl.return_value = 30

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            with pytest.raises(RateLimitExceededError):
                await check_login_rate_limit(mock_request, "testuser")

    @pytest.mark.asyncio()
    async def test_blocks_username_exceeding_limit(self, mock_redis, mock_request) -> None:
        """Should block username exceeding 10 attempts/hour."""
        call_count = 0

        def mock_incr(key):
            nonlocal call_count
            call_count += 1
            if "username" in key:
                return 11
            return 1

        mock_redis.incr.side_effect = mock_incr
        mock_redis.ttl.return_value = 1800

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            with pytest.raises(RateLimitExceededError):
                await check_login_rate_limit(mock_request, "testuser")

    @pytest.mark.asyncio()
    async def test_checks_both_ip_and_username(self, mock_redis, mock_request) -> None:
        """Should check both IP and username limits."""
        mock_redis.incr.return_value = 1

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            await check_login_rate_limit(mock_request, "testuser")

        assert mock_redis.incr.call_count == 2

        calls = [call[0][0] for call in mock_redis.incr.call_args_list]
        assert any("ip" in call and "192.168.1.1" in call for call in calls)
        assert any("username" in call and "testuser" in call for call in calls)

    @pytest.mark.asyncio()
    async def test_ip_limit_checked_first(self, mock_redis, mock_request) -> None:
        """IP limit should be checked before username limit."""
        call_count = 0

        def mock_incr(key):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                assert "ip" in key
                return 6
            return 1

        mock_redis.incr.side_effect = mock_incr
        mock_redis.ttl.return_value = 30

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            with pytest.raises(RateLimitExceededError):
                await check_login_rate_limit(mock_request, "testuser")

        assert call_count == 1


@pytest.mark.unit()
class TestResetLoginRateLimit:
    """Test reset_login_rate_limit function."""

    @pytest.mark.asyncio()
    async def test_deletes_username_key(self, mock_redis) -> None:
        """Should delete username rate limit key."""
        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            await reset_login_rate_limit("testuser")

        mock_redis.delete.assert_called_once_with("auth:login:username:testuser")

    @pytest.mark.asyncio()
    async def test_handles_redis_error_gracefully(self, mock_redis) -> None:
        """Should handle Redis errors gracefully."""
        mock_redis.delete.side_effect = Exception("Redis error")

        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            await reset_login_rate_limit("testuser")

    @pytest.mark.asyncio()
    async def test_resets_correct_username(self, mock_redis) -> None:
        """Should reset rate limit for correct username."""
        with patch("src.middleware.rate_limit.get_redis", return_value=mock_redis):
            await reset_login_rate_limit("user123")

        expected_key = "auth:login:username:user123"
        mock_redis.delete.assert_called_once_with(expected_key)


@pytest.mark.unit()
class TestLimiterInstance:
    """Test limiter instance configuration."""

    def test_limiter_exists(self) -> None:
        """Limiter instance should exist."""
        assert limiter is not None

    def test_limiter_default_limit(self) -> None:
        """Limiter should have default limit configured."""
        assert hasattr(limiter, "_default_limits")
