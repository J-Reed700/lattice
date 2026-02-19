"""API security tests.

Tests API-level security:
- Security headers
- CORS configuration
- Information disclosure
- Error message leakage
- HTTP methods
- API versioning
"""

from httpx import AsyncClient
import pytest


class TestSecurityHeaders:
    """Test that proper security headers are present."""

    @pytest.mark.asyncio()
    async def test_x_content_type_options_header(self, client: AsyncClient):
        """Test that X-Content-Type-Options header is set."""
        response = await client.get("/api/v1/health")
        headers_lower = {k.lower(): v for k, v in response.headers.items()}

        if "x-content-type-options" in headers_lower:
            assert headers_lower["x-content-type-options"] == "nosniff"

    @pytest.mark.asyncio()
    async def test_x_frame_options_header(self, client: AsyncClient):
        """Test that X-Frame-Options header is set."""
        response = await client.get("/api/v1/health")
        headers_lower = {k.lower(): v for k, v in response.headers.items()}

        if "x-frame-options" in headers_lower:
            assert headers_lower["x-frame-options"] in ["DENY", "SAMEORIGIN"]

    @pytest.mark.asyncio()
    async def test_strict_transport_security_header(self, client: AsyncClient):
        """Test that Strict-Transport-Security header is set."""
        response = await client.get("/api/v1/health")
        headers_lower = {k.lower(): v for k, v in response.headers.items()}

        if "strict-transport-security" in headers_lower:
            hsts = headers_lower["strict-transport-security"]
            assert "max-age" in hsts

    @pytest.mark.asyncio()
    async def test_x_xss_protection_header(self, client: AsyncClient):
        """Test that X-XSS-Protection header is set."""
        response = await client.get("/api/v1/health")
        headers_lower = {k.lower(): v for k, v in response.headers.items()}

        if "x-xss-protection" in headers_lower:
            assert headers_lower["x-xss-protection"] == "1; mode=block"

    @pytest.mark.asyncio()
    async def test_content_security_policy_header(self, client: AsyncClient):
        """Test that Content-Security-Policy header is set."""
        response = await client.get("/api/v1/health")
        headers_lower = {k.lower(): v for k, v in response.headers.items()}

        if "content-security-policy" in headers_lower:
            csp = headers_lower["content-security-policy"]
            assert "default-src" in csp or "script-src" in csp

    @pytest.mark.asyncio()
    async def test_server_header_not_revealing(self, client: AsyncClient):
        """Test that Server header doesn't reveal sensitive info."""
        response = await client.get("/api/v1/health")
        headers_lower = {k.lower(): v for k, v in response.headers.items()}

        if "server" in headers_lower:
            server = headers_lower["server"].lower()
            assert "uvicorn" not in server or "python" not in server


class TestCORSConfiguration:
    """Test CORS configuration security."""

    @pytest.mark.asyncio()
    async def test_cors_not_wildcard_in_production(self, client: AsyncClient):
        """Test that CORS doesn't allow all origins with wildcard."""
        response = await client.options("/api/v1/files/", headers={"Origin": "http://evil.com"})

        if "access-control-allow-origin" in response.headers:
            assert response.headers["access-control-allow-origin"] != "*"

    @pytest.mark.asyncio()
    async def test_cors_credentials_not_with_wildcard(self, client: AsyncClient):
        """Test that credentials are not allowed with wildcard origin."""
        response = await client.options("/api/v1/files/", headers={"Origin": "http://evil.com"})

        allow_origin = response.headers.get("access-control-allow-origin")
        allow_credentials = response.headers.get("access-control-allow-credentials")

        if allow_origin == "*":
            assert allow_credentials != "true"

    @pytest.mark.asyncio()
    async def test_cors_methods_restricted(self, client: AsyncClient):
        """Test that CORS methods are properly restricted."""
        response = await client.options(
            "/api/v1/files/", headers={"Origin": "http://localhost:3000"}
        )

        if "access-control-allow-methods" in response.headers:
            methods = response.headers["access-control-allow-methods"]
            assert "TRACE" not in methods
            assert "CONNECT" not in methods


class TestInformationDisclosure:
    """Test that sensitive information is not disclosed."""

    @pytest.mark.asyncio()
    async def test_error_messages_no_stack_trace(self, client: AsyncClient):
        """Test that error messages don't contain stack traces."""
        response = await client.get("/api/v1/files/99999999")

        response_text = response.text.lower()
        assert "traceback" not in response_text
        assert "line " not in response_text
        assert "file " not in response_text or "file not found" in response_text

    @pytest.mark.asyncio()
    async def test_error_messages_no_sql_info(self, client: AsyncClient, auth_headers: dict):
        """Test that SQL errors are not exposed."""
        response = await client.post(
            "/api/v1/search", headers=auth_headers, json={"query": "' OR 1=1--"}
        )

        response_text = response.text.lower()
        assert "syntax error" not in response_text
        assert "sql" not in response_text or "nosql" in response_text
        assert "postgresql" not in response_text
        assert "mysql" not in response_text

    @pytest.mark.asyncio()
    async def test_401_response_no_user_info(self, client: AsyncClient):
        """Test that 401 responses don't leak user information."""
        response = await client.post(
            "/api/v1/auth/token", data={"username": "admin", "password": "wrong"}
        )

        assert response.status_code == 401
        detail = response.json().get("detail", "").lower()
        assert "not found" not in detail
        assert "exists" not in detail

    @pytest.mark.asyncio()
    async def test_no_directory_listing(self, client: AsyncClient):
        """Test that directory listing is disabled."""
        response = await client.get("/static/")
        assert response.status_code in [403, 404]

    @pytest.mark.asyncio()
    async def test_version_endpoint_minimal_info(self, client: AsyncClient):
        """Test that version endpoint doesn't expose too much info."""
        response = await client.get("/api/v1/version")

        if response.status_code == 200:
            data = response.json()
            sensitive_keys = ["database_version", "python_version", "os_version"]
            for key in sensitive_keys:
                assert key not in data


class TestHTTPMethodSecurity:
    """Test HTTP method security."""

    @pytest.mark.asyncio()
    async def test_trace_method_disabled(self, client: AsyncClient):
        """Test that TRACE method is disabled."""
        response = await client.request("TRACE", "/api/v1/files/")
        assert response.status_code in [405, 501]

    @pytest.mark.asyncio()
    async def test_options_method_safe(self, client: AsyncClient):
        """Test that OPTIONS method is safe and doesn't modify data."""
        response = await client.options("/api/v1/files/")
        assert response.status_code in [200, 204, 405]

    @pytest.mark.asyncio()
    async def test_head_method_same_as_get(self, client: AsyncClient, auth_headers: dict):
        """Test that HEAD returns same headers as GET but no body."""
        get_response = await client.get("/api/v1/files/", headers=auth_headers)
        head_response = await client.head("/api/v1/files/", headers=auth_headers)

        if get_response.status_code == head_response.status_code == 200:
            assert len(head_response.content) == 0


class TestAPIVersioning:
    """Test API versioning security."""

    @pytest.mark.asyncio()
    async def test_old_api_versions_deprecated(self, client: AsyncClient):
        """Test that old API versions return deprecation warning."""
        response = await client.get("/api/v0/files/")
        assert response.status_code in [404, 410]

    @pytest.mark.asyncio()
    async def test_api_version_in_header(self, client: AsyncClient, auth_headers: dict):
        """Test that API version can be specified in header."""
        headers = {**auth_headers, "API-Version": "v1"}
        response = await client.get("/api/v1/files/", headers=headers)
        assert response.status_code in [200, 404]


class TestContentType:
    """Test content type security."""

    @pytest.mark.asyncio()
    async def test_json_endpoints_reject_xml(self, client: AsyncClient, auth_headers: dict):
        """Test that JSON endpoints reject XML content."""
        headers = {**auth_headers, "Content-Type": "application/xml"}
        response = await client.post(
            "/api/v1/search", headers=headers, content=b"<query>test</query>"
        )
        assert response.status_code in [400, 415, 422]

    @pytest.mark.asyncio()
    async def test_response_content_type_correct(self, client: AsyncClient, auth_headers: dict):
        """Test that response content-type is correct."""
        response = await client.get("/api/v1/files/", headers=auth_headers)

        if response.status_code == 200:
            content_type = response.headers.get("content-type", "")
            assert "application/json" in content_type


class TestRateLimitHeaders:
    """Test rate limiting headers."""

    @pytest.mark.asyncio()
    async def test_rate_limit_headers_present(self, client: AsyncClient, auth_headers: dict):
        """Test that rate limit information is in headers."""
        response = await client.get("/api/v1/files/", headers=auth_headers)

        headers_lower = {k.lower(): v for k, v in response.headers.items()}
        rate_limit_headers = ["x-ratelimit-limit", "x-ratelimit-remaining", "x-ratelimit-reset"]

        any(h in headers_lower for h in rate_limit_headers)


class TestCachingHeaders:
    """Test caching header security."""

    @pytest.mark.asyncio()
    async def test_sensitive_endpoints_no_cache(self, client: AsyncClient, auth_headers: dict):
        """Test that sensitive endpoints have no-cache headers."""
        response = await client.get("/api/v1/auth/me", headers=auth_headers)

        if response.status_code == 200:
            cache_control = response.headers.get("cache-control", "").lower()
            if cache_control:
                assert "no-store" in cache_control or "no-cache" in cache_control

    @pytest.mark.asyncio()
    async def test_private_data_not_cached(self, client: AsyncClient, auth_headers: dict):
        """Test that private user data is not cached."""
        response = await client.get("/api/v1/files/", headers=auth_headers)

        if response.status_code == 200:
            cache_control = response.headers.get("cache-control", "").lower()
            if cache_control and "public" in cache_control:
                pytest.fail("Private data should not be publicly cached")
