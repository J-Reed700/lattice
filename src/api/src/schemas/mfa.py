"""Pydantic schemas for MFA."""

from __future__ import annotations

from pydantic import BaseModel, Field


class MFASetupResponse(BaseModel):
    """Response for MFA setup initialization."""

    secret: str = Field(..., description="TOTP secret for manual entry")
    qr_code: str = Field(..., description="QR code data URL for scanning")
    backup_codes: list[str] = Field(..., description="Backup codes for recovery")


class MFAVerifyRequest(BaseModel):
    """Request to verify MFA code."""

    code: str = Field(..., min_length=6, max_length=6, description="6-digit TOTP code")


class MFALoginRequest(BaseModel):
    """Request to complete MFA login."""

    temp_token: str = Field(..., min_length=10, max_length=500, description="Temporary MFA token")
    mfa_code: str = Field(
        ..., min_length=6, max_length=8, description="6-digit TOTP code or 8-character backup code"
    )


class MFAStatusResponse(BaseModel):
    """MFA status for a user."""

    enabled: bool = Field(..., description="Whether MFA is enabled")
    verified: bool = Field(..., description="Whether MFA has been verified")


class TokenResponseWithMFA(BaseModel):
    """Token response with MFA requirement indicator."""

    access_token: str
    refresh_token: str | None = None
    token_type: str = "bearer"
    mfa_required: bool = False
    temp_token: str | None = None
