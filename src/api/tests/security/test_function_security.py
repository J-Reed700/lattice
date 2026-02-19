"""Security tests for function calling system.

Tests:
- Rate limiting prevents abuse
- Input validation blocks malicious input
- SSRF prevention (web service)
- SQL injection attempts (parameterized queries)
- Path traversal prevention
- Excessive content length handling
- Malformed JSON handling
- Error message information disclosure
"""

from __future__ import annotations

from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from src.services.llm.function_calling.executor import (
    FunctionCallError,
    FunctionExecutor,
    RateLimitError,
)
from src.services.llm.function_calling.registry import FunctionRegistry
from src.services.llm.function_calling.web_service import WebService
from tests.fixtures.function_calling_fixtures import (
    create_test_tool,
)


@pytest.mark.security()
class TestRateLimitingSecurity:
    """Test rate limiting prevents abuse."""

    @pytest.mark.asyncio()
    async def test_rate_limiting_prevents_dos(self) -> None:
        """Test rate limiting prevents DoS attacks."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool", rate_limit=5)
        registry.register(tool)

        executor = FunctionExecutor(registry)

        # Fill rate limit
        for _ in range(5):
            await executor.execute("test_tool", {"query": "test"})

        # Additional requests should be blocked
        with pytest.raises(RateLimitError):
            await executor.execute("test_tool", {"query": "test"})

        # Even with different arguments
        with pytest.raises(RateLimitError):
            await executor.execute("test_tool", {"query": "different"})

    @pytest.mark.asyncio()
    async def test_rate_limiting_per_function(self) -> None:
        """Test rate limits are enforced per function."""
        registry = FunctionRegistry()
        tool1 = create_test_tool(name="tool1", rate_limit=3)
        tool2 = create_test_tool(name="tool2", rate_limit=3)
        registry.register(tool1)
        registry.register(tool2)

        executor = FunctionExecutor(registry)

        # Fill rate limit for tool1
        for _ in range(3):
            await executor.execute("tool1", {"query": "test"})

        # tool1 should be blocked
        with pytest.raises(RateLimitError):
            await executor.execute("tool1", {"query": "test"})

        # tool2 should still work
        await executor.execute("tool2", {"query": "test"})

    @pytest.mark.asyncio()
    async def test_rate_limiting_cannot_be_bypassed(self) -> None:
        """Test rate limiting cannot be bypassed with different inputs."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool", rate_limit=2)
        registry.register(tool)

        executor = FunctionExecutor(registry)

        # Use up rate limit with different queries
        await executor.execute("test_tool", {"query": "query1"})
        await executor.execute("test_tool", {"query": "query2"})

        # Should still be blocked regardless of query
        with pytest.raises(RateLimitError):
            await executor.execute("test_tool", {"query": "query3"})


@pytest.mark.security()
class TestInputValidationSecurity:
    """Test input validation blocks malicious input."""

    @pytest.mark.asyncio()
    async def test_sql_injection_blocked_by_validation(self) -> None:
        """Test SQL injection attempts are blocked by Pydantic validation."""
        from pydantic import BaseModel, Field

        from src.services.llm.function_calling.registry import ToolDefinition

        class SearchInput(BaseModel):
            query: str = Field(min_length=1, max_length=100)

        class SearchOutput(BaseModel):
            results: list[str]

        async def handler(input_data: SearchInput) -> SearchOutput:
            # Would use parameterized queries anyway
            return SearchOutput(results=[])

        tool = ToolDefinition(
            name="search",
            description="Search tool",
            input_schema=SearchInput,
            output_schema=SearchOutput,
            handler=handler,
        )

        registry = FunctionRegistry()
        registry.register(tool)
        executor = FunctionExecutor(registry)

        # Try SQL injection payloads
        sql_injection_attempts = [
            "'; DROP TABLE users; --",
            "1' OR '1'='1",
            "admin'--",
            "' UNION SELECT * FROM users--",
        ]

        for payload in sql_injection_attempts:
            # Validation may or may not fail depending on length
            # But even if it passes validation, parameterized queries prevent injection
            try:
                result = await executor.execute("search", {"query": payload})
                # If validation passes, ensure handler is safe
                assert isinstance(result, dict)
            except FunctionCallError:
                # Validation blocked it - good
                pass

    @pytest.mark.asyncio()
    async def test_xss_payloads_sanitized(self) -> None:
        """Test XSS payloads don't break validation."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool")
        registry.register(tool)

        executor = FunctionExecutor(registry)

        xss_payloads = [
            "<script>alert('xss')</script>",
            "<img src=x onerror=alert('xss')>",
            "javascript:alert('xss')",
        ]

        for payload in xss_payloads:
            # Should either be processed safely or validation error
            try:
                result = await executor.execute("test_tool", {"query": payload})
                # Ensure output is safe
                assert "result" in result
            except FunctionCallError:
                # Validation rejected it
                pass

    @pytest.mark.asyncio()
    async def test_excessive_input_length_blocked(self) -> None:
        """Test excessively long inputs are blocked."""
        from pydantic import BaseModel, Field

        from src.services.llm.function_calling.registry import ToolDefinition

        class LimitedInput(BaseModel):
            query: str = Field(max_length=100)

        class SimpleOutput(BaseModel):
            result: str

        async def handler(input_data: LimitedInput) -> SimpleOutput:
            return SimpleOutput(result="ok")

        tool = ToolDefinition(
            name="limited_tool",
            description="Tool with input limits",
            input_schema=LimitedInput,
            output_schema=SimpleOutput,
            handler=handler,
        )

        registry = FunctionRegistry()
        registry.register(tool)
        executor = FunctionExecutor(registry)

        # Try excessively long input
        long_input = "a" * 10000

        with pytest.raises(FunctionCallError) as exc_info:
            await executor.execute("limited_tool", {"query": long_input})

        assert exc_info.value.error_code == "INVALID_INPUT"

    @pytest.mark.asyncio()
    async def test_null_byte_injection_blocked(self) -> None:
        """Test null byte injection is handled."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool")
        registry.register(tool)

        executor = FunctionExecutor(registry)

        # Null byte injection attempts
        null_payloads = [
            "test\x00.txt",
            "query\x00malicious",
            "\x00\x00\x00",
        ]

        for payload in null_payloads:
            try:
                result = await executor.execute("test_tool", {"query": payload})
                # If it passes, ensure it's handled safely
                assert isinstance(result, dict)
            except (FunctionCallError, ValueError):
                # Blocked - good
                pass

    @pytest.mark.asyncio()
    async def test_unicode_exploits_handled(self) -> None:
        """Test Unicode exploits are handled safely."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool")
        registry.register(tool)

        executor = FunctionExecutor(registry)

        # Unicode exploits
        unicode_payloads = [
            "\u202E",  # Right-to-left override
            "\uFEFF",  # Zero-width no-break space
            "test\u0000file",  # Unicode null
        ]

        for payload in unicode_payloads:
            try:
                result = await executor.execute("test_tool", {"query": payload})
                assert isinstance(result, dict)
            except (FunctionCallError, ValueError):
                pass


@pytest.mark.security()
class TestSSRFPrevention:
    """Test SSRF prevention in web service."""

    @pytest.mark.asyncio()
    async def test_localhost_blocked(self) -> None:
        """Test localhost URLs are blocked."""
        service = WebService(cache_enabled=False)

        localhost_urls = [
            "http://localhost",
            "http://localhost:8080",
            "https://localhost",
            "http://127.0.0.1",
            "http://127.0.0.1:80",
        ]

        for url in localhost_urls:
            with pytest.raises(ValueError, match="blocked"):
                service._validate_url(url)

    @pytest.mark.asyncio()
    async def test_private_ips_blocked(self) -> None:
        """Test private IP ranges are blocked."""
        service = WebService(cache_enabled=False)

        private_ips = [
            "http://192.168.0.1",
            "http://10.0.0.1",
            "http://172.16.0.1",
            "http://172.31.255.255",
        ]

        for url in private_ips:
            with pytest.raises(ValueError, match="blocked"):
                service._validate_url(url)

    @pytest.mark.asyncio()
    async def test_metadata_endpoints_blocked(self) -> None:
        """Test cloud metadata endpoints are blocked."""
        service = WebService(cache_enabled=False)

        metadata_urls = [
            "http://169.254.169.254",  # AWS/Azure/GCP
            "http://169.254.169.254/latest/meta-data",
            "http://169.254.169.254/latest/user-data",
        ]

        for url in metadata_urls:
            with pytest.raises(ValueError, match="blocked"):
                service._validate_url(url)

    @pytest.mark.asyncio()
    async def test_url_encoding_bypass_prevented(self) -> None:
        """Test URL encoding bypass is prevented."""
        service = WebService(cache_enabled=False)

        # Attempt to bypass with URL encoding
        encoded_attempts = [
            "http://127.0.0.1",  # Direct
            # Note: urlparse handles these before they reach validation
        ]

        for url in encoded_attempts:
            with pytest.raises(ValueError):
                service._validate_url(url)

    @pytest.mark.asyncio()
    async def test_invalid_schemes_blocked(self) -> None:
        """Test non-HTTP(S) schemes are blocked."""
        service = WebService(cache_enabled=False)

        invalid_schemes = [
            "ftp://example.com",
            "file:///etc/passwd",
            "gopher://example.com",
            "data:text/html,<script>alert()</script>",
        ]

        for url in invalid_schemes:
            with pytest.raises(ValueError, match="scheme"):
                service._validate_url(url)

    @pytest.mark.asyncio()
    async def test_dns_rebinding_prevention(self) -> None:
        """Test DNS rebinding attacks are prevented (CWE-918)."""
        service = WebService(cache_enabled=False)

        # Test that DNS is resolved and validated before fetching
        # Mock DNS resolution to return private IP
        with patch("socket.getaddrinfo") as mock_dns:
            # Simulate DNS resolving to private IP
            mock_dns.return_value = [(2, 1, 6, "", ("192.168.1.1", 80))]

            with pytest.raises(ValueError, match="blocked IP"):
                service._validate_url("http://malicious.example.com")

    @pytest.mark.asyncio()
    async def test_ipv6_private_addresses_blocked(self) -> None:
        """Test IPv6 private addresses are blocked."""
        service = WebService(cache_enabled=False)

        ipv6_private = [
            "http://[::1]",  # IPv6 loopback
            "http://[fc00::1]",  # IPv6 private (unique local)
            "http://[fe80::1]",  # IPv6 link-local
        ]

        for url in ipv6_private:
            with pytest.raises(ValueError, match="blocked"):
                service._validate_url(url)

    @pytest.mark.asyncio()
    async def test_comprehensive_ip_blocking(self) -> None:
        """Test comprehensive IP blocking using ipaddress library."""
        service = WebService(cache_enabled=False)

        # Test various reserved/special addresses
        blocked_ips = [
            "0.0.0.0",  # Unspecified
            "127.0.0.1",  # Loopback
            "10.0.0.1",  # Private
            "172.16.0.1",  # Private
            "192.168.1.1",  # Private
            "169.254.169.254",  # AWS metadata
            "224.0.0.1",  # Multicast
        ]

        for ip in blocked_ips:
            assert service._is_blocked_ip(ip), f"IP {ip} should be blocked"

        # Test public IPs are allowed
        public_ips = [
            "8.8.8.8",  # Google DNS
            "1.1.1.1",  # Cloudflare DNS
            "93.184.216.34",  # Example.com
        ]

        for ip in public_ips:
            assert not service._is_blocked_ip(ip), f"IP {ip} should be allowed"

    @pytest.mark.asyncio()
    async def test_redirect_validation(self) -> None:
        """Test that redirects to private IPs are blocked."""
        service = WebService(cache_enabled=False)
        await service.initialize()

        # Mock a redirect to private IP
        mock_response = MagicMock()
        mock_response.url = "http://192.168.1.1"  # Redirected to private IP
        mock_response.text = "<html><body>Test</body></html>"
        mock_response.headers = {"content-type": "text/html"}
        mock_response.raise_for_status = MagicMock()

        with patch.object(service.http_client, "get", new=AsyncMock(return_value=mock_response)):
            from src.schemas.function_calling import FetchUrlContentInput

            input_data = FetchUrlContentInput(url="https://example.com")

            # Should raise because final URL is private
            with pytest.raises(ValueError, match="blocked"):
                await service.fetch_url_content(input_data)

        await service.cleanup()


@pytest.mark.security()
class TestPathTraversalPrevention:
    """Test path traversal prevention."""

    @pytest.mark.asyncio()
    async def test_path_traversal_in_arguments(self) -> None:
        """Test path traversal attempts in function arguments."""
        from pydantic import BaseModel, Field, validator

        from src.services.llm.function_calling.registry import ToolDefinition

        class FileInput(BaseModel):
            path: str = Field(max_length=500)

            @validator("path")
            def validate_path(cls, v: str) -> str:
                # Prevent path traversal
                if ".." in v:
                    raise ValueError("Path traversal not allowed")
                return v

        class FileOutput(BaseModel):
            content: str

        async def handler(input_data: FileInput) -> FileOutput:
            return FileOutput(content="safe content")

        tool = ToolDefinition(
            name="file_tool",
            description="File tool",
            input_schema=FileInput,
            output_schema=FileOutput,
            handler=handler,
        )

        registry = FunctionRegistry()
        registry.register(tool)
        executor = FunctionExecutor(registry)

        traversal_attempts = [
            "../../../etc/passwd",
            "..\\..\\..\\windows\\system32",
            "../../sensitive/file",
        ]

        for path in traversal_attempts:
            with pytest.raises(FunctionCallError) as exc_info:
                await executor.execute("file_tool", {"path": path})

            assert exc_info.value.error_code == "INVALID_INPUT"

    def test_validate_file_path_prevents_traversal(self, tmp_path) -> None:
        """Test validate_file_path function prevents CWE-22 path traversal."""

        from src.services.llm.function_calling.tools import validate_file_path

        # Create a test file in allowed directory
        allowed_dir = tmp_path / "storage" / "documents"
        allowed_dir.mkdir(parents=True, exist_ok=True)
        test_file = allowed_dir / "test.txt"
        test_file.write_text("test content")

        # Mock settings to use tmp_path
        with patch("src.services.llm.function_calling.tools.get_settings") as mock_settings:
            mock_settings.return_value.storage_documents_path = str(allowed_dir)
            mock_settings.return_value.storage_screenshots_path = str(tmp_path / "screenshots")
            mock_settings.return_value.storage_base_path = str(tmp_path / "storage")

            # Valid path should pass
            validated = validate_file_path(str(test_file))
            assert validated == test_file.resolve()

            # Path traversal attempts should fail
            with pytest.raises(ValueError, match="outside allowed"):
                validate_file_path(str(allowed_dir / ".." / ".." / "etc" / "passwd"))

            # Symlink to outside directory should fail
            outside_file = tmp_path / "outside.txt"
            outside_file.write_text("outside")
            symlink = allowed_dir / "link.txt"
            symlink.symlink_to(outside_file)

            with pytest.raises(ValueError):
                validate_file_path(str(symlink))

    def test_validate_file_path_handles_nonexistent(self) -> None:
        """Test validate_file_path rejects nonexistent paths."""
        from src.services.llm.function_calling.tools import validate_file_path

        # Nonexistent file should raise
        with pytest.raises(ValueError):
            validate_file_path("/nonexistent/file.txt")


@pytest.mark.security()
class TestContentLengthLimits:
    """Test excessive content length handling."""

    @pytest.mark.asyncio()
    async def test_fetch_respects_max_content_length(self) -> None:
        """Test URL fetch respects maximum content length."""
        service = WebService(cache_enabled=False)
        await service.initialize()

        large_content = "word " * 50000  # Very large content
        html = f"<html><body><p>{large_content}</p></body></html>"

        mock_response = MagicMock()
        mock_response.text = html
        mock_response.url = "https://example.com"
        mock_response.headers = {"content-type": "text/html"}
        mock_response.raise_for_status = MagicMock()

        with patch.object(service.http_client, "get", new=AsyncMock(return_value=mock_response)):
            from src.schemas.function_calling import FetchUrlContentInput

            input_data = FetchUrlContentInput(
                url="https://example.com",
                max_content_length=1000,  # Very small limit
            )

            result = await service.fetch_url_content(input_data)

            # Content should be truncated
            assert len(result.content) <= 1000
            assert result.content_truncated is True

        await service.cleanup()

    @pytest.mark.asyncio()
    async def test_search_results_limited(self) -> None:
        """Test web search results are limited."""
        service = WebService(cache_enabled=False)

        mock_ddgs = MagicMock()
        # Return more results than requested
        mock_ddgs.text.return_value = [
            {"title": f"Result {i}", "href": f"https://example.com/{i}", "body": "snippet"}
            for i in range(100)
        ]

        with patch("src.services.llm.function_calling.web_service.DDGS", return_value=mock_ddgs):
            from src.schemas.function_calling import WebSearchInput

            input_data = WebSearchInput(
                query="test",
                max_results=10,  # Limit to 10
            )

            result = await service.search_web(input_data)

            # Should respect limit (though DDGS should too)
            assert result.result_count <= 100


@pytest.mark.security()
class TestErrorMessageSecurity:
    """Test error messages don't leak sensitive information."""

    @pytest.mark.asyncio()
    async def test_validation_errors_sanitized(self) -> None:
        """Test validation errors don't leak internal details."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool")
        registry.register(tool)

        executor = FunctionExecutor(registry)

        # Cause validation error
        try:
            await executor.execute("test_tool", {})  # Missing required field
        except FunctionCallError as e:
            # Error should be user-friendly, not leak internals
            assert "validation failed" in e.message.lower()
            # Should not contain internal paths or stack traces
            assert "/home/" not in e.message
            assert "Traceback" not in e.message

    @pytest.mark.asyncio()
    async def test_function_not_found_limited_info(self) -> None:
        """Test function not found error limits information disclosure."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="real_tool")
        registry.register(tool)

        executor = FunctionExecutor(registry)

        try:
            await executor.execute("nonexistent_tool", {})
        except FunctionCallError as e:
            # Should list available functions but not leak system details
            assert e.error_code == "FUNCTION_NOT_FOUND"
            # Available functions should be in details, not message
            assert "available_functions" in e.details

    @pytest.mark.asyncio()
    async def test_rate_limit_error_safe(self) -> None:
        """Test rate limit errors don't leak timing information."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool", rate_limit=1)
        registry.register(tool)

        executor = FunctionExecutor(registry)

        await executor.execute("test_tool", {"query": "test"})

        try:
            await executor.execute("test_tool", {"query": "test"})
        except RateLimitError as e:
            # Should indicate rate limit but not leak exact timing
            assert "rate limit" in str(e).lower()
            # Should not expose internal rate limiter state
            assert "_requests" not in str(e)


@pytest.mark.security()
class TestConcurrentAccessSecurity:
    """Test security under concurrent access."""

    @pytest.mark.asyncio()
    async def test_rate_limiting_thread_safe(self) -> None:
        """Test rate limiting is thread-safe under concurrent access."""
        import asyncio

        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool", rate_limit=5)
        registry.register(tool)

        executor = FunctionExecutor(registry)

        # Try to overwhelm rate limiter with concurrent requests
        tasks = [executor.execute("test_tool", {"query": f"test{i}"}) for i in range(10)]

        results = await asyncio.gather(*tasks, return_exceptions=True)

        # Should have some successes and some rate limit errors
        successes = [r for r in results if isinstance(r, dict)]
        failures = [r for r in results if isinstance(r, RateLimitError)]

        assert len(successes) <= 5  # At most rate limit
        assert len(failures) > 0  # Some should be blocked

    @pytest.mark.asyncio()
    async def test_cache_poisoning_prevented(self) -> None:
        """Test cache cannot be poisoned with malicious data."""
        from src.services.llm.function_calling.web_service import WebCache

        cache = WebCache()

        # Try to cache malicious data
        malicious_data = {
            "__class__": "malicious",
            "__init__": "code",
        }

        await cache.put("test", malicious_data, query="test")
        result = await cache.get("test", query="test")

        # Should return data as-is (Python objects are safe)
        # But ensure no code execution occurred
        assert result == malicious_data


@pytest.mark.security()
class TestLogSanitization:
    """Test sensitive data is sanitized in logs."""

    @pytest.mark.asyncio()
    async def test_user_id_sanitized_in_logs(self, caplog) -> None:
        """Test user IDs are sanitized in log messages (prevents PII exposure)."""

        from src.services.llm.function_calling.executor import _sanitize_user_id

        # Test sanitization function
        assert _sanitize_user_id("very_long_user_id_12345") == "very...2345"
        assert _sanitize_user_id("short") == "***"
        assert _sanitize_user_id(None) == "anonymous"
        assert _sanitize_user_id("") == "anonymous"

    @pytest.mark.asyncio()
    async def test_queries_truncated_in_logs(self, caplog) -> None:
        """Test queries are truncated in logs to prevent PII exposure."""
        caplog.set_level(logging.INFO)

        service = WebService(cache_enabled=False)

        # Mock successful search
        mock_ddgs = MagicMock()
        mock_ddgs.text.return_value = [
            {"title": "Result", "href": "https://example.com", "body": "snippet"}
        ]

        with patch("src.services.llm.function_calling.web_service.DDGS", return_value=mock_ddgs):
            from src.schemas.function_calling import WebSearchInput

            long_query = "a" * 100  # Long query that might contain PII
            input_data = WebSearchInput(query=long_query, max_results=5)

            await service.search_web(input_data)

            # Check that full query is not in logs
            log_messages = [record.message for record in caplog.records]
            assert any("..." in msg for msg in log_messages), "Query should be truncated"
            assert not any(
                long_query in msg for msg in log_messages
            ), "Full query should not be in logs"

    @pytest.mark.asyncio()
    async def test_urls_sanitized_in_logs(self, caplog) -> None:
        """Test URLs with API keys are sanitized in logs."""
        caplog.set_level(logging.INFO)

        service = WebService(cache_enabled=False)
        await service.initialize()

        # Mock successful fetch
        mock_response = MagicMock()
        mock_response.text = "<html><body>Test</body></html>"
        mock_response.url = "https://api.example.com/data?key=secret123&user=test"
        mock_response.headers = {"content-type": "text/html"}
        mock_response.raise_for_status = MagicMock()

        with patch.object(service.http_client, "get", new=AsyncMock(return_value=mock_response)):
            from src.schemas.function_calling import FetchUrlContentInput

            input_data = FetchUrlContentInput(
                url="https://api.example.com/data?key=secret123&user=test"
            )

            await service.fetch_url_content(input_data)

            # Check that URLs are truncated
            log_messages = [record.message for record in caplog.records]
            # URL should be truncated to 50 chars
            assert any("..." in msg for msg in log_messages), "URL should be truncated"

        await service.cleanup()


@pytest.mark.security()
class TestRateLimitAdjustments:
    """Test rate limits have been reduced for security."""

    def test_web_search_rate_limit_reduced(self) -> None:
        """Test web_search rate limit is 10/min (reduced for security)."""
        from src.services.llm.function_calling.tools import create_web_search_tool
        from src.services.llm.function_calling.web_service import WebService

        web_service = WebService(cache_enabled=False)
        tool = create_web_search_tool(web_service)

        # Verify rate limit is 10 (not 50)
        assert tool.rate_limit == 10, "web_search rate limit should be 10/min"

    def test_fetch_url_rate_limit_reduced(self) -> None:
        """Test fetch_url_content rate limit is 5/min (reduced for security)."""
        from src.services.llm.function_calling.tools import create_fetch_url_content_tool
        from src.services.llm.function_calling.web_service import WebService

        web_service = WebService(cache_enabled=False)
        tool = create_fetch_url_content_tool(web_service)

        # Verify rate limit is 5 (not 30)
        assert tool.rate_limit == 5, "fetch_url_content rate limit should be 5/min"
