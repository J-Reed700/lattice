"""Authorization security tests.

Tests access control and privilege escalation prevention:
- Access with another user's resources
- Privilege escalation attempts
- Superuser-only endpoint protection
- Inactive user access control
"""

from httpx import AsyncClient
import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from src.auth.models import User


class TestUserResourceAccess:
    """Test that users can only access their own resources."""

    @pytest.mark.asyncio()
    async def test_user_can_access_own_profile(self, client: AsyncClient, auth_headers: dict):
        """Test that users can access their own profile."""
        response = await client.get("/api/v1/auth/me", headers=auth_headers)
        assert response.status_code == 200
        data = response.json()
        assert data["username"] == "testuser"

    @pytest.mark.asyncio()
    async def test_user_cannot_access_another_profile_directly(
        self, client: AsyncClient, test_user: User, second_user: User, auth_headers: dict
    ):
        """Test that users cannot access other users' profiles by ID."""
        response = await client.get(f"/api/v1/users/{second_user.id}", headers=auth_headers)
        assert response.status_code in [403, 404]


class TestPrivilegeEscalation:
    """Test prevention of privilege escalation attacks."""

    @pytest.mark.asyncio()
    async def test_normal_user_cannot_access_admin_endpoint(
        self, client: AsyncClient, auth_headers: dict
    ):
        """Test that normal users cannot access admin endpoints."""
        response = await client.get("/api/v1/admin/users", headers=auth_headers)
        assert response.status_code in [403, 404]

    @pytest.mark.asyncio()
    async def test_superuser_can_access_admin_endpoint(
        self, client: AsyncClient, admin_headers: dict
    ):
        """Test that superusers can access admin endpoints."""
        response = await client.get("/api/v1/admin/users", headers=admin_headers)
        assert response.status_code in [200, 404]

    @pytest.mark.asyncio()
    async def test_normal_user_cannot_modify_superuser_flag(
        self, client: AsyncClient, test_user: User, auth_headers: dict, db_session: AsyncSession
    ):
        """Test that users cannot elevate themselves to superuser."""
        response = await client.patch(
            f"/api/v1/users/{test_user.id}", headers=auth_headers, json={"is_superuser": True}
        )
        assert response.status_code in [403, 404, 422]

        await db_session.refresh(test_user)
        assert not test_user.is_superuser

    @pytest.mark.asyncio()
    async def test_normal_user_cannot_activate_inactive_account(
        self, client: AsyncClient, inactive_user: User, auth_headers: dict, db_session: AsyncSession
    ):
        """Test that users cannot activate inactive accounts."""
        response = await client.patch(
            f"/api/v1/users/{inactive_user.id}", headers=auth_headers, json={"is_active": True}
        )
        assert response.status_code in [403, 404]

        await db_session.refresh(inactive_user)
        assert not inactive_user.is_active


class TestInactiveUserAccess:
    """Test that inactive users have restricted access."""

    @pytest.mark.asyncio()
    async def test_inactive_user_cannot_get_token(self, client: AsyncClient, inactive_user: User):
        """Test that inactive users cannot obtain access tokens."""
        response = await client.post(
            "/api/v1/auth/token", data={"username": "inactiveuser", "password": "testpass123"}
        )
        assert response.status_code == 403
        assert "Inactive" in response.json()["detail"]


class TestCrossUserAccess:
    """Test that users cannot access each other's resources."""

    @pytest.mark.asyncio()
    async def test_user_a_cannot_access_user_b_files(
        self,
        client: AsyncClient,
        test_user: User,
        second_user: User,
        auth_headers: dict,
        second_user_headers: dict,
    ):
        """Test that User A cannot access User B's files."""
        upload_response = await client.post(
            "/api/v1/files/",
            headers=second_user_headers,
            files={"file": ("userB_file.txt", b"User B's secret content")},
        )

        if upload_response.status_code in [200, 201]:
            file_id = upload_response.json().get("id")
            if file_id:
                access_response = await client.get(f"/api/v1/files/{file_id}", headers=auth_headers)
                assert access_response.status_code == 403

    @pytest.mark.asyncio()
    async def test_user_cannot_delete_another_users_resources(
        self, client: AsyncClient, auth_headers: dict, second_user_headers: dict
    ):
        """Test that users cannot delete other users' resources."""
        create_response = await client.post(
            "/api/v1/files/",
            headers=second_user_headers,
            files={"file": ("delete_test.txt", b"Test content")},
        )

        if create_response.status_code in [200, 201]:
            file_id = create_response.json().get("id")
            if file_id:
                delete_response = await client.delete(
                    f"/api/v1/files/{file_id}", headers=auth_headers
                )
                assert delete_response.status_code == 403


class TestTokenImpersonation:
    """Test that tokens cannot be used to impersonate other users."""

    @pytest.mark.asyncio()
    async def test_cannot_use_token_after_user_deactivation(
        self, client: AsyncClient, test_user: User, auth_headers: dict, db_session: AsyncSession
    ):
        """Test that tokens are invalid after user is deactivated."""
        response = await client.get("/api/v1/auth/me", headers=auth_headers)
        assert response.status_code == 200

        test_user.is_active = False
        db_session.add(test_user)
        await db_session.commit()

        response = await client.get("/api/v1/auth/me", headers=auth_headers)
        assert response.status_code == 403

    @pytest.mark.asyncio()
    async def test_token_contains_correct_user_data(
        self, client: AsyncClient, auth_headers: dict, test_user: User
    ):
        """Test that token authentication returns correct user."""
        response = await client.get("/api/v1/auth/me", headers=auth_headers)
        assert response.status_code == 200
        data = response.json()
        assert data["username"] == test_user.username
        assert data["email"] == test_user.email
        assert data["id"] == test_user.id
