"""Fix unique constraint to allow path reuse after deletion

Revision ID: 004
Revises: 003_add_sync_state_and_soft_delete
Create Date: 2025-11-15

Changes:
- Drop old unique constraint uq_document_user_path
- Create partial unique index that only applies to active (non-deleted) documents
- This allows users to reuse paths immediately after deletion without waiting for cleanup
"""
from typing import Sequence, Union

from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision: str = '004'
down_revision: Union[str, None] = '003_add_sync_state_and_soft_delete'
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """Upgrade database schema.

    Replace full unique constraint with partial unique index.
    The partial index only enforces uniqueness for active documents (WHERE deleted_at IS NULL).
    This allows users to immediately reuse paths after deleting a document.
    """
    # Drop the old unique constraint
    op.drop_constraint('uq_document_user_path', 'documents', type_='unique')

    # Create partial unique index (PostgreSQL 9.2+)
    # Only enforces uniqueness when deleted_at IS NULL
    op.create_index(
        'uq_document_user_path_active',
        'documents',
        ['user_id', 'path'],
        unique=True,
        postgresql_where=sa.text('deleted_at IS NULL')
    )


def downgrade() -> None:
    """Downgrade database schema.

    NOTE: This downgrade may fail if multiple soft-deleted documents exist with the same path.
    Clean up soft-deleted documents before downgrading.
    """
    # Drop the partial unique index
    op.drop_index('uq_document_user_path_active', table_name='documents')

    # Recreate the old unique constraint
    # WARNING: This will fail if multiple deleted documents have the same path
    op.create_unique_constraint('uq_document_user_path', 'documents', ['user_id', 'path'])
