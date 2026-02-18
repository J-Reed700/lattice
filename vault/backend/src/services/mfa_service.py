"""Multi-factor authentication service using TOTP."""

from __future__ import annotations

import base64
import io
import secrets
from typing import TYPE_CHECKING

import pyotp
import qrcode
import structlog

if TYPE_CHECKING:
    from PIL.Image import Image

from src.auth.password import pwd_context

logger = structlog.get_logger(__name__)


class MFAService:
    """Service for managing multi-factor authentication."""

    @staticmethod
    def generate_secret() -> str:
        """Generate a random TOTP secret.

        Returns:
            Base32-encoded secret string
        """
        return pyotp.random_base32()

    @staticmethod
    def generate_qr_code(secret: str, user_email: str, issuer: str = "Recall") -> str:
        """Generate QR code for TOTP setup.

        Args:
            secret: TOTP secret
            user_email: User's email address
            issuer: Application name

        Returns:
            Base64-encoded PNG image data URL
        """
        totp = pyotp.TOTP(secret)
        uri = totp.provisioning_uri(name=user_email, issuer_name=issuer)

        qr = qrcode.QRCode(
            version=1,
            error_correction=qrcode.constants.ERROR_CORRECT_L,
            box_size=10,
            border=4,
        )
        qr.add_data(uri)
        qr.make(fit=True)

        img: Image = qr.make_image(fill_color="black", back_color="white")

        buffer = io.BytesIO()
        img.save(buffer, format="PNG")
        img_str = base64.b64encode(buffer.getvalue()).decode()

        return f"data:image/png;base64,{img_str}"

    @staticmethod
    def verify_totp(secret: str, code: str) -> bool:
        """Verify a TOTP code.

        Args:
            secret: TOTP secret
            code: 6-digit TOTP code from user

        Returns:
            True if code is valid
        """
        totp = pyotp.TOTP(secret)
        return totp.verify(code, valid_window=1)

    @staticmethod
    def generate_backup_codes(count: int = 10) -> tuple[list[str], str]:
        """Generate backup codes for account recovery.

        Args:
            count: Number of backup codes to generate

        Returns:
            Tuple of (plain_text_codes, hashed_codes_csv)
            - plain_text_codes: List of codes to show to user (ONCE only)
            - hashed_codes_csv: Comma-separated bcrypt hashes to store in DB

        Security:
            Backup codes are cryptographically hashed using bcrypt before storage.
            This prevents database leaks from exposing usable backup codes.
        """
        plain_codes = [secrets.token_hex(4) for _ in range(count)]
        hashed_codes = [pwd_context.hash(code) for code in plain_codes]
        hashed_codes_csv = ",".join(hashed_codes)
        return (plain_codes, hashed_codes_csv)

    @staticmethod
    def verify_backup_code(hashed_backup_codes: str, code: str) -> bool:
        """Verify a backup code against hashed values.

        Args:
            hashed_backup_codes: Comma-separated bcrypt hashed backup codes
            code: Plain text code to verify

        Returns:
            True if code matches any hashed code

        Security:
            Uses constant-time bcrypt comparison to prevent timing attacks.
        """
        if not hashed_backup_codes or not code:
            return False

        hashed_codes = hashed_backup_codes.split(",")
        for hashed in hashed_codes:
            if hashed and pwd_context.verify(code, hashed):
                return True
        return False

    @staticmethod
    def remove_backup_code(hashed_backup_codes: str, code: str) -> str:
        """Remove a used backup code by finding and removing its hash.

        Args:
            hashed_backup_codes: Comma-separated bcrypt hashed backup codes
            code: Plain text code that was used (to find matching hash)

        Returns:
            Updated comma-separated hashed backup codes (with matching hash removed)

        Security:
            Removes the hash that matches the used code, maintaining bcrypt protection
            for remaining codes.
        """
        if not hashed_backup_codes or not code:
            return hashed_backup_codes

        hashed_codes = hashed_backup_codes.split(",")
        remaining_codes = []

        # Find and remove the matching hash
        found = False
        for hashed in hashed_codes:
            if hashed and not found and pwd_context.verify(code, hashed):
                # Skip this hash (it's the one being removed)
                found = True
                continue
            if hashed:
                remaining_codes.append(hashed)

        return ",".join(remaining_codes)

    @staticmethod
    def get_current_totp(secret: str) -> str:
        """Get current TOTP code (for testing).

        Args:
            secret: TOTP secret

        Returns:
            Current 6-digit TOTP code
        """
        totp = pyotp.TOTP(secret)
        return totp.now()
