"""User authentication models."""

from __future__ import annotations

from datetime import UTC, datetime
from typing import TYPE_CHECKING

from sqlalchemy import Boolean, DateTime, ForeignKey, String, Text
from sqlalchemy.orm import Mapped, mapped_column, relationship

from src.db import Base

if TYPE_CHECKING:
    from src.models import File, WatchFolder


class User(Base):
    """User model for authentication.

    Stores user credentials and metadata for system access control.

    Attributes:
        id: Primary key
        username: Unique username for login
        email: Unique email address
        hashed_password: Bcrypt hashed password (never store plaintext!)
        is_active: Whether account is active (for soft deletion)
        is_superuser: Whether user has admin privileges
        created_at: Account creation timestamp
        updated_at: Last update timestamp
    """

    __tablename__ = "users"

    id: Mapped[int] = mapped_column(primary_key=True, index=True)
    username: Mapped[str] = mapped_column(String(50), unique=True, index=True, nullable=False)
    email: Mapped[str] = mapped_column(String(255), unique=True, index=True, nullable=False)
    hashed_password: Mapped[str] = mapped_column(String(255), nullable=False)
    is_active: Mapped[bool] = mapped_column(Boolean, default=True, nullable=False)
    is_superuser: Mapped[bool] = mapped_column(Boolean, default=False, nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime, default=lambda: datetime.now(UTC), nullable=False
    )
    updated_at: Mapped[datetime | None] = mapped_column(
        DateTime,
        default=lambda: datetime.now(UTC),
        onupdate=lambda: datetime.now(UTC),
        nullable=True,
    )

    # Relationships
    files: Mapped[list[File]] = relationship(
        "File", back_populates="user", cascade="all, delete-orphan"
    )
    watch_folders: Mapped[list[WatchFolder]] = relationship(
        "WatchFolder", back_populates="user", cascade="all, delete-orphan"
    )
    mfa: Mapped[UserMFA | None] = relationship(
        "UserMFA", back_populates="user", uselist=False, cascade="all, delete-orphan"
    )

    def __repr__(self) -> str:
        return f"<User(id={self.id}, username='{self.username}', email='{self.email}')>"


class RefreshToken(Base):
    """Refresh token model for token rotation.

    Stores refresh tokens to enable secure token rotation without re-authentication.

    Attributes:
        id: Primary key
        user_id: Foreign key to user
        token: Unique refresh token string
        expires_at: Expiration timestamp
        revoked: Whether token has been revoked
        created_at: Token creation timestamp
        user: Relationship to User model
    """

    __tablename__ = "refresh_tokens"

    id: Mapped[int] = mapped_column(primary_key=True, index=True)
    user_id: Mapped[int] = mapped_column(
        ForeignKey("users.id", ondelete="CASCADE"), nullable=False, index=True
    )
    token: Mapped[str] = mapped_column(String(500), unique=True, index=True, nullable=False)
    expires_at: Mapped[datetime] = mapped_column(DateTime, nullable=False)
    revoked: Mapped[bool] = mapped_column(Boolean, default=False, nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime, default=lambda: datetime.now(UTC), nullable=False
    )

    # Relationship to User
    user: Mapped[User] = relationship("User", backref="refresh_tokens")

    def __repr__(self) -> str:
        return f"<RefreshToken(id={self.id}, user_id={self.user_id}, revoked={self.revoked})>"

    def is_valid(self) -> bool:
        """Check if token is still valid (not expired and not revoked)."""
        return not self.revoked and datetime.now(UTC) < self.expires_at


class UserMFA(Base):
    """Multi-factor authentication settings for users.

    Stores TOTP secret and backup codes for enhanced account security.

    Attributes:
        id: Primary key
        user_id: Foreign key to user (one-to-one relationship)
        is_enabled: Whether MFA is currently active
        secret: Base32-encoded TOTP secret
        backup_codes: Comma-separated backup codes for account recovery
        created_at: When MFA was first set up
        verified_at: When MFA was successfully verified and enabled
        user: Relationship to User model
    """

    __tablename__ = "user_mfa"

    id: Mapped[int] = mapped_column(primary_key=True)
    user_id: Mapped[int] = mapped_column(
        ForeignKey("users.id", ondelete="CASCADE"), unique=True, nullable=False, index=True
    )
    is_enabled: Mapped[bool] = mapped_column(Boolean, default=False, nullable=False)
    secret: Mapped[str] = mapped_column(String(32), nullable=False)
    backup_codes: Mapped[str] = mapped_column(Text, nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime, default=lambda: datetime.now(UTC), nullable=False
    )
    verified_at: Mapped[datetime | None] = mapped_column(DateTime, nullable=True)

    # Relationships
    user: Mapped[User] = relationship("User", back_populates="mfa")

    def __repr__(self) -> str:
        return f"<UserMFA(id={self.id}, user_id={self.user_id}, is_enabled={self.is_enabled})>"
