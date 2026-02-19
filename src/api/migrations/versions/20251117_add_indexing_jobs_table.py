"""Add indexing jobs table for concurrent job tracking

Revision ID: 20251117_add_indexing_jobs
Revises: 20251111_upgrade_bge_m3_embeddings
Create Date: 2025-11-17

"""
from typing import Sequence, Union

from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision: str = "20251117_add_indexing_jobs"
down_revision: Union[str, None] = "20251111_upgrade_bge_m3_embeddings"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """Create indexing_jobs table for tracking indexing operations."""
    op.create_table(
        "indexing_jobs",
        sa.Column("id", sa.Integer(), nullable=False),
        sa.Column("watch_folder_id", sa.String(length=36), nullable=False),
        sa.Column(
            "status",
            sa.Enum(
                "idle",
                "starting",
                "running",
                "completed",
                "failed",
                "cancelling",
                "cancelled",
                name="jobstatus",
            ),
            nullable=False,
        ),
        sa.Column("total_files", sa.Integer(), nullable=False),
        sa.Column("processed_files", sa.Integer(), nullable=False),
        sa.Column("failed_files", sa.Integer(), nullable=False),
        sa.Column("start_time", sa.DateTime(), nullable=True),
        sa.Column("end_time", sa.DateTime(), nullable=True),
        sa.Column("current_file", sa.String(length=1024), nullable=True),
        sa.Column("error_message", sa.String(length=2048), nullable=True),
        sa.Column("created_at", sa.DateTime(), nullable=False),
        sa.Column("updated_at", sa.DateTime(), nullable=True),
        sa.PrimaryKeyConstraint("id"),
    )

    # Create indexes for efficient querying
    op.create_index(
        op.f("ix_indexing_jobs_id"), "indexing_jobs", ["id"], unique=False
    )
    op.create_index(
        op.f("ix_indexing_jobs_watch_folder_id"),
        "indexing_jobs",
        ["watch_folder_id"],
        unique=False,
    )
    op.create_index(
        op.f("ix_indexing_jobs_status"), "indexing_jobs", ["status"], unique=False
    )


def downgrade() -> None:
    """Drop indexing_jobs table."""
    op.drop_index(op.f("ix_indexing_jobs_status"), table_name="indexing_jobs")
    op.drop_index(op.f("ix_indexing_jobs_watch_folder_id"), table_name="indexing_jobs")
    op.drop_index(op.f("ix_indexing_jobs_id"), table_name="indexing_jobs")
    op.drop_table("indexing_jobs")

    # Drop the enum type
    op.execute("DROP TYPE IF EXISTS jobstatus")
