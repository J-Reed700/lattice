"""
Comprehensive CSRF Protection Tests
"""

from fastapi.testclient import TestClient
import pytest

from src.api.app import create_app
from src.middleware.csrf import (
    CSRF_COOKIE_NAME,
    CSRF_HEADER_NAME,
    generate_csrf_token,
    verify_csrf_token,
)


@pytest.fixture()
def client():
    """Create test client with CSRF protection enabled."""
    app = create_app()
    return TestClient(app)


class TestCSRFTokenGeneration:
    """Test CSRF token generation and verification."""

    def test_generate_csrf_token_creates_unique_tokens(self):
        """Test that generated tokens are unique."""
        token1 = generate_csrf_token()
        token2 = generate_csrf_token()

        assert token1 != token2
        assert len(token1) == 64
        assert len(token2) == 64

    def test_verify_csrf_token_accepts_matching_tokens(self):
        """Test that matching tokens are accepted."""
        token = generate_csrf_token()
        assert verify_csrf_token(token, token) is True

    def test_verify_csrf_token_rejects_mismatched_tokens(self):
        """Test that mismatched tokens are rejected."""
        token1 = generate_csrf_token()
        token2 = generate_csrf_token()
        assert verify_csrf_token(token1, token2) is False


class TestCSRFEndpoint:
    """Test CSRF token endpoint."""

    def test_get_csrf_token_returns_token(self, client):
        """Test that CSRF endpoint returns a token."""
        response = client.get("/api/v1/csrf/token")
        assert response.status_code == 200
        data = response.json()
        assert "csrf_token" in data
        assert len(data["csrf_token"]) == 64

    def test_get_csrf_token_sets_cookie(self, client):
        """Test that CSRF endpoint sets cookie."""
        response = client.get("/api/v1/csrf/token")
        assert response.status_code == 200
        assert CSRF_COOKIE_NAME in response.cookies


class TestCSRFProtectionMiddleware:
    """Test CSRF protection on endpoints."""

    def test_safe_methods_dont_require_csrf_token(self, client):
        """Test that GET does not require CSRF token."""
        response = client.get("/api/v1/health")
        assert response.status_code in [200, 404]

    def test_post_without_csrf_token_is_rejected(self, client):
        """Test that POST without CSRF token is rejected."""
        response = client.post("/api/v1/files/")
        assert response.status_code == 403
        assert "CSRF" in response.json()["detail"]

    def test_post_with_valid_csrf_token_is_accepted(self, client):
        """Test that POST with valid CSRF token is accepted."""
        token_response = client.get("/api/v1/csrf/token")
        csrf_token = token_response.json()["csrf_token"]

        response = client.post(
            "/api/v1/files/",
            headers={CSRF_HEADER_NAME: csrf_token},
            cookies={CSRF_COOKIE_NAME: csrf_token},
        )

        assert response.status_code != 403 or "CSRF" not in response.json()["detail"]
