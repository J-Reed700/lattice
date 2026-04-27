"""Authentication API endpoints."""

from __future__ import annotations

from datetime import UTC, datetime, timedelta

from fastapi import APIRouter, Body, Depends, HTTPException, Request, status
from fastapi.security import OAuth2PasswordRequestForm
from jose import JWTError
from jose import jwt as jwt_lib
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession
import structlog

from src.auth.dependencies import get_current_active_user, oauth2_scheme
from src.auth.jwt import (
    create_access_token,
    create_refresh_token,
    create_temp_token,
    verify_refresh_token,
    verify_temp_token,
)
from src.auth.models import RefreshToken, User
from src.auth.password import hash_password, verify_password
from src.auth.schemas import (
    RefreshTokenRequest,
    RefreshTokenResponse,
    UserCreate,
    UserResponse,
)
from src.auth.token_blacklist import token_blacklist
from src.config.settings import get_settings
from src.db import get_session
from src.middleware.rate_limit import (
    check_login_rate_limit,
    limiter,
    reset_login_rate_limit,
)
from src.monitoring.logging import log_security_event
from src.schemas.mfa import MFALoginRequest, TokenResponseWithMFA
from src.services.mfa_service import MFAService

logger = structlog.get_logger(__name__)

router = APIRouter(prefix="/auth", tags=["authentication"])


@router.post("/register", response_model=UserResponse, status_code=status.HTTP_201_CREATED)
@limiter.limit("3/hour")
async def register(
    request: Request, user_data: UserCreate, db: AsyncSession = Depends(get_session)
) -> UserResponse:
    """
    Register a new user account.

    Creates a new user with hashed password. Username and email must be unique.

    **Security**:
    - Password is hashed with bcrypt before storage
    - Rate limited to 3 registrations/hour per IP to prevent spam
    """
    stmt = select(User).where(User.username == user_data.username)
    result = await db.execute(stmt)
    if result.scalar_one_or_none():
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST, detail="Username already registered"
        )

    stmt = select(User).where(User.email == user_data.email)
    result = await db.execute(stmt)
    if result.scalar_one_or_none():
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST, detail="Email already registered"
        )

    hashed_password = hash_password(user_data.password)
    new_user = User(
        username=user_data.username, email=user_data.email, hashed_password=hashed_password
    )

    db.add(new_user)
    await db.commit()
    await db.refresh(new_user)

    return UserResponse.from_orm(new_user)


@router.post("/token", response_model=TokenResponseWithMFA)
@limiter.limit("5/minute")
async def login(
    request: Request,
    form_data: OAuth2PasswordRequestForm = Depends(OAuth2PasswordRequestForm),
    db: AsyncSession = Depends(get_session),
) -> TokenResponseWithMFA:
    """
    Login with username and password to get JWT tokens.

    **OAuth2 compatible endpoint** - accepts form data with:
    - username: User's username
    - password: User's password

    Returns both access and refresh tokens:
    - Access token: Short-lived (30 min), use in Authorization header
    - Refresh token: Long-lived (7 days), use to get new access tokens

    **Security**:
    - Uses bcrypt to verify password hash
    - Rate limited: 5 attempts/minute per IP, 10 attempts/hour per username
    - Prevents brute force and credential stuffing attacks
    - Implements token rotation for refresh tokens
    """
    settings = get_settings()

    await check_login_rate_limit(request, form_data.username)

    stmt = select(User).where(User.username == form_data.username)
    result = await db.execute(stmt)
    user = result.scalar_one_or_none()

    if not user or not verify_password(form_data.password, user.hashed_password):
        log_security_event(
            logger,
            "login_failed",
            severity="warning",
            username=form_data.username,
            ip_address=request.client.host if request.client else "unknown",
            reason="invalid_credentials",
        )
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Incorrect username or password",
            headers={"WWW-Authenticate": "Bearer"},
        )

    if not user.is_active:
        log_security_event(
            logger,
            "login_failed",
            severity="warning",
            username=form_data.username,
            user_id=user.id,
            ip_address=request.client.host if request.client else "unknown",
            reason="inactive_account",
        )
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail="Inactive user account")

    # Check if MFA is enabled
    if user.mfa and user.mfa.is_enabled:
        temp_token = create_temp_token({"sub": str(user.id)})
        log_security_event(
            logger,
            "mfa_required",
            severity="info",
            username=user.username,
            user_id=user.id,
            ip_address=request.client.host if request.client else "unknown",
        )
        return TokenResponseWithMFA(
            access_token="",
            temp_token=temp_token,
            mfa_required=True,
        )

    # Create access token
    access_token_expires = timedelta(minutes=settings.jwt_access_token_expire_minutes)
    access_token = create_access_token(
        data={"sub": user.username, "user_id": user.id}, expires_delta=access_token_expires
    )

    # Create refresh token
    refresh_token_expires = timedelta(days=settings.jwt_refresh_token_expire_days)
    refresh_token_jwt = create_refresh_token(
        data={"sub": user.username, "user_id": user.id}, expires_delta=refresh_token_expires
    )

    # Store refresh token in database
    refresh_token_record = RefreshToken(
        user_id=user.id,
        token=refresh_token_jwt,
        expires_at=datetime.now(UTC) + refresh_token_expires,
    )
    db.add(refresh_token_record)
    await db.commit()

    await reset_login_rate_limit(form_data.username)

    log_security_event(
        logger,
        "login_successful",
        severity="info",
        username=user.username,
        user_id=user.id,
        ip_address=request.client.host if request.client else "unknown",
    )

    return TokenResponseWithMFA(
        access_token=access_token,
        refresh_token=refresh_token_jwt,
        token_type="bearer",
        mfa_required=False,
    )


@router.post("/refresh", response_model=RefreshTokenResponse)
@limiter.limit("10/minute")
async def refresh_token(
    request: Request, refresh_request: RefreshTokenRequest, db: AsyncSession = Depends(get_session)
) -> RefreshTokenResponse:
    """
    Refresh access token using a valid refresh token.

    Implements token rotation: returns new access AND refresh tokens,
    and revokes the old refresh token.

    **Security**:
    - Verifies refresh token signature and expiration
    - Checks token hasn't been revoked
    - Implements token rotation (one-time use)
    - Rate limited to prevent abuse

    **Usage**:
    ```
    POST /auth/refresh
    {
        "refresh_token": "eyJhbGc..."
    }
    ```
    """
    settings = get_settings()

    # Verify JWT signature and decode
    try:
        payload = await verify_refresh_token(refresh_request.refresh_token)
    except HTTPException:
        raise
    except JWTError as e:
        log_security_event(
            logger,
            "token_validation_failed",
            severity="warning",
            ip_address=request.client.host if request.client else "unknown",
            reason="invalid_jwt_signature",
            error=str(e),
        )
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail=f"Invalid refresh token: {e!s}",
            headers={"WWW-Authenticate": "Bearer"},
        )

    # Check token exists in database and is valid
    stmt = select(RefreshToken).where(RefreshToken.token == refresh_request.refresh_token)
    result = await db.execute(stmt)
    token_record = result.scalar_one_or_none()

    if not token_record:
        log_security_event(
            logger,
            "token_validation_failed",
            severity="warning",
            ip_address=request.client.host if request.client else "unknown",
            reason="token_not_found",
        )
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Refresh token not found",
            headers={"WWW-Authenticate": "Bearer"},
        )

    if not token_record.is_valid():
        log_security_event(
            logger,
            "token_validation_failed",
            severity="warning",
            ip_address=request.client.host if request.client else "unknown",
            reason="token_expired_or_revoked",
        )
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Refresh token expired or revoked",
            headers={"WWW-Authenticate": "Bearer"},
        )

    # Get user
    user_id = payload.get("user_id")
    stmt = select(User).where(User.id == user_id)
    result = await db.execute(stmt)
    user = result.scalar_one_or_none()

    if not user or not user.is_active:
        log_security_event(
            logger,
            "token_validation_failed",
            severity="warning",
            user_id=user_id,
            ip_address=request.client.host if request.client else "unknown",
            reason="user_not_found_or_inactive",
        )
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="User not found or inactive",
            headers={"WWW-Authenticate": "Bearer"},
        )

    # Revoke old refresh token (token rotation)
    token_record.revoked = True

    # Create new access token
    access_token_expires = timedelta(minutes=settings.jwt_access_token_expire_minutes)
    access_token = create_access_token(
        data={"sub": user.username, "user_id": user.id}, expires_delta=access_token_expires
    )

    # Create new refresh token
    refresh_token_expires = timedelta(days=settings.jwt_refresh_token_expire_days)
    new_refresh_token_jwt = create_refresh_token(
        data={"sub": user.username, "user_id": user.id}, expires_delta=refresh_token_expires
    )

    # Store new refresh token in database
    new_refresh_token_record = RefreshToken(
        user_id=user.id,
        token=new_refresh_token_jwt,
        expires_at=datetime.now(UTC) + refresh_token_expires,
    )
    db.add(new_refresh_token_record)
    await db.commit()

    return RefreshTokenResponse(
        access_token=access_token,
        refresh_token=new_refresh_token_jwt,
        token_type="bearer",
        expires_in=settings.jwt_access_token_expire_minutes * 60,
    )


@router.get("/me", response_model=UserResponse)
async def get_current_user_info(
    current_user: User = Depends(get_current_active_user),
) -> UserResponse:
    """
    Get current authenticated user information.

    **Requires authentication** - returns user data for the token owner.
    """
    return UserResponse.from_orm(current_user)


@router.post("/logout")
async def logout(
    current_user: User = Depends(get_current_active_user),
    token: str = Depends(oauth2_scheme),
    db: AsyncSession = Depends(get_session),
) -> dict:
    """
    Logout current user and revoke current access token.

    Revokes:
    - Current access token (added to blacklist)
    - All refresh tokens for the user

    **Client implementation**:
    - Delete both access and refresh tokens from client storage
    - Remove Authorization header from future requests
    """
    # Decode current token to get jti (without full verification)
    payload = jwt_lib.decode(token, options={"verify_signature": False})
    token_id = payload.get("jti")

    if token_id:
        await token_blacklist.add_token(token_id)

    # Revoke all user's refresh tokens
    stmt = select(RefreshToken).where(
        RefreshToken.user_id == current_user.id, RefreshToken.revoked == False
    )
    result = await db.execute(stmt)
    tokens = result.scalars().all()

    for token in tokens:
        token.revoked = True

    await db.commit()

    log_security_event(
        logger,
        "logout_successful",
        severity="info",
        user_id=current_user.id,
        username=current_user.username,
        tokens_revoked=len(tokens),
    )

    return {
        "message": "Successfully logged out",
        "detail": "Access token and all refresh tokens revoked",
    }


@router.post("/revoke")
async def revoke_token(
    token: str = Body(..., embed=True),
    current_user: User = Depends(get_current_active_user),
) -> dict:
    """
    Revoke a specific token (access or refresh).

    Requires authentication with a valid token to revoke another token.
    Useful for revoking tokens from specific devices or sessions.

    **Usage**:
    ```
    POST /auth/revoke
    {
        "token": "eyJhbGc..."
    }
    ```
    """
    # Decode without verification to get jti
    try:
        payload = jwt_lib.decode(token, options={"verify_signature": False})
        token_id = payload.get("jti")

        if not token_id:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Token does not have a valid identifier",
            )

        await token_blacklist.add_token(token_id)

        log_security_event(
            logger,
            "token_revoked",
            severity="info",
            user_id=current_user.id,
            username=current_user.username,
            token_id=token_id,
        )

        return {"message": "Token revoked successfully"}

    except Exception as e:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST, detail=f"Invalid token format: {e!s}"
        )


@router.post("/revoke-all")
async def revoke_all_tokens(
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_session),
) -> dict:
    """
    Revoke all tokens for current user.

    This will invalidate all existing access and refresh tokens.
    User will need to login again.

    **Use cases**:
    - Security breach suspected
    - Password reset initiated
    - Logout from all devices
    """
    # Revoke all access tokens via blacklist
    await token_blacklist.revoke_all_user_tokens(str(current_user.id))

    # Revoke all refresh tokens in database
    stmt = select(RefreshToken).where(
        RefreshToken.user_id == current_user.id, RefreshToken.revoked == False
    )
    result = await db.execute(stmt)
    tokens = result.scalars().all()

    for token in tokens:
        token.revoked = True

    await db.commit()

    log_security_event(
        logger,
        "all_tokens_revoked",
        severity="warning",
        user_id=current_user.id,
        username=current_user.username,
        tokens_revoked=len(tokens),
    )

    return {
        "message": "All tokens revoked successfully. Please login again.",
        "tokens_revoked": len(tokens),
    }


@router.post("/change-password")
async def change_password(
    old_password: str = Body(...),
    new_password: str = Body(...),
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_session),
) -> dict:
    """
    Change password and revoke all existing tokens.

    Security best practice: When password changes, all existing sessions
    should be invalidated to prevent unauthorized access.

    **Requirements**:
    - Old password must be correct
    - New password must meet security requirements (validated by Pydantic)

    After password change:
    - All access tokens blacklisted
    - All refresh tokens revoked
    - User must login again with new password
    """
    # Verify old password
    if not verify_password(old_password, current_user.hashed_password):
        log_security_event(
            logger,
            "password_change_failed",
            severity="warning",
            user_id=current_user.id,
            username=current_user.username,
            reason="incorrect_old_password",
        )
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST, detail="Incorrect current password"
        )

    # Update password
    current_user.hashed_password = hash_password(new_password)

    # Revoke all tokens
    await token_blacklist.revoke_all_user_tokens(str(current_user.id))

    # Revoke all refresh tokens in database
    stmt = select(RefreshToken).where(
        RefreshToken.user_id == current_user.id, RefreshToken.revoked == False
    )
    result = await db.execute(stmt)
    tokens = result.scalars().all()

    for token in tokens:
        token.revoked = True

    await db.commit()

    log_security_event(
        logger,
        "password_changed",
        severity="info",
        user_id=current_user.id,
        username=current_user.username,
        tokens_revoked=len(tokens),
    )

    return {
        "message": "Password changed successfully. All tokens revoked. Please login again.",
        "tokens_revoked": len(tokens),
    }


@router.get("/token-stats")
async def token_stats(
    current_user: User = Depends(get_current_active_user),
) -> dict:
    """
    Get token blacklist statistics.

    Returns information about currently blacklisted tokens and users.

    **Note**: In production, restrict this endpoint to admin users only.
    """
    stats = await token_blacklist.get_stats()

    return {
        "blacklist_stats": stats,
        "user_id": current_user.id,
        "username": current_user.username,
    }


@router.post("/token/mfa", response_model=TokenResponseWithMFA)
@limiter.limit("5/minute")
async def login_mfa(
    request: Request,
    mfa_request: MFALoginRequest,
    db: AsyncSession = Depends(get_session),
) -> TokenResponseWithMFA:
    """
    Complete login with MFA code.

    After initial login with MFA enabled, use the temporary token
    and MFA code to complete authentication and get access tokens.

    **Security**:
    - Temporary token expires in 5 minutes
    - Supports TOTP codes and backup codes
    - Backup codes are single-use and removed after verification
    """
    settings = get_settings()

    # Verify temporary token
    payload = await verify_temp_token(mfa_request.temp_token)
    user_id = int(payload.get("sub"))
    temp_token_id = payload.get("jti")

    # Check per-token rate limiting to prevent brute-force attacks
    from src.auth.mfa_rate_limiter import mfa_rate_limiter
    from src.auth.token_blacklist import token_blacklist

    if temp_token_id and await mfa_rate_limiter.is_rate_limited(temp_token_id):
        # Rate limit exceeded - blacklist the temp token immediately
        await token_blacklist.add_token(temp_token_id)
        log_security_event(
            logger,
            "mfa_rate_limit_exceeded",
            severity="warning",
            user_id=user_id,
            ip_address=request.client.host if request.client else "unknown",
            temp_token_id=temp_token_id,
        )
        raise HTTPException(
            status_code=status.HTTP_429_TOO_MANY_REQUESTS,
            detail="Too many failed MFA attempts. Please login again.",
        )

    # Get user
    result = await db.execute(select(User).where(User.id == user_id))
    user = result.scalar_one_or_none()

    if not user or not user.mfa or not user.mfa.is_enabled:
        # Record failed attempt even for invalid setup (defense in depth)
        if temp_token_id:
            await mfa_rate_limiter.record_attempt(temp_token_id, success=False)
        log_security_event(
            logger,
            "mfa_verification_failed",
            severity="warning",
            user_id=user_id,
            ip_address=request.client.host if request.client else "unknown",
            reason="invalid_mfa_setup",
        )
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid MFA setup",
        )

    # Verify MFA code (TOTP or backup code)
    mfa_service = MFAService()
    totp_valid = mfa_service.verify_totp(user.mfa.secret, mfa_request.mfa_code)
    backup_valid = mfa_service.verify_backup_code(user.mfa.backup_codes, mfa_request.mfa_code)

    if totp_valid:
        # Record successful attempt
        if temp_token_id:
            await mfa_rate_limiter.record_attempt(temp_token_id, success=True)
        log_security_event(
            logger,
            "mfa_verification_successful",
            severity="info",
            username=user.username,
            user_id=user.id,
            ip_address=request.client.host if request.client else "unknown",
            method="totp",
        )
    elif backup_valid:
        # Backup code valid - remove it (one-time use)
        user.mfa.backup_codes = mfa_service.remove_backup_code(
            user.mfa.backup_codes,
            mfa_request.mfa_code,
        )
        await db.commit()
        # Record successful attempt
        if temp_token_id:
            await mfa_rate_limiter.record_attempt(temp_token_id, success=True)
        log_security_event(
            logger,
            "mfa_verification_successful",
            severity="info",
            username=user.username,
            user_id=user.id,
            ip_address=request.client.host if request.client else "unknown",
            method="backup_code",
        )
    else:
        # Record failed attempt
        if temp_token_id:
            await mfa_rate_limiter.record_attempt(temp_token_id, success=False)
            remaining = await mfa_rate_limiter.get_remaining_attempts(temp_token_id)
            log_security_event(
                logger,
                "mfa_verification_failed",
                severity="warning",
                username=user.username,
                user_id=user.id,
                ip_address=request.client.host if request.client else "unknown",
                reason="invalid_code",
                remaining_attempts=remaining,
            )
        else:
            log_security_event(
                logger,
                "mfa_verification_failed",
                severity="warning",
                username=user.username,
                user_id=user.id,
                ip_address=request.client.host if request.client else "unknown",
                reason="invalid_code",
            )
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid MFA code",
        )

    # CRITICAL SECURITY: Blacklist the temp token BEFORE generating real tokens
    # This prevents race condition where token could be replayed before blacklisting
    temp_token_id = payload.get("jti")
    if temp_token_id:
        await token_blacklist.add_token(temp_token_id)
        log_security_event(
            logger,
            "temp_token_blacklisted",
            severity="info",
            username=user.username,
            user_id=user.id,
            ip_address=request.client.host if request.client else "unknown",
            temp_token_id=temp_token_id,
        )

    # Generate real tokens (after blacklisting temp token)
    access_token_expires = timedelta(minutes=settings.jwt_access_token_expire_minutes)
    access_token = create_access_token(
        data={"sub": user.username, "user_id": user.id}, expires_delta=access_token_expires
    )

    refresh_token_expires = timedelta(days=settings.jwt_refresh_token_expire_days)
    refresh_token_jwt = create_refresh_token(
        data={"sub": user.username, "user_id": user.id}, expires_delta=refresh_token_expires
    )

    # Store refresh token in database
    refresh_token_record = RefreshToken(
        user_id=user.id,
        token=refresh_token_jwt,
        expires_at=datetime.now(UTC) + refresh_token_expires,
    )
    db.add(refresh_token_record)
    await db.commit()

    return TokenResponseWithMFA(
        access_token=access_token,
        refresh_token=refresh_token_jwt,
        token_type="bearer",
        mfa_required=False,
    )
