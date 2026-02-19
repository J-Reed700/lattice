"""
Comprehensive tests for CSRF protection middleware.

Tests cover:
- Token generation (uniqueness, length, cryptographic security)
- Token verification (constant-time comparison)
- CSRF protection dependency (safe methods, exempt paths, validation)
- CSRFMiddleware ASGI middleware (cookie setting, various configurations)
- Error handling and edge cases
"""

from unittest.mock import AsyncMock, Mock

from fastapi import Request, status
import pytest
from starlette.types import Receive, Scope, Send

from src.middleware.csrf import (
    CSRF_COOKIE_NAME,
    CSRF_EXEMPT_PATHS,
    CSRF_FORM_FIELD,
    CSRF_HEADER_NAME,
    CSRF_SAFE_METHODS,
    CSRF_TOKEN_LENGTH,
    CSRFError,
    CSRFMiddleware,
    csrf_protect,
    generate_csrf_token,
    get_csrf_token,
    verify_csrf_token,
)


@pytest.mark.unit()
class TestCSRFTokenGeneration:
    """Test CSRF token generation."""

    def test_generate_unique_tokens(self) -> None:
        """Tokens should be unique on each generation."""
        token1 = generate_csrf_token()
        token2 = generate_csrf_token()
        assert token1 != token2

    def test_token_length(self) -> None:
        """Tokens should be 64 characters (32 bytes hex)."""
        token = generate_csrf_token()
        assert len(token) == CSRF_TOKEN_LENGTH * 2

    def test_token_is_hex(self) -> None:
        """Tokens should be valid hexadecimal strings."""
        token = generate_csrf_token()
        try:
            int(token, 16)
        except ValueError:
            pytest.fail("Token is not valid hexadecimal")

    def test_multiple_tokens_unique(self) -> None:
        """Generate 100 tokens and ensure all are unique."""
        tokens = {generate_csrf_token() for _ in range(100)}
        assert len(tokens) == 100


@pytest.mark.unit()
class TestCSRFTokenVerification:
    """Test CSRF token verification."""

    def test_verify_matching_tokens(self) -> None:
        """Valid matching tokens should pass verification."""
        token = generate_csrf_token()
        assert verify_csrf_token(token, token) is True

    def test_verify_different_tokens(self) -> None:
        """Different tokens should fail verification."""
        token1 = generate_csrf_token()
        token2 = generate_csrf_token()
        assert verify_csrf_token(token1, token2) is False

    def test_verify_none_cookie(self) -> None:
        """None cookie token should fail verification."""
        token = generate_csrf_token()
        assert verify_csrf_token(None, token) is False

    def test_verify_none_request(self) -> None:
        """None request token should fail verification."""
        token = generate_csrf_token()
        assert verify_csrf_token(token, None) is False

    def test_verify_both_none(self) -> None:
        """Both None tokens should fail verification."""
        assert verify_csrf_token(None, None) is False

    def test_verify_empty_strings(self) -> None:
        """Empty string tokens should fail verification."""
        assert verify_csrf_token("", "") is False

    def test_verify_different_lengths(self) -> None:
        """Tokens with different lengths should fail verification."""
        token = generate_csrf_token()
        assert verify_csrf_token(token, token[:-1]) is False

    def test_verify_case_sensitive(self) -> None:
        """Token verification should be case-sensitive."""
        token = generate_csrf_token()
        assert verify_csrf_token(token, token.upper()) is False


@pytest.mark.unit()
class TestGetCSRFToken:
    """Test get_csrf_token function."""

    @pytest.mark.asyncio()
    async def test_returns_existing_cookie_token(self) -> None:
        """Should return existing token from cookie."""
        existing_token = generate_csrf_token()
        request = Mock(spec=Request)
        request.cookies = {CSRF_COOKIE_NAME: existing_token}
        request.state = Mock()

        token = await get_csrf_token(request)

        assert token == existing_token

    @pytest.mark.asyncio()
    async def test_generates_new_token_if_missing(self) -> None:
        """Should generate new token if cookie is missing."""
        request = Mock(spec=Request)
        request.cookies = {}
        request.state = Mock()

        token = await get_csrf_token(request)

        assert len(token) == CSRF_TOKEN_LENGTH * 2
        assert hasattr(request.state, "new_csrf_token")
        assert request.state.new_csrf_token == token

    @pytest.mark.asyncio()
    async def test_token_is_hex(self) -> None:
        """Generated token should be valid hex."""
        request = Mock(spec=Request)
        request.cookies = {}
        request.state = Mock()

        token = await get_csrf_token(request)

        try:
            int(token, 16)
        except ValueError:
            pytest.fail("Generated token is not valid hexadecimal")


@pytest.mark.unit()
class TestCSRFProtect:
    """Test csrf_protect dependency function."""

    @pytest.mark.asyncio()
    async def test_safe_methods_bypass_protection(self) -> None:
        """Safe HTTP methods should bypass CSRF protection."""
        for method in CSRF_SAFE_METHODS:
            request = Mock(spec=Request)
            request.method = method

            await csrf_protect(request)

    @pytest.mark.asyncio()
    async def test_exempt_paths_bypass_protection(self) -> None:
        """Exempt paths should bypass CSRF protection."""
        for path in CSRF_EXEMPT_PATHS:
            request = Mock(spec=Request)
            request.method = "POST"
            request.url = Mock()
            request.url.path = path

            await csrf_protect(request)

    @pytest.mark.asyncio()
    async def test_missing_cookie_raises_error(self) -> None:
        """Request without CSRF cookie should raise CSRFError."""
        request = Mock(spec=Request)
        request.method = "POST"
        request.url = Mock()
        request.url.path = "/api/v1/documents"
        request.cookies = {}
        request.headers = {}

        with pytest.raises(CSRFError) as exc_info:
            await csrf_protect(request)

        assert exc_info.value.status_code == status.HTTP_403_FORBIDDEN
        assert "CSRF token missing in cookie" in exc_info.value.detail

    @pytest.mark.asyncio()
    async def test_missing_header_and_form_raises_error(self) -> None:
        """Request without CSRF token in header or form should raise CSRFError."""
        token = generate_csrf_token()
        request = Mock(spec=Request)
        request.method = "POST"
        request.url = Mock()
        request.url.path = "/api/v1/documents"
        request.cookies = {CSRF_COOKIE_NAME: token}
        request.headers = {}
        request.form = AsyncMock(side_effect=Exception("No form data"))

        with pytest.raises(CSRFError) as exc_info:
            await csrf_protect(request)

        assert exc_info.value.status_code == status.HTTP_403_FORBIDDEN
        assert "CSRF token required" in exc_info.value.detail

    @pytest.mark.asyncio()
    async def test_valid_header_token_passes(self) -> None:
        """Valid token in header should pass verification."""
        token = generate_csrf_token()
        request = Mock(spec=Request)
        request.method = "POST"
        request.url = Mock()
        request.url.path = "/api/v1/documents"
        request.cookies = {CSRF_COOKIE_NAME: token}
        request.headers = {CSRF_HEADER_NAME: token}

        await csrf_protect(request)

    @pytest.mark.asyncio()
    async def test_invalid_token_raises_error(self) -> None:
        """Mismatched tokens should raise CSRFError."""
        cookie_token = generate_csrf_token()
        header_token = generate_csrf_token()

        request = Mock(spec=Request)
        request.method = "POST"
        request.url = Mock()
        request.url.path = "/api/v1/documents"
        request.cookies = {CSRF_COOKIE_NAME: cookie_token}
        request.headers = {CSRF_HEADER_NAME: header_token}

        with pytest.raises(CSRFError) as exc_info:
            await csrf_protect(request)

        assert exc_info.value.status_code == status.HTTP_403_FORBIDDEN
        assert "CSRF token validation failed" in exc_info.value.detail

    @pytest.mark.asyncio()
    async def test_valid_form_token_passes(self) -> None:
        """Valid token in form data should pass verification."""
        token = generate_csrf_token()
        request = Mock(spec=Request)
        request.method = "POST"
        request.url = Mock()
        request.url.path = "/api/v1/documents"
        request.cookies = {CSRF_COOKIE_NAME: token}
        request.headers = {}

        form_data = {CSRF_FORM_FIELD: token}
        request.form = AsyncMock(return_value=form_data)

        await csrf_protect(request)

    @pytest.mark.asyncio()
    async def test_header_takes_precedence_over_form(self) -> None:
        """Header token should take precedence over form token."""
        token = generate_csrf_token()
        wrong_token = generate_csrf_token()

        request = Mock(spec=Request)
        request.method = "POST"
        request.url = Mock()
        request.url.path = "/api/v1/documents"
        request.cookies = {CSRF_COOKIE_NAME: token}
        request.headers = {CSRF_HEADER_NAME: token}

        form_data = {CSRF_FORM_FIELD: wrong_token}
        request.form = AsyncMock(return_value=form_data)

        await csrf_protect(request)


@pytest.mark.unit()
class TestCSRFError:
    """Test CSRFError exception."""

    def test_csrf_error_status_code(self) -> None:
        """CSRFError should have 403 status code."""
        error = CSRFError("Test error")
        assert error.status_code == status.HTTP_403_FORBIDDEN

    def test_csrf_error_detail(self) -> None:
        """CSRFError should include detail message."""
        detail = "Custom error message"
        error = CSRFError(detail)
        assert error.detail == detail


@pytest.mark.unit()
class TestCSRFMiddleware:
    """Test CSRFMiddleware ASGI middleware."""

    @pytest.mark.asyncio()
    async def test_non_http_scope_passes_through(self) -> None:
        """Non-HTTP requests should pass through unchanged."""
        app = AsyncMock()
        middleware = CSRFMiddleware(app)

        scope = {"type": "websocket"}
        receive = AsyncMock()
        send = AsyncMock()

        await middleware(scope, receive, send)

        app.assert_called_once_with(scope, receive, send)

    @pytest.mark.asyncio()
    async def test_sets_csrf_cookie_on_response(self) -> None:
        """Middleware should set CSRF cookie on HTTP responses."""
        AsyncMock()

        async def mock_app(scope: Scope, receive: Receive, send: Send) -> None:
            await send(
                {
                    "type": "http.response.start",
                    "status": 200,
                    "headers": [],
                }
            )

        middleware = CSRFMiddleware(
            mock_app,
            cookie_secure=True,
            cookie_samesite="Strict",
            cookie_httponly=False,
        )

        scope = {
            "type": "http",
            "method": "GET",
            "path": "/",
            "query_string": b"",
            "headers": [],
        }
        receive = AsyncMock()

        sent_messages = []

        async def capture_send(message: dict) -> None:
            sent_messages.append(message)

        await middleware(scope, receive, capture_send)

        assert len(sent_messages) > 0
        start_message = sent_messages[0]
        assert start_message["type"] == "http.response.start"

        headers = start_message["headers"]
        set_cookie_headers = [value for name, value in headers if name == b"set-cookie"]

        assert len(set_cookie_headers) > 0
        cookie = set_cookie_headers[0].decode()

        assert CSRF_COOKIE_NAME in cookie
        assert "Path=/" in cookie
        assert "SameSite=Strict" in cookie
        assert "Secure" in cookie
        assert "HttpOnly" not in cookie

    @pytest.mark.asyncio()
    async def test_cookie_with_httponly(self) -> None:
        """Middleware should set HttpOnly flag when configured."""

        async def mock_app(scope: Scope, receive: Receive, send: Send) -> None:
            await send(
                {
                    "type": "http.response.start",
                    "status": 200,
                    "headers": [],
                }
            )

        middleware = CSRFMiddleware(
            mock_app,
            cookie_httponly=True,
        )

        scope = {
            "type": "http",
            "method": "GET",
            "path": "/",
            "query_string": b"",
            "headers": [],
        }
        receive = AsyncMock()

        sent_messages = []

        async def capture_send(message: dict) -> None:
            sent_messages.append(message)

        await middleware(scope, receive, capture_send)

        start_message = sent_messages[0]
        headers = start_message["headers"]
        set_cookie_headers = [value for name, value in headers if name == b"set-cookie"]

        cookie = set_cookie_headers[0].decode()
        assert "HttpOnly" in cookie

    @pytest.mark.asyncio()
    async def test_cookie_with_domain(self) -> None:
        """Middleware should set Domain attribute when configured."""

        async def mock_app(scope: Scope, receive: Receive, send: Send) -> None:
            await send(
                {
                    "type": "http.response.start",
                    "status": 200,
                    "headers": [],
                }
            )

        middleware = CSRFMiddleware(
            mock_app,
            cookie_domain="example.com",
        )

        scope = {
            "type": "http",
            "method": "GET",
            "path": "/",
            "query_string": b"",
            "headers": [],
        }
        receive = AsyncMock()

        sent_messages = []

        async def capture_send(message: dict) -> None:
            sent_messages.append(message)

        await middleware(scope, receive, capture_send)

        start_message = sent_messages[0]
        headers = start_message["headers"]
        set_cookie_headers = [value for name, value in headers if name == b"set-cookie"]

        cookie = set_cookie_headers[0].decode()
        assert "Domain=example.com" in cookie

    @pytest.mark.asyncio()
    async def test_does_not_duplicate_csrf_cookie(self) -> None:
        """Middleware should not add duplicate CSRF cookie if already set."""

        async def mock_app(scope: Scope, receive: Receive, send: Send) -> None:
            await send(
                {
                    "type": "http.response.start",
                    "status": 200,
                    "headers": [(b"set-cookie", f"{CSRF_COOKIE_NAME}=existing_token".encode())],
                }
            )

        middleware = CSRFMiddleware(mock_app)

        scope = {
            "type": "http",
            "method": "GET",
            "path": "/",
            "query_string": b"",
            "headers": [],
        }
        receive = AsyncMock()

        sent_messages = []

        async def capture_send(message: dict) -> None:
            sent_messages.append(message)

        await middleware(scope, receive, capture_send)

        start_message = sent_messages[0]
        headers = start_message["headers"]
        set_cookie_headers = [value for name, value in headers if name == b"set-cookie"]

        assert len(set_cookie_headers) == 1

    @pytest.mark.asyncio()
    async def test_uses_existing_cookie_token(self) -> None:
        """Middleware should use existing cookie token if present."""
        existing_token = generate_csrf_token()

        async def mock_app(scope: Scope, receive: Receive, send: Send) -> None:
            await send(
                {
                    "type": "http.response.start",
                    "status": 200,
                    "headers": [],
                }
            )

        middleware = CSRFMiddleware(mock_app)

        scope = {
            "type": "http",
            "method": "GET",
            "path": "/",
            "query_string": b"",
            "headers": [(b"cookie", f"{CSRF_COOKIE_NAME}={existing_token}".encode())],
        }
        receive = AsyncMock()

        sent_messages = []

        async def capture_send(message: dict) -> None:
            sent_messages.append(message)

        await middleware(scope, receive, capture_send)

        start_message = sent_messages[0]
        headers = start_message["headers"]
        set_cookie_headers = [value for name, value in headers if name == b"set-cookie"]

        cookie = set_cookie_headers[0].decode()
        assert existing_token in cookie

    @pytest.mark.asyncio()
    async def test_samesite_lax(self) -> None:
        """Middleware should support SameSite=Lax."""

        async def mock_app(scope: Scope, receive: Receive, send: Send) -> None:
            await send(
                {
                    "type": "http.response.start",
                    "status": 200,
                    "headers": [],
                }
            )

        middleware = CSRFMiddleware(
            mock_app,
            cookie_samesite="Lax",
        )

        scope = {
            "type": "http",
            "method": "GET",
            "path": "/",
            "query_string": b"",
            "headers": [],
        }
        receive = AsyncMock()

        sent_messages = []

        async def capture_send(message: dict) -> None:
            sent_messages.append(message)

        await middleware(scope, receive, capture_send)

        start_message = sent_messages[0]
        headers = start_message["headers"]
        set_cookie_headers = [value for name, value in headers if name == b"set-cookie"]

        cookie = set_cookie_headers[0].decode()
        assert "SameSite=Lax" in cookie

    @pytest.mark.asyncio()
    async def test_insecure_cookie(self) -> None:
        """Middleware should support insecure cookies for development."""

        async def mock_app(scope: Scope, receive: Receive, send: Send) -> None:
            await send(
                {
                    "type": "http.response.start",
                    "status": 200,
                    "headers": [],
                }
            )

        middleware = CSRFMiddleware(
            mock_app,
            cookie_secure=False,
        )

        scope = {
            "type": "http",
            "method": "GET",
            "path": "/",
            "query_string": b"",
            "headers": [],
        }
        receive = AsyncMock()

        sent_messages = []

        async def capture_send(message: dict) -> None:
            sent_messages.append(message)

        await middleware(scope, receive, capture_send)

        start_message = sent_messages[0]
        headers = start_message["headers"]
        set_cookie_headers = [value for name, value in headers if name == b"set-cookie"]

        cookie = set_cookie_headers[0].decode()
        assert "Secure" not in cookie
