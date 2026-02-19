"""
Security tests for rate limiting functionality.

Tests brute force protection, credential stuffing prevention, and rate limit enforcement.
"""


from fastapi.testclient import TestClient
import pytest


class TestAuthenticationRateLimiting:
    """Test rate limiting on authentication endpoints"""

    def test_login_rate_limit_by_ip(self, client: TestClient):
        """Test that login is rate limited by IP (5 requests/minute)"""
        for i in range(6):
            response = client.post(
                "/api/v1/auth/token", data={"username": f"user{i}", "password": "wrongpassword"}
            )

            if i < 5:
                assert response.status_code in [
                    401,
                    422,
                ], f"Request {i+1} should fail with auth error, not rate limit"
            else:
                assert response.status_code == 429, f"Request {i+1} should be rate limited"
                assert "Rate limit exceeded" in response.json()["detail"]
                assert "Retry-After" in response.headers

    def test_login_rate_limit_by_username(self, client: TestClient):
        """Test that login is rate limited by username (10 requests/hour)"""
        username = "targetuser"

        for i in range(11):
            response = client.post(
                "/api/v1/auth/token",
                data={"username": username, "password": f"wrongpass{i}"},
                headers={"X-Forwarded-For": f"192.168.1.{i}"},
            )

            if i < 10:
                assert response.status_code in [
                    401,
                    422,
                ], f"Request {i+1} should fail with auth error"
            else:
                assert (
                    response.status_code == 429
                ), f"Request {i+1} should be rate limited by username"

    def test_registration_rate_limit(self, client: TestClient):
        """Test that registration is rate limited (3 requests/hour per IP)"""
        for i in range(4):
            response = client.post(
                "/api/v1/auth/register",
                json={
                    "username": f"newuser{i}",
                    "email": f"user{i}@example.com",
                    "password": "SecurePass123!",
                },
            )

            if i < 3:
                assert response.status_code in [
                    201,
                    400,
                ], f"Request {i+1} should succeed or fail validation"
            else:
                assert response.status_code == 429, f"Request {i+1} should be rate limited"


@pytest.fixture()
def test_user(db_session):
    """Create a test user"""
    from src.auth.models import User
    from src.auth.password import hash_password

    user = User(
        username="testuser",
        email="test@example.com",
        hashed_password=hash_password("correctpassword"),
        is_active=True,
    )
    db_session.add(user)
    db_session.commit()
    return user
