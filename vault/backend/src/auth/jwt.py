"""JWT token creation and verification."""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
import uuid

from fastapi import HTTPException, status
from jose import JWTError, jwt

from src.auth.token_blacklist import token_blacklist
from src.config.settings import get_settings


def create_access_token(data: dict, expires_delta: timedelta | None = None) -> str:
    """
    Create a JWT access token with unique identifier.

    Args:
        data: Dictionary containing claims to encode in the token
        expires_delta: Optional custom expiration time (defaults to settings)

    Returns:
        Encoded JWT token string

    Example:
        >>> token = create_access_token({"sub": "john_doe", "user_id": 123})
        >>> # Returns: "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
    """
    settings = get_settings()
    to_encode = data.copy()

    if expires_delta:
        expire = datetime.now(UTC) + expires_delta
    else:
        expire = datetime.now(UTC) + timedelta(minutes=settings.jwt_access_token_expire_minutes)

    to_encode.update(
        {
            "exp": expire,
            "iat": datetime.now(UTC),
            "type": "access",
            "jti": str(uuid.uuid4()),
        }
    )

    encoded_jwt = jwt.encode(to_encode, settings.jwt_secret_key, algorithm=settings.jwt_algorithm)

    return encoded_jwt


async def verify_access_token(token: str) -> dict:
    """
    Verify and decode a JWT access token with strict validation.

    Args:
        token: JWT token string to verify

    Returns:
        Decoded token payload as dictionary

    Raises:
        HTTPException: If token is invalid, expired, malformed, revoked, or wrong type

    Example:
        >>> payload = await verify_access_token(token)
        >>> print(payload["sub"])  # "john_doe"
    """
    settings = get_settings()

    try:
        payload = jwt.decode(
            token,
            settings.jwt_secret_key,
            algorithms=[settings.jwt_algorithm],
            options={
                "verify_signature": True,
                "verify_exp": True,
                "verify_iat": True,
                "require": ["exp", "iat", "sub", "type", "jti"],
            },
        )

        # Verify token type
        if payload.get("type") != "access":
            raise HTTPException(
                status_code=status.HTTP_401_UNAUTHORIZED,
                detail="Invalid token type - expected access token",
                headers={"WWW-Authenticate": "Bearer"},
            )

    except JWTError as e:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail=f"Invalid token: {e!s}",
            headers={"WWW-Authenticate": "Bearer"},
        )

    token_id = payload.get("jti")
    user_id = str(payload.get("sub"))

    if token_id and await token_blacklist.is_token_blacklisted(token_id):
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Token has been revoked",
            headers={"WWW-Authenticate": "Bearer"},
        )

    if await token_blacklist.is_user_blacklisted(user_id):
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="All user tokens have been revoked",
            headers={"WWW-Authenticate": "Bearer"},
        )

    return payload


def create_refresh_token(data: dict, expires_delta: timedelta | None = None) -> str:
    """
    Create a JWT refresh token with separate secret and unique identifier.

    Args:
        data: Dictionary containing claims to encode in the token
        expires_delta: Optional custom expiration time (defaults to settings)

    Returns:
        Encoded JWT refresh token string

    Example:
        >>> token = create_refresh_token({"sub": "john_doe", "user_id": 123})
        >>> # Returns: "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
    """
    settings = get_settings()
    to_encode = data.copy()

    if expires_delta:
        expire = datetime.now(UTC) + expires_delta
    else:
        expire = datetime.now(UTC) + timedelta(days=settings.jwt_refresh_token_expire_days)

    to_encode.update(
        {
            "exp": expire,
            "iat": datetime.now(UTC),
            "type": "refresh",
            "jti": str(uuid.uuid4()),
        }
    )

    encoded_jwt = jwt.encode(
        to_encode, settings.jwt_refresh_secret_key, algorithm=settings.jwt_algorithm
    )

    return encoded_jwt


async def verify_refresh_token(token: str) -> dict:
    """
    Verify and decode a JWT refresh token with separate secret and strict validation.

    Args:
        token: JWT refresh token string to verify

    Returns:
        Decoded token payload as dictionary

    Raises:
        HTTPException: If token is invalid, expired, malformed, revoked, or not a refresh token

    Example:
        >>> payload = await verify_refresh_token(token)
        >>> print(payload["sub"])  # "john_doe"
    """
    settings = get_settings()

    try:
        payload = jwt.decode(
            token,
            settings.jwt_refresh_secret_key,
            algorithms=[settings.jwt_algorithm],
            options={
                "verify_signature": True,
                "verify_exp": True,
                "verify_iat": True,
                "require": ["exp", "iat", "sub", "type", "jti"],
            },
        )

        # Verify token type
        if payload.get("type") != "refresh":
            raise HTTPException(
                status_code=status.HTTP_401_UNAUTHORIZED,
                detail="Invalid token type - expected refresh token",
                headers={"WWW-Authenticate": "Bearer"},
            )

    except JWTError as e:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail=f"Invalid refresh token: {e!s}",
            headers={"WWW-Authenticate": "Bearer"},
        )

    token_id = payload.get("jti")
    user_id = str(payload.get("sub"))

    if token_id and await token_blacklist.is_token_blacklisted(token_id):
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Refresh token has been revoked",
            headers={"WWW-Authenticate": "Bearer"},
        )

    if await token_blacklist.is_user_blacklisted(user_id):
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="All user tokens have been revoked",
            headers={"WWW-Authenticate": "Bearer"},
        )

    return payload


def create_temp_token(data: dict) -> str:
    """Create temporary token for MFA verification (5 minute expiry).

    Args:
        data: Dictionary containing claims to encode in the token

    Returns:
        Encoded JWT temporary token string

    Example:
        >>> token = create_temp_token({"sub": "123"})
        >>> # Returns: "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
    """
    settings = get_settings()
    to_encode = data.copy()
    now = datetime.now(UTC)
    expire = now + timedelta(minutes=5)

    to_encode.update({"exp": expire, "iat": now, "type": "temp", "jti": str(uuid.uuid4())})

    return jwt.encode(to_encode, settings.jwt_secret_key, algorithm=settings.jwt_algorithm)


async def verify_temp_token(token: str) -> dict:
    """Verify temporary MFA token with strict validation and blacklist checking.

    Args:
        token: JWT temporary token string to verify

    Returns:
        Decoded token payload as dictionary

    Raises:
        HTTPException: If token is invalid, expired, revoked, or not a temp token

    Example:
        >>> payload = await verify_temp_token(token)
        >>> print(payload["sub"])  # "123"
    """
    settings = get_settings()

    try:
        payload = jwt.decode(
            token,
            settings.jwt_secret_key,
            algorithms=[settings.jwt_algorithm],
            options={
                "verify_signature": True,
                "verify_exp": True,
                "verify_iat": True,
                "require": ["exp", "iat", "sub", "type", "jti"],
            },
        )

        if payload.get("type") != "temp":
            raise HTTPException(
                status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid token type"
            )

    except JWTError as e:
        if "expired" in str(e).lower():
            raise HTTPException(
                status_code=status.HTTP_401_UNAUTHORIZED,
                detail="Temporary token expired. Please login again.",
            )
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid temporary token",
        )

    # Check if temp token has been blacklisted (revoked/used)
    token_id = payload.get("jti")
    user_id = str(payload.get("sub"))

    if token_id and await token_blacklist.is_token_blacklisted(token_id):
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Temporary token has been revoked or already used",
            headers={"WWW-Authenticate": "Bearer"},
        )

    if await token_blacklist.is_user_blacklisted(user_id):
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="All user tokens have been revoked",
            headers={"WWW-Authenticate": "Bearer"},
        )

    return payload
