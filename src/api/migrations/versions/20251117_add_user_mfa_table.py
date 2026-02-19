"""Add UserMFA table for multi-factor authentication

Revision ID: add_user_mfa
Revises:
Create Date: 2025-11-17

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = "add_user_mfa"
down_revision = None


def upgrade() -> None:
    """Add UserMFA table for TOTP-based multi-factor authentication."""
    op.create_table(
        "user_mfa",
        sa.Column("id", sa.Integer(), nullable=False),
        sa.Column("user_id", sa.Integer(), nullable=False),
        sa.Column("is_enabled", sa.Boolean(), nullable=False, server_default="false"),
        sa.Column("secret", sa.String(length=32), nullable=False),
        sa.Column("backup_codes", sa.Text(), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False, server_default=sa.text("CURRENT_TIMESTAMP")),
        sa.Column("verified_at", sa.DateTime(), nullable=True),
        sa.PrimaryKeyConstraint("id"),
        sa.ForeignKeyConstraint(["user_id"], ["users.id"], ondelete="CASCADE"),
        sa.UniqueConstraint("user_id"),
    )
    op.create_index(op.f("ix_user_mfa_user_id"), "user_mfa", ["user_id"], unique=True)


def downgrade() -> None:
    """Remove UserMFA table."""
    op.drop_index(op.f("ix_user_mfa_user_id"), table_name="user_mfa")
    op.drop_table("user_mfa")
