'''Add user_id to files table

Revision ID: 20251110_add_user_id
Create Date: 2025-11-10
'''
from alembic import op
import sqlalchemy as sa

revision = '20251110_add_user_id'
down_revision = None  # TODO: Update with your latest migration
branch_labels = None
depends_on = None

def upgrade():
    op.add_column('files', sa.Column('user_id', sa.Integer(), nullable=True))
    op.create_foreign_key('fk_files_user_id', 'files', 'users', ['user_id'], ['id'], ondelete='CASCADE')
    op.create_index('idx_files_user_id', 'files', ['user_id'])
    op.execute("UPDATE files SET user_id = (SELECT id FROM users ORDER BY created_at LIMIT 1) WHERE user_id IS NULL")
    op.alter_column('files', 'user_id', nullable=False)

def downgrade():
    op.drop_index('idx_files_user_id', table_name='files')
    op.drop_constraint('fk_files_user_id', 'files', type_='foreignkey')
    op.drop_column('files', 'user_id')
