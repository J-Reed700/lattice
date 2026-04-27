"""Multi-factor authentication endpoints."""

from __future__ import annotations

from datetime import UTC, datetime

from fastapi import APIRouter, Body, Depends, HTTPException, status
from sqlalchemy.ext.asyncio import AsyncSession

from src.auth.dependencies import get_current_active_user
from src.auth.models import User, UserMFA
from src.db import get_session
from src.schemas.mfa import (
    MFASetupResponse,
    MFAStatusResponse,
    MFAVerifyRequest,
)
from src.services.mfa_service import MFAService

router = APIRouter()
mfa_service = MFAService()


@router.post("/setup", response_model=MFASetupResponse)
async def setup_mfa(
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_session),
) -> MFASetupResponse:
    """Initialize MFA setup for user.

    Generates TOTP secret, QR code, and backup codes.
    MFA is not enabled until verification is complete.
    """
    if current_user.mfa and current_user.mfa.is_enabled:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="MFA is already enabled. Disable first to re-setup.",
        )

    secret = mfa_service.generate_secret()
    # Returns (plain_codes, hashed_codes_csv)
    plain_backup_codes, hashed_backup_codes = mfa_service.generate_backup_codes()
    qr_code = mfa_service.generate_qr_code(secret, current_user.email)

    if current_user.mfa:
        current_user.mfa.secret = secret
        # Store HASHED codes in database for security
        current_user.mfa.backup_codes = hashed_backup_codes
        current_user.mfa.is_enabled = False
        current_user.mfa.verified_at = None
    else:
        mfa = UserMFA(
            user_id=current_user.id,
            secret=secret,
            # Store HASHED codes in database for security
            backup_codes=hashed_backup_codes,
            is_enabled=False,
        )
        db.add(mfa)

    await db.commit()

    # Return PLAIN codes to user (they can only see these ONCE)
    return MFASetupResponse(secret=secret, qr_code=qr_code, backup_codes=plain_backup_codes)


@router.post("/verify")
async def verify_and_enable_mfa(
    request: MFAVerifyRequest,
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_session),
) -> dict[str, str]:
    """Verify TOTP code and enable MFA.

    User must provide valid TOTP code from authenticator app.
    """
    if not current_user.mfa:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="MFA not initialized. Call /setup first.",
        )

    if not mfa_service.verify_totp(current_user.mfa.secret, request.code):
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST, detail="Invalid verification code"
        )

    current_user.mfa.is_enabled = True
    current_user.mfa.verified_at = datetime.now(UTC)
    await db.commit()

    return {"message": "MFA enabled successfully"}


@router.post("/disable")
async def disable_mfa(
    code: str = Body(..., embed=True),
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_session),
) -> dict[str, str]:
    """Disable MFA for current user.

    Requires valid TOTP code or backup code for verification.
    """
    if not current_user.mfa or not current_user.mfa.is_enabled:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="MFA is not enabled")

    totp_valid = mfa_service.verify_totp(current_user.mfa.secret, code)
    backup_valid = mfa_service.verify_backup_code(current_user.mfa.backup_codes, code)

    if not (totp_valid or backup_valid):
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Invalid code")

    if backup_valid:
        current_user.mfa.backup_codes = mfa_service.remove_backup_code(
            current_user.mfa.backup_codes, code
        )

    current_user.mfa.is_enabled = False
    await db.commit()

    return {"message": "MFA disabled successfully"}


@router.get("/status", response_model=MFAStatusResponse)
async def mfa_status(
    current_user: User = Depends(get_current_active_user),
) -> MFAStatusResponse:
    """Get MFA status for current user."""
    if not current_user.mfa:
        return MFAStatusResponse(enabled=False, verified=False)

    return MFAStatusResponse(
        enabled=current_user.mfa.is_enabled,
        verified=current_user.mfa.verified_at is not None,
    )
