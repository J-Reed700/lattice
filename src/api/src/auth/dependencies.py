"""FastAPI dependencies for authentication."""

from fastapi import Depends, HTTPException, status
from fastapi.security import OAuth2PasswordBearer
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from src.db import get_session

from .jwt import verify_access_token
from .models import User
from .schemas import TokenData

# OAuth2 scheme - looks for "Authorization: Bearer <token>" header
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="/api/v1/auth/token")


async def get_current_user(
    token: str = Depends(oauth2_scheme), db: AsyncSession = Depends(get_session)
) -> User:
    """
    Get current authenticated user from JWT token.

    This dependency extracts and validates the JWT token from the
    Authorization header, then fetches the corresponding user from the database.

    Args:
        token: JWT token from Authorization header
        db: Database session

    Returns:
        User object for the authenticated user

    Raises:
        HTTPException: 401 if token is invalid or user not found

    Example:
        >>> @router.get("/protected")
        >>> async def protected_route(user: User = Depends(get_current_user)):
        ...     return {"user_id": user.id, "username": user.username}
    """
    credentials_exception = HTTPException(
        status_code=status.HTTP_401_UNAUTHORIZED,
        detail="Could not validate credentials",
        headers={"WWW-Authenticate": "Bearer"},
    )

    try:
        payload = await verify_access_token(token)
        username: str | None = payload.get("sub")
        user_id: int | None = payload.get("user_id")

        if username is None:
            raise credentials_exception

        token_data = TokenData(username=username, user_id=user_id)
    except HTTPException:
        raise
    except Exception:
        raise credentials_exception

    # Fetch user from database
    stmt = select(User).where(User.username == token_data.username)
    result = await db.execute(stmt)
    user = result.scalar_one_or_none()

    if user is None:
        raise credentials_exception

    return user


async def get_current_active_user(current_user: User = Depends(get_current_user)) -> User:
    """
    Get current user and verify they are active.

    This adds an additional check on top of get_current_user to ensure
    the account hasn't been deactivated.

    Args:
        current_user: User from get_current_user dependency

    Returns:
        User object if active

    Raises:
        HTTPException: 403 if user account is inactive

    Example:
        >>> @router.post("/upload")
        >>> async def upload_file(
        ...     file: UploadFile,
        ...     user: User = Depends(get_current_active_user)
        ... ):
        ...     # Only active users can upload files
        ...     return await process_upload(file, user)
    """
    if not current_user.is_active:
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail="Inactive user account")
    return current_user


async def get_current_superuser(current_user: User = Depends(get_current_active_user)) -> User:
    """
    Get current user and verify they have superuser privileges.

    Use this dependency for admin-only endpoints.

    Args:
        current_user: User from get_current_active_user

    Returns:
        User object if superuser

    Raises:
        HTTPException: 403 if user is not a superuser
    """
    if not current_user.is_superuser:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Insufficient permissions. Superuser access required.",
        )
    return current_user


# Optional auth dependency - for endpoints that work with or without auth
async def get_current_user_optional(
    token: str | None = Depends(oauth2_scheme), db: AsyncSession = Depends(get_session)
) -> User | None:
    """
    Get current user if authenticated, None otherwise.

    Use for endpoints that provide enhanced features when authenticated
    but still work without authentication.

    Returns:
        User if authenticated, None otherwise
    """
    if token is None:
        return None

    try:
        return await get_current_user(token, db)
    except HTTPException:
        return None
