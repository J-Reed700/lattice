"""Authentication security tests.

Tests authentication mechanisms including:
- Login with valid/invalid credentials
- Token validation (valid, expired, malformed, tampered)
- Access without authentication
- Token expiration
- Inactive user handling
"""

from httpx import AsyncClient
import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from src.auth.models import User


class TestLoginSecurity:
    """Test suite for login security."""

    @pytest.mark.asyncio()
    async def test_login_with_valid_credentials(self, client: AsyncClient, test_user: User):
        """Test successful login with valid credentials."""
        response = await client.post(
            "/api/v1/auth/token", data={"username": "testuser", "password": "testpass123"}
        )
        assert response.status_code == 200
        data = response.json()
        assert "access_token" in data
        assert data["token_type"] == "bearer"
        assert "expires_in" in data

    @pytest.mark.asyncio()
    async def test_login_with_invalid_password(self, client: AsyncClient, test_user: User):
        """Test login fails with wrong password."""
        response = await client.post(
            "/api/v1/auth/token", data={"username": "testuser", "password": "wrongpassword"}
        )
        assert response.status_code == 401
        assert "Incorrect username or password" in response.json()["detail"]

    @pytest.mark.asyncio()
    async def test_login_with_nonexistent_user(self, client: AsyncClient):
        """Test login fails for non-existent user."""
        response = await client.post(
            "/api/v1/auth/token", data={"username": "nonexistent", "password": "password"}
        )
        assert response.status_code == 401
        assert "Incorrect username or password" in response.json()["detail"]

    @pytest.mark.asyncio()
    async def test_username_enumeration_prevention(self, client: AsyncClient, test_user: User):
        """Test that error messages don't reveal if username exists."""
        response_nonexistent = await client.post(
            "/api/v1/auth/token", data={"username": "nonexistent", "password": "password"}
        )

        response_wrong_password = await client.post(
            "/api/v1/auth/token", data={"username": "testuser", "password": "wrongpassword"}
        )

        assert response_nonexistent.status_code == response_wrong_password.status_code == 401
        assert response_nonexistent.json()["detail"] == response_wrong_password.json()["detail"]

    @pytest.mark.asyncio()
    async def test_login_with_inactive_user(self, client: AsyncClient, inactive_user: User):
        """Test that inactive users cannot login."""
        response = await client.post(
            "/api/v1/auth/token", data={"username": "inactiveuser", "password": "testpass123"}
        )
        assert response.status_code == 403
        assert "Inactive user account" in response.json()["detail"]

    @pytest.mark.asyncio()
    async def test_login_with_empty_credentials(self, client: AsyncClient):
        """Test login fails with empty credentials."""
        response = await client.post("/api/v1/auth/token", data={"username": "", "password": ""})
        assert response.status_code == 422

    @pytest.mark.asyncio()
    async def test_login_with_sql_injection_attempts(
        self, client: AsyncClient, sql_injection_payloads: list[str]
    ):
        """Test that SQL injection attempts in login are safely handled."""
        for payload in sql_injection_payloads:
            response = await client.post(
                "/api/v1/auth/token", data={"username": payload, "password": "password"}
            )
            assert response.status_code in [401, 422]


class TestTokenValidation:
    """Test suite for JWT token validation."""

    @pytest.mark.asyncio()
    async def test_access_protected_endpoint_without_token(self, client: AsyncClient):
        """Test that protected endpoints reject requests without auth."""
        response = await client.get("/api/v1/auth/me")
        assert response.status_code == 401

    @pytest.mark.asyncio()
    async def test_access_protected_endpoint_with_valid_token(
        self, client: AsyncClient, auth_headers: dict
    ):
        """Test that protected endpoints accept valid tokens."""
        response = await client.get("/api/v1/auth/me", headers=auth_headers)
        assert response.status_code == 200
        data = response.json()
        assert data["username"] == "testuser"

    @pytest.mark.asyncio()
    async def test_access_with_expired_token(self, client: AsyncClient, expired_token: str):
        """Test that expired tokens are rejected."""
        headers = {"Authorization": f"Bearer {expired_token}"}
        response = await client.get("/api/v1/auth/me", headers=headers)
        assert response.status_code == 401

    @pytest.mark.asyncio()
    async def test_access_with_malformed_token(self, client: AsyncClient, malformed_token: str):
        """Test that malformed tokens are rejected."""
        headers = {"Authorization": f"Bearer {malformed_token}"}
        response = await client.get("/api/v1/auth/me", headers=headers)
        assert response.status_code == 401

    @pytest.mark.asyncio()
    async def test_access_with_tampered_token(self, client: AsyncClient, tampered_token: str):
        """Test that tampered tokens are rejected."""
        headers = {"Authorization": f"Bearer {tampered_token}"}
        response = await client.get("/api/v1/auth/me", headers=headers)
        assert response.status_code == 401

    @pytest.mark.asyncio()
    async def test_access_with_missing_bearer_prefix(self, client: AsyncClient, auth_token: str):
        """Test that tokens without 'Bearer' prefix are rejected."""
        headers = {"Authorization": auth_token}
        response = await client.get("/api/v1/auth/me", headers=headers)
        assert response.status_code == 401

    @pytest.mark.asyncio()
    async def test_access_with_token_for_nonexistent_user(
        self, client: AsyncClient, token_for_nonexistent_user: str
    ):
        """Test that tokens for non-existent users are rejected."""
        headers = {"Authorization": f"Bearer {token_for_nonexistent_user}"}
        response = await client.get("/api/v1/auth/me", headers=headers)
        assert response.status_code == 401

    @pytest.mark.asyncio()
    async def test_access_with_empty_token(self, client: AsyncClient):
        """Test that empty tokens are rejected."""
        headers = {"Authorization": "Bearer "}
        response = await client.get("/api/v1/auth/me", headers=headers)
        assert response.status_code == 401

    @pytest.mark.asyncio()
    async def test_access_with_wrong_token_type(self, client: AsyncClient, auth_token: str):
        """Test that wrong token types are rejected."""
        headers = {"Authorization": f"Basic {auth_token}"}
        response = await client.get("/api/v1/auth/me", headers=headers)
        assert response.status_code == 401


class TestRegistrationSecurity:
    """Test suite for registration security."""

    @pytest.mark.asyncio()
    async def test_register_with_valid_data(self, client: AsyncClient, db_session: AsyncSession):
        """Test successful user registration."""
        response = await client.post(
            "/api/v1/auth/register",
            json={
                "username": "newuser",
                "email": "newuser@example.com",
                "password": "SecurePass123!",
            },
        )
        assert response.status_code == 201
        data = response.json()
        assert data["username"] == "newuser"
        assert data["email"] == "newuser@example.com"
        assert "password" not in data
        assert "hashed_password" not in data

    @pytest.mark.asyncio()
    async def test_register_duplicate_username(self, client: AsyncClient, test_user: User):
        """Test that duplicate usernames are rejected."""
        response = await client.post(
            "/api/v1/auth/register",
            json={
                "username": "testuser",
                "email": "different@example.com",
                "password": "SecurePass123!",
            },
        )
        assert response.status_code == 400
        assert "Username already registered" in response.json()["detail"]

    @pytest.mark.asyncio()
    async def test_register_duplicate_email(self, client: AsyncClient, test_user: User):
        """Test that duplicate emails are rejected."""
        response = await client.post(
            "/api/v1/auth/register",
            json={
                "username": "differentuser",
                "email": "test@example.com",
                "password": "SecurePass123!",
            },
        )
        assert response.status_code == 400
        assert "Email already registered" in response.json()["detail"]

    @pytest.mark.asyncio()
    async def test_register_with_sql_injection(
        self, client: AsyncClient, sql_injection_payloads: list[str]
    ):
        """Test that SQL injection in registration is prevented."""
        for payload in sql_injection_payloads[:3]:
            response = await client.post(
                "/api/v1/auth/register",
                json={
                    "username": payload,
                    "email": f"test_{hash(payload)}@example.com",
                    "password": "SecurePass123!",
                },
            )
            assert response.status_code in [400, 422]

    @pytest.mark.asyncio()
    async def test_register_with_xss_in_username(
        self, client: AsyncClient, xss_payloads: list[str]
    ):
        """Test that XSS payloads in username are handled safely."""
        for payload in xss_payloads[:3]:
            response = await client.post(
                "/api/v1/auth/register",
                json={
                    "username": payload,
                    "email": f"test_{hash(payload)}@example.com",
                    "password": "SecurePass123!",
                },
            )
            assert response.status_code in [400, 422]


class TestLogoutSecurity:
    """Test suite for logout security."""

    @pytest.mark.asyncio()
    async def test_logout_with_valid_token(self, client: AsyncClient, auth_headers: dict):
        """Test logout with valid authentication."""
        response = await client.post("/api/v1/auth/logout", headers=auth_headers)
        assert response.status_code == 200
        assert "logged out" in response.json()["message"].lower()

    @pytest.mark.asyncio()
    async def test_logout_without_token(self, client: AsyncClient):
        """Test that logout requires authentication."""
        response = await client.post("/api/v1/auth/logout")
        assert response.status_code == 401


class TestPasswordValidation:
    """Test suite for enhanced password validation."""

    @pytest.mark.asyncio()
    async def test_password_requires_special_character(self, client: AsyncClient):
        """Test that passwords must contain special characters."""
        response = await client.post(
            "/api/v1/auth/register",
            json={
                "username": "newuser1",
                "email": "newuser1@example.com",
                "password": "NoSpecialChar123",
            },
        )
        assert response.status_code == 422
        assert "special character" in str(response.json()["detail"]).lower()

    @pytest.mark.asyncio()
    async def test_password_accepts_valid_special_characters(self, client: AsyncClient):
        """Test that various special characters are accepted."""
        special_chars = [
            "!",
            "@",
            "#",
            "$",
            "%",
            "^",
            "&",
            "*",
            "(",
            ")",
            ".",
            ",",
            "?",
            ":",
            "{",
            "}",
            "|",
            "<",
            ">",
        ]

        for i, char in enumerate(special_chars[:5]):  # Test first 5
            password = f"Pass123{char}"
            response = await client.post(
                "/api/v1/auth/register",
                json={
                    "username": f"user_special_{i}",
                    "email": f"user_special_{i}@example.com",
                    "password": password,
                },
            )
            assert response.status_code == 201, f"Failed for special char: {char}"

    @pytest.mark.asyncio()
    async def test_password_rejects_common_passwords(self, client: AsyncClient):
        """Test that common passwords are rejected."""
        common_passwords = [
            "Password1!",  # Contains "password"
            "123456!aA",  # Contains "123456"
            "Qwerty123!",  # Contains "qwerty"
            "Letmein1!",  # Contains "letmein"
        ]

        for i, password in enumerate(common_passwords):
            response = await client.post(
                "/api/v1/auth/register",
                json={
                    "username": f"user_common_{i}",
                    "email": f"user_common_{i}@example.com",
                    "password": password,
                },
            )
            assert response.status_code == 422
            detail = str(response.json()["detail"]).lower()
            assert "common" in detail or "secure" in detail

    @pytest.mark.asyncio()
    async def test_password_still_requires_uppercase(self, client: AsyncClient):
        """Test that uppercase requirement is still enforced."""
        response = await client.post(
            "/api/v1/auth/register",
            json={
                "username": "user_no_upper",
                "email": "user_no_upper@example.com",
                "password": "nouppercase123!",
            },
        )
        assert response.status_code == 422
        assert "uppercase" in str(response.json()["detail"]).lower()

    @pytest.mark.asyncio()
    async def test_password_still_requires_lowercase(self, client: AsyncClient):
        """Test that lowercase requirement is still enforced."""
        response = await client.post(
            "/api/v1/auth/register",
            json={
                "username": "user_no_lower",
                "email": "user_no_lower@example.com",
                "password": "NOLOWERCASE123!",
            },
        )
        assert response.status_code == 422
        assert "lowercase" in str(response.json()["detail"]).lower()

    @pytest.mark.asyncio()
    async def test_password_still_requires_digit(self, client: AsyncClient):
        """Test that digit requirement is still enforced."""
        response = await client.post(
            "/api/v1/auth/register",
            json={
                "username": "user_no_digit",
                "email": "user_no_digit@example.com",
                "password": "NoDigitsHere!",
            },
        )
        assert response.status_code == 422
        assert "digit" in str(response.json()["detail"]).lower()

    @pytest.mark.asyncio()
    async def test_password_still_requires_min_length(self, client: AsyncClient):
        """Test that minimum length requirement is still enforced."""
        response = await client.post(
            "/api/v1/auth/register",
            json={
                "username": "user_short",
                "email": "user_short@example.com",
                "password": "Sh0rt!",
            },
        )
        assert response.status_code == 422
        assert "8 characters" in str(response.json()["detail"]).lower()

    @pytest.mark.asyncio()
    async def test_password_all_requirements_met(self, client: AsyncClient):
        """Test that a password meeting all requirements is accepted."""
        response = await client.post(
            "/api/v1/auth/register",
            json={
                "username": "user_valid_pass",
                "email": "user_valid_pass@example.com",
                "password": "ValidPassword123!",
            },
        )
        assert response.status_code == 201
        data = response.json()
        assert data["username"] == "user_valid_pass"
        assert "password" not in data
