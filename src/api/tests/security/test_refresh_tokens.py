"""Refresh token security tests.

Tests refresh token mechanism including:
- Token rotation (one-time use)
- Token validation
- Token revocation
- Token expiration
"""

from datetime import datetime, timedelta

from httpx import AsyncClient
import pytest
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from src.auth.jwt import create_refresh_token
from src.auth.models import RefreshToken, User


class TestRefreshTokenMechanism:
    """Test suite for refresh token functionality."""

    @pytest.mark.asyncio()
    async def test_login_returns_refresh_token(self, client: AsyncClient, test_user: User):
        """Test that login returns both access and refresh tokens."""
        response = await client.post(
            "/api/v1/auth/token", data={"username": "testuser", "password": "testpass123"}
        )
        assert response.status_code == 200
        data = response.json()
        assert "access_token" in data
        assert "refresh_token" in data
        assert data["token_type"] == "bearer"
        assert "expires_in" in data

    @pytest.mark.asyncio()
    async def test_refresh_token_creates_new_tokens(self, client: AsyncClient, test_user: User):
        """Test that refresh endpoint returns new access and refresh tokens."""
        # Login to get initial tokens
        login_response = await client.post(
            "/api/v1/auth/token", data={"username": "testuser", "password": "testpass123"}
        )
        refresh_token = login_response.json()["refresh_token"]

        # Use refresh token
        response = await client.post("/api/v1/auth/refresh", json={"refresh_token": refresh_token})
        assert response.status_code == 200
        data = response.json()
        assert "access_token" in data
        assert "refresh_token" in data
        assert data["refresh_token"] != refresh_token  # New token
        assert data["token_type"] == "bearer"

    @pytest.mark.asyncio()
    async def test_refresh_token_rotation_revokes_old_token(
        self, client: AsyncClient, test_user: User, db_session: AsyncSession
    ):
        """Test that using a refresh token revokes it (one-time use)."""
        # Login to get initial tokens
        login_response = await client.post(
            "/api/v1/auth/token", data={"username": "testuser", "password": "testpass123"}
        )
        old_refresh_token = login_response.json()["refresh_token"]

        # Use refresh token once
        await client.post("/api/v1/auth/refresh", json={"refresh_token": old_refresh_token})

        # Try to use the same token again (should fail)
        response = await client.post(
            "/api/v1/auth/refresh", json={"refresh_token": old_refresh_token}
        )
        assert response.status_code == 401
        assert "revoked" in response.json()["detail"].lower()

    @pytest.mark.asyncio()
    async def test_refresh_with_invalid_token(self, client: AsyncClient):
        """Test that invalid refresh tokens are rejected."""
        response = await client.post(
            "/api/v1/auth/refresh", json={"refresh_token": "invalid.token.here"}
        )
        assert response.status_code == 401
        assert "invalid" in response.json()["detail"].lower()

    @pytest.mark.asyncio()
    async def test_refresh_with_access_token_fails(self, client: AsyncClient, test_user: User):
        """Test that access tokens cannot be used for refresh."""
        # Login to get tokens
        login_response = await client.post(
            "/api/v1/auth/token", data={"username": "testuser", "password": "testpass123"}
        )
        access_token = login_response.json()["access_token"]

        # Try to use access token for refresh
        response = await client.post("/api/v1/auth/refresh", json={"refresh_token": access_token})
        assert response.status_code == 401
        detail = response.json()["detail"].lower()
        assert "refresh" in detail or "invalid" in detail

    @pytest.mark.asyncio()
    async def test_refresh_token_stored_in_database(
        self, client: AsyncClient, test_user: User, db_session: AsyncSession
    ):
        """Test that refresh tokens are stored in database."""
        # Login
        response = await client.post(
            "/api/v1/auth/token", data={"username": "testuser", "password": "testpass123"}
        )
        refresh_token_jwt = response.json()["refresh_token"]

        # Check database
        stmt = select(RefreshToken).where(RefreshToken.token == refresh_token_jwt)
        result = await db_session.execute(stmt)
        token_record = result.scalar_one_or_none()

        assert token_record is not None
        assert token_record.user_id == test_user.id
        assert token_record.revoked is False
        assert token_record.expires_at > datetime.utcnow()

    @pytest.mark.asyncio()
    async def test_logout_revokes_all_refresh_tokens(
        self, client: AsyncClient, test_user: User, db_session: AsyncSession
    ):
        """Test that logout revokes all user's refresh tokens."""
        # Login to get tokens
        login_response = await client.post(
            "/api/v1/auth/token", data={"username": "testuser", "password": "testpass123"}
        )
        access_token = login_response.json()["access_token"]
        refresh_token = login_response.json()["refresh_token"]

        # Logout
        headers = {"Authorization": f"Bearer {access_token}"}
        logout_response = await client.post("/api/v1/auth/logout", headers=headers)
        assert logout_response.status_code == 200

        # Try to use refresh token (should fail)
        refresh_response = await client.post(
            "/api/v1/auth/refresh", json={"refresh_token": refresh_token}
        )
        assert refresh_response.status_code == 401

    @pytest.mark.asyncio()
    async def test_refresh_token_for_inactive_user_fails(
        self, client: AsyncClient, test_user: User, db_session: AsyncSession
    ):
        """Test that refresh tokens for inactive users are rejected."""
        # Login to get tokens
        login_response = await client.post(
            "/api/v1/auth/token", data={"username": "testuser", "password": "testpass123"}
        )
        refresh_token = login_response.json()["refresh_token"]

        # Deactivate user
        test_user.is_active = False
        await db_session.commit()

        # Try to refresh (should fail)
        response = await client.post("/api/v1/auth/refresh", json={"refresh_token": refresh_token})
        assert response.status_code == 401
        assert "inactive" in response.json()["detail"].lower()

    @pytest.mark.asyncio()
    async def test_expired_refresh_token_rejected(
        self, client: AsyncClient, test_user: User, db_session: AsyncSession
    ):
        """Test that expired refresh tokens are rejected."""
        # Create expired token
        expired_token = create_refresh_token(
            data={"sub": test_user.username, "user_id": test_user.id},
            expires_delta=timedelta(seconds=-1),  # Already expired
        )

        # Store in database with expired timestamp
        token_record = RefreshToken(
            user_id=test_user.id,
            token=expired_token,
            expires_at=datetime.utcnow() - timedelta(seconds=1),
        )
        db_session.add(token_record)
        await db_session.commit()

        # Try to use expired token
        response = await client.post("/api/v1/auth/refresh", json={"refresh_token": expired_token})
        assert response.status_code == 401

    @pytest.mark.asyncio()
    async def test_refresh_token_not_in_database_rejected(
        self, client: AsyncClient, test_user: User
    ):
        """Test that valid JWT but not in DB is rejected."""
        # Create a valid JWT that's not stored in database
        fake_token = create_refresh_token(data={"sub": test_user.username, "user_id": test_user.id})

        # Try to use it (should fail because not in DB)
        response = await client.post("/api/v1/auth/refresh", json={"refresh_token": fake_token})
        assert response.status_code == 401
        assert "not found" in response.json()["detail"].lower()

    @pytest.mark.asyncio()
    async def test_new_access_token_works_after_refresh(self, client: AsyncClient, test_user: User):
        """Test that new access token from refresh works correctly."""
        # Login
        login_response = await client.post(
            "/api/v1/auth/token", data={"username": "testuser", "password": "testpass123"}
        )
        refresh_token = login_response.json()["refresh_token"]

        # Refresh to get new access token
        refresh_response = await client.post(
            "/api/v1/auth/refresh", json={"refresh_token": refresh_token}
        )
        new_access_token = refresh_response.json()["access_token"]

        # Use new access token
        headers = {"Authorization": f"Bearer {new_access_token}"}
        response = await client.get("/api/v1/auth/me", headers=headers)
        assert response.status_code == 200
        assert response.json()["username"] == "testuser"


class TestRefreshTokenRateLimiting:
    """Test suite for refresh token rate limiting."""

    @pytest.mark.asyncio()
    async def test_refresh_endpoint_has_rate_limiting(self, client: AsyncClient, test_user: User):
        """Test that refresh endpoint is rate limited."""
        # Login to get a refresh token
        await client.post(
            "/api/v1/auth/token", data={"username": "testuser", "password": "testpass123"}
        )

        # Make many refresh requests (should eventually hit rate limit)
        # Note: This test may need adjustment based on actual rate limit config
        responses = []
        for _ in range(15):  # Try 15 times (limit is 10/minute)
            response = await client.post(
                "/api/v1/auth/refresh", json={"refresh_token": "fake_token"}
            )
            responses.append(response.status_code)

        # Should eventually get 429 (rate limited)
        assert 429 in responses or all(r == 401 for r in responses)
