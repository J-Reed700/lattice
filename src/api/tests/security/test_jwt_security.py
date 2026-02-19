"""
Security tests for JWT token vulnerabilities.

Tests for:
1. Separate secrets for access and refresh tokens (CVE-2023-XXXXX)
2. Algorithm verification to prevent algorithm confusion attacks
3. Required claims validation to prevent token manipulation
4. Token type validation to prevent token substitution
"""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
import uuid

from fastapi import HTTPException
from jose import jwt
import pytest

from src.auth.jwt import (
    create_access_token,
    create_refresh_token,
    verify_access_token,
    verify_refresh_token,
)
from src.config.settings import get_settings


@pytest.mark.unit()
@pytest.mark.asyncio()
class TestJWTSecretSeparation:
    """Test that access and refresh tokens use different secrets."""

    def test_separate_secrets_configured(self) -> None:
        """Ensure access and refresh tokens use different secrets."""
        settings = get_settings()
        assert settings.jwt_secret_key != settings.jwt_refresh_secret_key, (
            "JWT_SECRET_KEY and JWT_REFRESH_SECRET_KEY must be different. "
            "Using same secret for both tokens is a critical security vulnerability (CVSS 7.5)."
        )

    def test_access_token_uses_access_secret(self) -> None:
        """Verify access token is signed with access secret."""
        settings = get_settings()
        token = create_access_token({"sub": "testuser", "user_id": 123})

        # Should decode with access secret
        payload = jwt.decode(token, settings.jwt_secret_key, algorithms=["HS256"])
        assert payload["sub"] == "testuser"

        # Should fail with refresh secret
        with pytest.raises(Exception):
            jwt.decode(token, settings.jwt_refresh_secret_key, algorithms=["HS256"])

    def test_refresh_token_uses_refresh_secret(self) -> None:
        """Verify refresh token is signed with refresh secret."""
        settings = get_settings()
        token = create_refresh_token({"sub": "testuser", "user_id": 123})

        # Should decode with refresh secret
        payload = jwt.decode(token, settings.jwt_refresh_secret_key, algorithms=["HS256"])
        assert payload["sub"] == "testuser"

        # Should fail with access secret
        with pytest.raises(Exception):
            jwt.decode(token, settings.jwt_secret_key, algorithms=["HS256"])


@pytest.mark.unit()
@pytest.mark.asyncio()
class TestTokenTypeValidation:
    """Test that tokens cannot be substituted for each other."""

    async def test_access_token_has_type_field(self) -> None:
        """Access token must include type field."""
        token = create_access_token({"sub": "testuser", "user_id": 123})
        settings = get_settings()
        payload = jwt.decode(token, settings.jwt_secret_key, algorithms=["HS256"])
        assert payload.get("type") == "access", "Access token missing type field"

    async def test_refresh_token_has_type_field(self) -> None:
        """Refresh token must include type field."""
        token = create_refresh_token({"sub": "testuser", "user_id": 123})
        settings = get_settings()
        payload = jwt.decode(token, settings.jwt_refresh_secret_key, algorithms=["HS256"])
        assert payload.get("type") == "refresh", "Refresh token missing type field"

    async def test_access_token_rejected_as_refresh_token(self) -> None:
        """Access token cannot be used as refresh token."""
        access_token = create_access_token({"sub": "testuser", "user_id": 123})

        with pytest.raises(HTTPException) as exc_info:
            await verify_refresh_token(access_token)

        assert exc_info.value.status_code == 401
        assert "expected refresh token" in str(exc_info.value.detail).lower()

    async def test_refresh_token_rejected_as_access_token(self) -> None:
        """Refresh token cannot be used as access token."""
        refresh_token = create_refresh_token({"sub": "testuser", "user_id": 123})

        with pytest.raises(HTTPException) as exc_info:
            await verify_access_token(refresh_token)

        assert exc_info.value.status_code == 401
        assert "expected access token" in str(exc_info.value.detail).lower()


@pytest.mark.unit()
@pytest.mark.asyncio()
class TestAlgorithmVerification:
    """Test algorithm confusion attack prevention."""

    async def test_algorithm_explicitly_verified(self) -> None:
        """Ensure algorithm is verified during token decoding."""
        get_settings()

        # Create a token with correct secret but try to bypass algorithm verification
        # by creating a token with "none" algorithm
        malicious_payload = {
            "sub": "attacker",
            "user_id": 999,
            "type": "access",
            "jti": str(uuid.uuid4()),
            "exp": datetime.now(UTC) + timedelta(minutes=30),
            "iat": datetime.now(UTC),
        }

        # Try to create token with "none" algorithm
        malicious_token = jwt.encode(malicious_payload, "", algorithm="none")

        # Should be rejected
        with pytest.raises(HTTPException) as exc_info:
            await verify_access_token(malicious_token)

        assert exc_info.value.status_code == 401

    async def test_wrong_algorithm_rejected(self) -> None:
        """Token with wrong algorithm should be rejected."""
        settings = get_settings()

        # Create token with HS512 instead of HS256
        malicious_payload = {
            "sub": "attacker",
            "user_id": 999,
            "type": "access",
            "jti": str(uuid.uuid4()),
            "exp": datetime.now(UTC) + timedelta(minutes=30),
            "iat": datetime.now(UTC),
        }

        malicious_token = jwt.encode(malicious_payload, settings.jwt_secret_key, algorithm="HS512")

        # Should be rejected
        with pytest.raises(HTTPException) as exc_info:
            await verify_access_token(malicious_token)

        assert exc_info.value.status_code == 401


@pytest.mark.unit()
@pytest.mark.asyncio()
class TestRequiredClaims:
    """Test required claims validation."""

    async def test_access_token_requires_all_claims(self) -> None:
        """Access token must have all required claims."""
        settings = get_settings()

        # Create token missing 'jti' claim
        incomplete_payload = {
            "sub": "testuser",
            "type": "access",
            "exp": datetime.now(UTC) + timedelta(minutes=30),
            "iat": datetime.now(UTC),
            # Missing 'jti'
        }

        incomplete_token = jwt.encode(
            incomplete_payload, settings.jwt_secret_key, algorithm="HS256"
        )

        # Should be rejected
        with pytest.raises(HTTPException) as exc_info:
            await verify_access_token(incomplete_token)

        assert exc_info.value.status_code == 401

    async def test_refresh_token_requires_all_claims(self) -> None:
        """Refresh token must have all required claims."""
        settings = get_settings()

        # Create token missing 'jti' claim
        incomplete_payload = {
            "sub": "testuser",
            "type": "refresh",
            "exp": datetime.now(UTC) + timedelta(days=7),
            "iat": datetime.now(UTC),
            # Missing 'jti'
        }

        incomplete_token = jwt.encode(
            incomplete_payload, settings.jwt_refresh_secret_key, algorithm="HS256"
        )

        # Should be rejected
        with pytest.raises(HTTPException) as exc_info:
            await verify_refresh_token(incomplete_token)

        assert exc_info.value.status_code == 401

    async def test_token_missing_sub_rejected(self) -> None:
        """Token without 'sub' claim should be rejected."""
        settings = get_settings()

        payload_no_sub = {
            "user_id": 123,
            "type": "access",
            "jti": str(uuid.uuid4()),
            "exp": datetime.now(UTC) + timedelta(minutes=30),
            "iat": datetime.now(UTC),
            # Missing 'sub'
        }

        token = jwt.encode(payload_no_sub, settings.jwt_secret_key, algorithm="HS256")

        with pytest.raises(HTTPException) as exc_info:
            await verify_access_token(token)

        assert exc_info.value.status_code == 401

    async def test_token_missing_exp_rejected(self) -> None:
        """Token without 'exp' claim should be rejected."""
        settings = get_settings()

        payload_no_exp = {
            "sub": "testuser",
            "type": "access",
            "jti": str(uuid.uuid4()),
            "iat": datetime.now(UTC),
            # Missing 'exp'
        }

        token = jwt.encode(payload_no_exp, settings.jwt_secret_key, algorithm="HS256")

        with pytest.raises(HTTPException) as exc_info:
            await verify_access_token(token)

        assert exc_info.value.status_code == 401

    async def test_token_missing_iat_rejected(self) -> None:
        """Token without 'iat' claim should be rejected."""
        settings = get_settings()

        payload_no_iat = {
            "sub": "testuser",
            "type": "access",
            "jti": str(uuid.uuid4()),
            "exp": datetime.now(UTC) + timedelta(minutes=30),
            # Missing 'iat'
        }

        token = jwt.encode(payload_no_iat, settings.jwt_secret_key, algorithm="HS256")

        with pytest.raises(HTTPException) as exc_info:
            await verify_access_token(token)

        assert exc_info.value.status_code == 401


@pytest.mark.unit()
@pytest.mark.asyncio()
class TestJTIUniqueness:
    """Test that JTI (JWT ID) is unique for each token."""

    def test_access_tokens_have_unique_jti(self) -> None:
        """Each access token should have a unique JTI."""
        settings = get_settings()

        token1 = create_access_token({"sub": "user1", "user_id": 1})
        token2 = create_access_token({"sub": "user1", "user_id": 1})

        payload1 = jwt.decode(token1, settings.jwt_secret_key, algorithms=["HS256"])
        payload2 = jwt.decode(token2, settings.jwt_secret_key, algorithms=["HS256"])

        assert payload1["jti"] != payload2["jti"], "JTI should be unique for each token"

    def test_refresh_tokens_have_unique_jti(self) -> None:
        """Each refresh token should have a unique JTI."""
        settings = get_settings()

        token1 = create_refresh_token({"sub": "user1", "user_id": 1})
        token2 = create_refresh_token({"sub": "user1", "user_id": 1})

        payload1 = jwt.decode(token1, settings.jwt_refresh_secret_key, algorithms=["HS256"])
        payload2 = jwt.decode(token2, settings.jwt_refresh_secret_key, algorithms=["HS256"])

        assert payload1["jti"] != payload2["jti"], "JTI should be unique for each token"

    def test_jti_is_valid_uuid(self) -> None:
        """JTI should be a valid UUID."""
        settings = get_settings()

        token = create_access_token({"sub": "testuser", "user_id": 123})
        payload = jwt.decode(token, settings.jwt_secret_key, algorithms=["HS256"])

        jti = payload["jti"]
        # Should be able to parse as UUID
        parsed_uuid = uuid.UUID(jti)
        assert str(parsed_uuid) == jti, "JTI should be a valid UUID string"


@pytest.mark.unit()
@pytest.mark.asyncio()
class TestTokenExpiration:
    """Test token expiration is properly validated."""

    async def test_expired_access_token_rejected(self) -> None:
        """Expired access token should be rejected."""
        settings = get_settings()

        # Create token that expired 1 hour ago
        expired_payload = {
            "sub": "testuser",
            "user_id": 123,
            "type": "access",
            "jti": str(uuid.uuid4()),
            "exp": datetime.now(UTC) - timedelta(hours=1),
            "iat": datetime.now(UTC) - timedelta(hours=2),
        }

        expired_token = jwt.encode(expired_payload, settings.jwt_secret_key, algorithm="HS256")

        with pytest.raises(HTTPException) as exc_info:
            await verify_access_token(expired_token)

        assert exc_info.value.status_code == 401

    async def test_expired_refresh_token_rejected(self) -> None:
        """Expired refresh token should be rejected."""
        settings = get_settings()

        # Create token that expired 1 day ago
        expired_payload = {
            "sub": "testuser",
            "user_id": 123,
            "type": "refresh",
            "jti": str(uuid.uuid4()),
            "exp": datetime.now(UTC) - timedelta(days=1),
            "iat": datetime.now(UTC) - timedelta(days=8),
        }

        expired_token = jwt.encode(
            expired_payload, settings.jwt_refresh_secret_key, algorithm="HS256"
        )

        with pytest.raises(HTTPException) as exc_info:
            await verify_refresh_token(expired_token)

        assert exc_info.value.status_code == 401


@pytest.mark.unit()
class TestSettingsValidation:
    """Test that settings validation enforces security requirements."""

    def test_jwt_secrets_are_different(self) -> None:
        """Settings should enforce different secrets."""
        settings = get_settings()

        assert settings.jwt_secret_key != settings.jwt_refresh_secret_key, (
            "jwt_secret_key and jwt_refresh_secret_key must be different. "
            "This is enforced by the settings validator."
        )

    def test_jwt_secrets_are_strong(self) -> None:
        """JWT secrets should meet minimum strength requirements."""
        settings = get_settings()

        # Both secrets should be at least 32 characters
        assert len(settings.jwt_secret_key) >= 32, "jwt_secret_key must be at least 32 characters"
        assert (
            len(settings.jwt_refresh_secret_key) >= 32
        ), "jwt_refresh_secret_key must be at least 32 characters"

        # Secrets should have sufficient entropy (not too repetitive)
        assert len(set(settings.jwt_secret_key)) >= 16, "jwt_secret_key has insufficient entropy"
        assert (
            len(set(settings.jwt_refresh_secret_key)) >= 16
        ), "jwt_refresh_secret_key has insufficient entropy"
