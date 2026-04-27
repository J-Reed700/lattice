"""Migrate String(36) UUIDs to native PostgreSQL UUID type

Revision ID: 20251120_migrate_uuids
Revises: 20251117_add_user_mfa
Create Date: 2025-11-20

This migration converts all String(36) UUID columns to native PostgreSQL UUID type
for better performance and storage efficiency:
- Reduces storage from 36 bytes to 16 bytes per UUID
- Improves comparison performance
- Reduces index size by ~50%

Tables migrated (in dependency order):
1. watch_folders (id)
2. files (id, watch_folder_id)
3. text_content (id, file_id)
4. text_embeddings (id, text_content_id)
5. images (id, file_id)
6. image_embeddings (id, image_id)
7. tags (id)
8. file_tags (id, file_id, tag_id)
9. search_history (id)
10. devices (device_id)
11. indexing_jobs (watch_folder_id)
"""

from alembic import op
import sqlalchemy as sa
from sqlalchemy.dialects import postgresql

# revision identifiers, used by Alembic.
revision = "20251120_migrate_uuids"
down_revision = "20251117_add_user_mfa"


def upgrade() -> None:
    """Migrate String(36) UUID columns to native PostgreSQL UUID type."""
    
    # Migration order is critical - parent tables before children
    # to avoid foreign key constraint violations
    
    # ==================================================================
    # PHASE 1: Parent tables with no UUID foreign keys
    # ==================================================================
    
    # 1. watch_folders (parent of files, indexing_jobs)
    print("Migrating watch_folders...")
    _migrate_table_uuid_column("watch_folders", "id", is_pk=True)
    
    # 2. tags (parent of file_tags)
    print("Migrating tags...")
    _migrate_table_uuid_column("tags", "id", is_pk=True)
    
    # ==================================================================
    # PHASE 2: files table (child of watch_folders, parent of many)
    # ==================================================================
    
    print("Migrating files...")
    # Drop FK constraints first
    op.drop_constraint("files_watch_folder_id_fkey", "files", type_="foreignkey")
    
    # Migrate columns
    _migrate_table_uuid_column("files", "id", is_pk=True)
    _migrate_table_uuid_column("files", "watch_folder_id", is_pk=False)
    
    # Recreate FK
    op.create_foreign_key(
        "files_watch_folder_id_fkey",
        "files", "watch_folders",
        ["watch_folder_id"], ["id"],
        ondelete="CASCADE"
    )
    
    # ==================================================================
    # PHASE 3: text_content (child of files, parent of text_embeddings)
    # ==================================================================
    
    print("Migrating text_content...")
    op.drop_constraint("text_content_file_id_fkey", "text_content", type_="foreignkey")
    
    _migrate_table_uuid_column("text_content", "id", is_pk=True)
    _migrate_table_uuid_column("text_content", "file_id", is_pk=False)
    
    op.create_foreign_key(
        "text_content_file_id_fkey",
        "text_content", "files",
        ["file_id"], ["id"],
        ondelete="CASCADE"
    )
    
    # ==================================================================
    # PHASE 4: text_embeddings (child of text_content)
    # ==================================================================
    
    print("Migrating text_embeddings...")
    op.drop_constraint("text_embeddings_text_content_id_fkey", "text_embeddings", type_="foreignkey")
    
    _migrate_table_uuid_column("text_embeddings", "id", is_pk=True)
    _migrate_table_uuid_column("text_embeddings", "text_content_id", is_pk=False)
    
    op.create_foreign_key(
        "text_embeddings_text_content_id_fkey",
        "text_embeddings", "text_content",
        ["text_content_id"], ["id"],
        ondelete="CASCADE"
    )
    
    # ==================================================================
    # PHASE 5: images (child of files, parent of image_embeddings)
    # ==================================================================
    
    print("Migrating images...")
    op.drop_constraint("images_file_id_fkey", "images", type_="foreignkey")
    
    _migrate_table_uuid_column("images", "id", is_pk=True)
    _migrate_table_uuid_column("images", "file_id", is_pk=False)
    
    op.create_foreign_key(
        "images_file_id_fkey",
        "images", "files",
        ["file_id"], ["id"],
        ondelete="CASCADE"
    )
    
    # ==================================================================
    # PHASE 6: image_embeddings (child of images)
    # ==================================================================
    
    print("Migrating image_embeddings...")
    op.drop_constraint("image_embeddings_image_id_fkey", "image_embeddings", type_="foreignkey")
    
    _migrate_table_uuid_column("image_embeddings", "id", is_pk=True)
    _migrate_table_uuid_column("image_embeddings", "image_id", is_pk=False)
    
    op.create_foreign_key(
        "image_embeddings_image_id_fkey",
        "image_embeddings", "images",
        ["image_id"], ["id"],
        ondelete="CASCADE"
    )
    
    # ==================================================================
    # PHASE 7: file_tags (child of files and tags)
    # ==================================================================
    
    print("Migrating file_tags...")
    op.drop_constraint("file_tags_file_id_fkey", "file_tags", type_="foreignkey")
    op.drop_constraint("file_tags_tag_id_fkey", "file_tags", type_="foreignkey")
    
    _migrate_table_uuid_column("file_tags", "id", is_pk=True)
    _migrate_table_uuid_column("file_tags", "file_id", is_pk=False)
    _migrate_table_uuid_column("file_tags", "tag_id", is_pk=False)
    
    op.create_foreign_key(
        "file_tags_file_id_fkey",
        "file_tags", "files",
        ["file_id"], ["id"],
        ondelete="CASCADE"
    )
    op.create_foreign_key(
        "file_tags_tag_id_fkey",
        "file_tags", "tags",
        ["tag_id"], ["id"],
        ondelete="CASCADE"
    )
    
    # ==================================================================
    # PHASE 8: search_history (independent table)
    # ==================================================================
    
    print("Migrating search_history...")
    _migrate_table_uuid_column("search_history", "id", is_pk=True)
    
    # ==================================================================
    # PHASE 9: devices (device_id is UNIQUE, not PK)
    # ==================================================================
    
    print("Migrating devices...")
    _migrate_table_uuid_column("devices", "device_id", is_pk=False, is_unique=True)
    
    # ==================================================================
    # PHASE 10: indexing_jobs (child of watch_folders)
    # ==================================================================
    
    print("Migrating indexing_jobs...")
    # Note: watch_folder_id has no explicit FK constraint in model, but may exist in DB
    try:
        op.drop_constraint("indexing_jobs_watch_folder_id_fkey", "indexing_jobs", type_="foreignkey")
    except Exception:
        # FK might not exist
        pass
    
    _migrate_table_uuid_column("indexing_jobs", "watch_folder_id", is_pk=False)
    
    print("UUID migration complete!")


def downgrade() -> None:
    """Revert UUID columns back to String(36)."""
    
    print("Reverting UUID migration...")
    
    # Reverse order: children before parents
    
    # 10. indexing_jobs
    print("Reverting indexing_jobs...")
    _revert_table_uuid_column("indexing_jobs", "watch_folder_id", is_pk=False)
    
    # 9. devices
    print("Reverting devices...")
    _revert_table_uuid_column("devices", "device_id", is_pk=False, is_unique=True)
    
    # 8. search_history
    print("Reverting search_history...")
    _revert_table_uuid_column("search_history", "id", is_pk=True)
    
    # 7. file_tags
    print("Reverting file_tags...")
    op.drop_constraint("file_tags_file_id_fkey", "file_tags", type_="foreignkey")
    op.drop_constraint("file_tags_tag_id_fkey", "file_tags", type_="foreignkey")
    
    _revert_table_uuid_column("file_tags", "id", is_pk=True)
    _revert_table_uuid_column("file_tags", "file_id", is_pk=False)
    _revert_table_uuid_column("file_tags", "tag_id", is_pk=False)
    
    op.create_foreign_key(
        "file_tags_file_id_fkey",
        "file_tags", "files",
        ["file_id"], ["id"],
        ondelete="CASCADE"
    )
    op.create_foreign_key(
        "file_tags_tag_id_fkey",
        "file_tags", "tags",
        ["tag_id"], ["id"],
        ondelete="CASCADE"
    )
    
    # 6. image_embeddings
    print("Reverting image_embeddings...")
    op.drop_constraint("image_embeddings_image_id_fkey", "image_embeddings", type_="foreignkey")
    
    _revert_table_uuid_column("image_embeddings", "id", is_pk=True)
    _revert_table_uuid_column("image_embeddings", "image_id", is_pk=False)
    
    op.create_foreign_key(
        "image_embeddings_image_id_fkey",
        "image_embeddings", "images",
        ["image_id"], ["id"],
        ondelete="CASCADE"
    )
    
    # 5. images
    print("Reverting images...")
    op.drop_constraint("images_file_id_fkey", "images", type_="foreignkey")
    
    _revert_table_uuid_column("images", "id", is_pk=True)
    _revert_table_uuid_column("images", "file_id", is_pk=False)
    
    op.create_foreign_key(
        "images_file_id_fkey",
        "images", "files",
        ["file_id"], ["id"],
        ondelete="CASCADE"
    )
    
    # 4. text_embeddings
    print("Reverting text_embeddings...")
    op.drop_constraint("text_embeddings_text_content_id_fkey", "text_embeddings", type_="foreignkey")
    
    _revert_table_uuid_column("text_embeddings", "id", is_pk=True)
    _revert_table_uuid_column("text_embeddings", "text_content_id", is_pk=False)
    
    op.create_foreign_key(
        "text_embeddings_text_content_id_fkey",
        "text_embeddings", "text_content",
        ["text_content_id"], ["id"],
        ondelete="CASCADE"
    )
    
    # 3. text_content
    print("Reverting text_content...")
    op.drop_constraint("text_content_file_id_fkey", "text_content", type_="foreignkey")
    
    _revert_table_uuid_column("text_content", "id", is_pk=True)
    _revert_table_uuid_column("text_content", "file_id", is_pk=False)
    
    op.create_foreign_key(
        "text_content_file_id_fkey",
        "text_content", "files",
        ["file_id"], ["id"],
        ondelete="CASCADE"
    )
    
    # 2. files
    print("Reverting files...")
    op.drop_constraint("files_watch_folder_id_fkey", "files", type_="foreignkey")
    
    _revert_table_uuid_column("files", "id", is_pk=True)
    _revert_table_uuid_column("files", "watch_folder_id", is_pk=False)
    
    op.create_foreign_key(
        "files_watch_folder_id_fkey",
        "files", "watch_folders",
        ["watch_folder_id"], ["id"],
        ondelete="CASCADE"
    )
    
    # 1. tags
    print("Reverting tags...")
    _revert_table_uuid_column("tags", "id", is_pk=True)
    
    # 0. watch_folders
    print("Reverting watch_folders...")
    _revert_table_uuid_column("watch_folders", "id", is_pk=True)
    
    print("UUID reversion complete!")


# ==================================================================
# Helper functions for column migration
# ==================================================================

def _migrate_table_uuid_column(
    table_name: str,
    column_name: str,
    is_pk: bool = False,
    is_unique: bool = False
) -> None:
    """Migrate a single UUID column from String(36) to native UUID.
    
    Args:
        table_name: Name of the table
        column_name: Name of the column to migrate
        is_pk: Whether this column is a primary key
        is_unique: Whether this column has a unique constraint
    """
    temp_column = f"{column_name}_uuid_temp"
    
    # 1. Add temporary UUID column
    op.add_column(
        table_name,
        sa.Column(temp_column, postgresql.UUID(as_uuid=True), nullable=True)
    )
    
    # 2. Copy and cast data from String to UUID
    op.execute(f"""
        UPDATE {table_name}
        SET {temp_column} = {column_name}::uuid
        WHERE {column_name} IS NOT NULL
    """)
    
    # 3. Drop primary key constraint if applicable
    if is_pk:
        op.drop_constraint(f"{table_name}_pkey", table_name, type_="primary")
    
    # 4. Drop unique constraint if applicable
    if is_unique:
        # Find and drop unique constraint
        op.execute(f"""
            DO $$
            DECLARE
                constraint_name text;
            BEGIN
                SELECT tc.constraint_name INTO constraint_name
                FROM information_schema.table_constraints tc
                WHERE tc.table_name = '{table_name}'
                  AND tc.constraint_type = 'UNIQUE'
                  AND EXISTS (
                      SELECT 1 FROM information_schema.key_column_usage kcu
                      WHERE kcu.constraint_name = tc.constraint_name
                        AND kcu.column_name = '{column_name}'
                  );
                
                IF constraint_name IS NOT NULL THEN
                    EXECUTE 'ALTER TABLE {table_name} DROP CONSTRAINT ' || constraint_name;
                END IF;
            END $$;
        """)
    
    # 5. Drop old String column (and its index)
    op.drop_index(f"ix_{table_name}_{column_name}", table_name=table_name)
    op.drop_column(table_name, column_name)
    
    # 6. Rename temp column to original name
    op.alter_column(table_name, temp_column, new_column_name=column_name)
    
    # 7. Set NOT NULL constraint
    op.alter_column(table_name, column_name, nullable=False)
    
    # 8. Recreate primary key if applicable
    if is_pk:
        op.create_primary_key(f"{table_name}_pkey", table_name, [column_name])
    
    # 9. Recreate unique constraint if applicable
    if is_unique:
        op.create_unique_constraint(f"uq_{table_name}_{column_name}", table_name, [column_name])
    
    # 10. Recreate index
    op.create_index(f"ix_{table_name}_{column_name}", table_name, [column_name], unique=False)


def _revert_table_uuid_column(
    table_name: str,
    column_name: str,
    is_pk: bool = False,
    is_unique: bool = False
) -> None:
    """Revert a UUID column back to String(36).
    
    Args:
        table_name: Name of the table
        column_name: Name of the column to revert
        is_pk: Whether this column is a primary key
        is_unique: Whether this column has a unique constraint
    """
    temp_column = f"{column_name}_string_temp"
    
    # 1. Add temporary String column
    op.add_column(
        table_name,
        sa.Column(temp_column, sa.String(36), nullable=True)
    )
    
    # 2. Copy and cast data from UUID to String
    op.execute(f"""
        UPDATE {table_name}
        SET {temp_column} = {column_name}::text
        WHERE {column_name} IS NOT NULL
    """)
    
    # 3. Drop primary key constraint if applicable
    if is_pk:
        op.drop_constraint(f"{table_name}_pkey", table_name, type_="primary")
    
    # 4. Drop unique constraint if applicable
    if is_unique:
        op.drop_constraint(f"uq_{table_name}_{column_name}", table_name, type_="unique")
    
    # 5. Drop old UUID column (and its index)
    op.drop_index(f"ix_{table_name}_{column_name}", table_name=table_name)
    op.drop_column(table_name, column_name)
    
    # 6. Rename temp column to original name
    op.alter_column(table_name, temp_column, new_column_name=column_name)
    
    # 7. Set NOT NULL constraint
    op.alter_column(table_name, column_name, nullable=False)
    
    # 8. Recreate primary key if applicable
    if is_pk:
        op.create_primary_key(f"{table_name}_pkey", table_name, [column_name])
    
    # 9. Recreate unique constraint if applicable
    if is_unique:
        op.create_unique_constraint(None, table_name, [column_name])
    
    # 10. Recreate index
    op.create_index(f"ix_{table_name}_{column_name}", table_name, [column_name], unique=False)
