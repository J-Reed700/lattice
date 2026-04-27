"""Tests for token revocation system."""

from __future__ import annotations

from fastapi import status
from httpx import AsyncClient
from jose import jwt as jwt_lib
import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from src.auth.jwt import create_access_token, verify_access_token
from src.auth.models import User
from src.auth.token_blacklist import token_blacklist
from src.config.settings import get_settings

settings = get_settings()


@pytest.mark.asyncio()
class TestTokenBlacklist:
    """Tests for token blacklist functionality."""

    async def test_add_token_to_blacklist(self):
        """Test adding a token to the blacklist."""
        token_id = "test-token-123"
        await token_blacklist.add_token(token_id)
        assert await token_blacklist.is_token_blacklisted(token_id) is True

    async def test_remove_token_from_blacklist(self):
        """Test removing a token from the blacklist."""
        token_id = "test-token-456"
        await token_blacklist.add_token(token_id)
        assert await token_blacklist.is_token_blacklisted(token_id) is True

        await token_blacklist.remove_token(token_id)
        assert await token_blacklist.is_token_blacklisted(token_id) is False

    async def test_revoke_all_user_tokens(self):
        """Test revoking all tokens for a user."""
        user_id = "user-123"
        await token_blacklist.revoke_all_user_tokens(user_id)
        assert await token_blacklist.is_user_blacklisted(user_id) is True

    async def test_clear_user_blacklist(self):
        """Test clearing user from blacklist."""
        user_id = "user-456"
        await token_blacklist.revoke_all_user_tokens(user_id)
        assert await token_blacklist.is_user_blacklisted(user_id) is True

        await token_blacklist.clear_user_blacklist(user_id)
        assert await token_blacklist.is_user_blacklisted(user_id) is False

    async def test_get_blacklist_stats(self):
        """Test getting blacklist statistics."""
        token_id = "stats-token"
        user_id = "stats-user"

        await token_blacklist.add_token(token_id)
        await token_blacklist.revoke_all_user_tokens(user_id)

        stats = await token_blacklist.get_stats()
        assert "blacklisted_tokens" in stats
        assert "blacklisted_users" in stats
        assert stats["blacklisted_tokens"] >= 1
        assert stats["blacklisted_users"] >= 1


@pytest.mark.asyncio()
class TestTokenRevocationAPI:
    """Tests for token revocation API endpoints."""

    async def test_logout_revokes_current_token(
        self, client: AsyncClient, test_user: User, auth_token: str, auth_headers: dict
    ):
        """Test logout revokes the current access token."""
        response = await client.get("/api/v1/auth/me", headers=auth_headers)
        assert response.status_code == status.HTTP_200_OK

        response = await client.post("/api/v1/auth/logout", headers=auth_headers)
        assert response.status_code == status.HTTP_200_OK
        assert "logged out" in response.json()["message"].lower()

        response = await client.get("/api/v1/auth/me", headers=auth_headers)
        assert response.status_code == status.HTTP_401_UNAUTHORIZED
        assert "revoked" in response.json()["detail"].lower()

    async def test_logout_without_auth(self, client: AsyncClient):
        """Test logout requires authentication."""
        response = await client.post("/api/v1/auth/logout")
        assert response.status_code == status.HTTP_401_UNAUTHORIZED

    async def test_revoke_specific_token(
        self, client: AsyncClient, test_user: User, auth_headers: dict
    ):
        """Test revoking a specific token."""
        token_to_revoke = create_access_token(
            data={"sub": test_user.username, "user_id": test_user.id}
        )

        response = await client.post(
            "/api/v1/auth/revoke", headers=auth_headers, json={"token": token_to_revoke}
        )
        assert response.status_code == status.HTTP_200_OK
        assert "revoked" in response.json()["message"].lower()

        headers = {"Authorization": f"Bearer {token_to_revoke}"}
        response = await client.get("/api/v1/auth/me", headers=headers)
        assert response.status_code == status.HTTP_401_UNAUTHORIZED

    async def test_revoke_requires_auth(self, client: AsyncClient, test_user: User):
        """Test revoke endpoint requires authentication."""
        token_to_revoke = create_access_token(
            data={"sub": test_user.username, "user_id": test_user.id}
        )

        response = await client.post("/api/v1/auth/revoke", json={"token": token_to_revoke})
        assert response.status_code == status.HTTP_401_UNAUTHORIZED

    async def test_revoke_invalid_token(self, client: AsyncClient, auth_headers: dict):
        """Test revoking an invalid token returns error."""
        response = await client.post(
            "/api/v1/auth/revoke", headers=auth_headers, json={"token": "invalid.token.here"}
        )
        assert response.status_code == status.HTTP_400_BAD_REQUEST

    async def test_revoke_all_tokens(
        self, client: AsyncClient, test_user: User, auth_headers: dict
    ):
        """Test revoking all tokens for current user."""
        token1 = create_access_token(data={"sub": test_user.username, "user_id": test_user.id})
        token2 = create_access_token(data={"sub": test_user.username, "user_id": test_user.id})

        response = await client.post("/api/v1/auth/revoke-all", headers=auth_headers)
        assert response.status_code == status.HTTP_200_OK
        assert "all tokens revoked" in response.json()["message"].lower()

        headers1 = {"Authorization": f"Bearer {token1}"}
        response1 = await client.get("/api/v1/auth/me", headers=headers1)
        assert response1.status_code == status.HTTP_401_UNAUTHORIZED

        headers2 = {"Authorization": f"Bearer {token2}"}
        response2 = await client.get("/api/v1/auth/me", headers=headers2)
        assert response2.status_code == status.HTTP_401_UNAUTHORIZED

    async def test_revoke_all_requires_auth(self, client: AsyncClient):
        """Test revoke-all requires authentication."""
        response = await client.post("/api/v1/auth/revoke-all")
        assert response.status_code == status.HTTP_401_UNAUTHORIZED

    async def test_change_password_revokes_tokens(
        self, client: AsyncClient, test_user: User, auth_token: str, auth_headers: dict
    ):
        """Test changing password revokes all existing tokens."""
        response = await client.get("/api/v1/auth/me", headers=auth_headers)
        assert response.status_code == status.HTTP_200_OK

        response = await client.post(
            "/api/v1/auth/change-password",
            headers=auth_headers,
            json={"old_password": "testpass123", "new_password": "newpass456"},
        )
        assert response.status_code == status.HTTP_200_OK
        assert "password changed" in response.json()["message"].lower()

        response = await client.get("/api/v1/auth/me", headers=auth_headers)
        assert response.status_code == status.HTTP_401_UNAUTHORIZED

    async def test_change_password_incorrect_old_password(
        self, client: AsyncClient, auth_headers: dict
    ):
        """Test change password fails with incorrect old password."""
        response = await client.post(
            "/api/v1/auth/change-password",
            headers=auth_headers,
            json={"old_password": "wrongpass", "new_password": "newpass456"},
        )
        assert response.status_code == status.HTTP_400_BAD_REQUEST
        assert "incorrect" in response.json()["detail"].lower()

    async def test_change_password_requires_auth(self, client: AsyncClient):
        """Test change password requires authentication."""
        response = await client.post(
            "/api/v1/auth/change-password",
            json={"old_password": "testpass123", "new_password": "newpass456"},
        )
        assert response.status_code == status.HTTP_401_UNAUTHORIZED

    async def test_token_stats(self, client: AsyncClient, auth_headers: dict):
        """Test getting token blacklist statistics."""
        response = await client.get("/api/v1/auth/token-stats", headers=auth_headers)
        assert response.status_code == status.HTTP_200_OK
        assert "blacklist_stats" in response.json()
        assert "blacklisted_tokens" in response.json()["blacklist_stats"]
        assert "blacklisted_users" in response.json()["blacklist_stats"]

    async def test_token_stats_requires_auth(self, client: AsyncClient):
        """Test token stats requires authentication."""
        response = await client.get("/api/v1/auth/token-stats")
        assert response.status_code == status.HTTP_401_UNAUTHORIZED


@pytest.mark.asyncio()
class TestTokenVerification:
    """Tests for token verification with blacklist checking."""

    async def test_verify_valid_token(self, test_user: User):
        """Test verifying a valid non-blacklisted token."""
        token = create_access_token(data={"sub": test_user.username, "user_id": test_user.id})
        payload = await verify_access_token(token)
        assert payload["sub"] == test_user.username

    async def test_verify_blacklisted_token(self, test_user: User):
        """Test verifying a blacklisted token raises exception."""
        token = create_access_token(data={"sub": test_user.username, "user_id": test_user.id})

        decoded = jwt_lib.decode(token, options={"verify_signature": False})
        token_id = decoded.get("jti")

        await token_blacklist.add_token(token_id)

        with pytest.raises(Exception) as exc:
            await verify_access_token(token)
        assert "revoked" in str(exc.value).lower()

    async def test_verify_token_user_blacklisted(self, test_user: User):
        """Test verifying a token when user is blacklisted."""
        token = create_access_token(data={"sub": test_user.username, "user_id": test_user.id})

        await token_blacklist.revoke_all_user_tokens(str(test_user.id))

        with pytest.raises(Exception) as exc:
            await verify_access_token(token)
        assert "revoked" in str(exc.value).lower()


@pytest.mark.asyncio()
class TestTokenRevocationIntegration:
    """Integration tests for token revocation."""

    async def test_multiple_device_logout(
        self, client: AsyncClient, test_user: User, auth_headers: dict
    ):
        """Test logout from one device doesn't affect other sessions until revoke-all."""
        token1 = create_access_token(data={"sub": test_user.username, "user_id": test_user.id})
        token2 = create_access_token(data={"sub": test_user.username, "user_id": test_user.id})

        headers1 = {"Authorization": f"Bearer {token1}"}
        headers2 = {"Authorization": f"Bearer {token2}"}

        response = await client.get("/api/v1/auth/me", headers=headers1)
        assert response.status_code == status.HTTP_200_OK
        response = await client.get("/api/v1/auth/me", headers=headers2)
        assert response.status_code == status.HTTP_200_OK

        await client.post("/api/v1/auth/logout", headers=headers1)

        response = await client.get("/api/v1/auth/me", headers=headers1)
        assert response.status_code == status.HTTP_401_UNAUTHORIZED

        response = await client.get("/api/v1/auth/me", headers=headers2)
        assert response.status_code == status.HTTP_200_OK

    async def test_revoke_all_affects_all_devices(
        self, client: AsyncClient, test_user: User, auth_headers: dict
    ):
        """Test revoke-all invalidates tokens from all devices."""
        token1 = create_access_token(data={"sub": test_user.username, "user_id": test_user.id})
        token2 = create_access_token(data={"sub": test_user.username, "user_id": test_user.id})

        headers1 = {"Authorization": f"Bearer {token1}"}
        headers2 = {"Authorization": f"Bearer {token2}"}

        await client.post("/api/v1/auth/revoke-all", headers=auth_headers)

        response = await client.get("/api/v1/auth/me", headers=headers1)
        assert response.status_code == status.HTTP_401_UNAUTHORIZED
        response = await client.get("/api/v1/auth/me", headers=headers2)
        assert response.status_code == status.HTTP_401_UNAUTHORIZED

    async def test_new_login_after_revoke_all(
        self, client: AsyncClient, test_user: User, auth_headers: dict, db_session: AsyncSession
    ):
        """Test new login works after revoke-all."""
        await client.post("/api/v1/auth/revoke-all", headers=auth_headers)

        response = await client.post(
            "/api/v1/auth/token",
            data={"username": test_user.username, "password": "testpass123"},
        )
        assert response.status_code == status.HTTP_200_OK

        new_token = response.json()["access_token"]
        new_headers = {"Authorization": f"Bearer {new_token}"}

        response = await client.get("/api/v1/auth/me", headers=new_headers)
        assert response.status_code == status.HTTP_200_OK

    async def test_security_breach_scenario(
        self, client: AsyncClient, test_user: User, auth_headers: dict
    ):
        """Test security breach scenario: change password revokes all tokens."""
        token1 = create_access_token(data={"sub": test_user.username, "user_id": test_user.id})
        token2 = create_access_token(data={"sub": test_user.username, "user_id": test_user.id})

        await client.post(
            "/api/v1/auth/change-password",
            headers=auth_headers,
            json={"old_password": "testpass123", "new_password": "securepass789"},
        )

        headers1 = {"Authorization": f"Bearer {token1}"}
        headers2 = {"Authorization": f"Bearer {token2}"}

        response1 = await client.get("/api/v1/auth/me", headers=headers1)
        assert response1.status_code == status.HTTP_401_UNAUTHORIZED
        response2 = await client.get("/api/v1/auth/me", headers=headers2)
        assert response2.status_code == status.HTTP_401_UNAUTHORIZED
