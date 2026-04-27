"""Verify data integrity after UUID migration."""

import asyncio
import os
import sys
from pathlib import Path

# Add src to path
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

from sqlalchemy import text
from sqlalchemy.ext.asyncio import create_async_engine

from config.settings import get_settings


async def verify_migration() -> bool:
    """Verify that UUID migration completed successfully."""

    settings = get_settings()
    database_url = os.environ.get("DATABASE_URL", settings.database_url)

    engine = create_async_engine(database_url, echo=False)

    errors = []
    warnings = []

    async with engine.connect() as conn:
        print("🔍 Verifying UUID Migration")
        print("=" * 80)
        print()

        # 1. Check that all UUID columns are now native UUID type
        print("1. Verifying UUID column types...")
        uuid_columns = [
            ("watch_folders", "id"),
            ("files", "id"),
            ("files", "watch_folder_id"),
            ("text_content", "id"),
            ("text_content", "file_id"),
            ("text_embeddings", "id"),
            ("text_embeddings", "text_content_id"),
            ("images", "id"),
            ("images", "file_id"),
            ("image_embeddings", "id"),
            ("image_embeddings", "image_id"),
            ("tags", "id"),
            ("file_tags", "id"),
            ("file_tags", "file_id"),
            ("file_tags", "tag_id"),
            ("search_history", "id"),
        ]

        for table, column in uuid_columns:
            result = await conn.execute(
                text(f"""
                SELECT data_type, character_maximum_length
                FROM information_schema.columns
                WHERE table_name = '{table}' AND column_name = '{column}'
            """)
            )
            row = result.fetchone()
            if not row:
                errors.append(f"   ❌ {table}.{column} - Column not found")
            elif row.data_type != "uuid":
                errors.append(
                    f"   ❌ {table}.{column} - Expected 'uuid', got '{row.data_type}'"
                )
            else:
                print(f"   ✅ {table}.{column} - Type: uuid")

        print()

        # 2. Check foreign key constraints
        print("2. Verifying foreign key constraints...")
        fk_checks = [
            ("files", "files_watch_folder_id_fkey", "watch_folder_id", "watch_folders", "id"),
            ("text_content", "text_content_file_id_fkey", "file_id", "files", "id"),
            ("text_embeddings", "text_embeddings_text_content_id_fkey", "text_content_id", "text_content", "id"),
            ("images", "images_file_id_fkey", "file_id", "files", "id"),
            ("image_embeddings", "image_embeddings_image_id_fkey", "image_id", "images", "id"),
            ("file_tags", "file_tags_file_id_fkey", "file_id", "files", "id"),
            ("file_tags", "file_tags_tag_id_fkey", "tag_id", "tags", "id"),
        ]

        for table, constraint, col, ref_table, ref_col in fk_checks:
            result = await conn.execute(
                text(f"""
                SELECT 1
                FROM information_schema.table_constraints tc
                WHERE tc.table_name = '{table}'
                  AND tc.constraint_name = '{constraint}'
                  AND tc.constraint_type = 'FOREIGN KEY'
            """)
            )
            if result.fetchone():
                print(f"   ✅ {constraint} exists on {table}.{col} -> {ref_table}.{ref_col}")
            else:
                errors.append(f"   ❌ {constraint} missing on {table}")

        print()

        # 3. Check for orphaned records
        print("3. Checking for orphaned records...")

        orphan_checks = [
            ("files", "watch_folder_id", "watch_folders", "id"),
            ("text_content", "file_id", "files", "id"),
            ("text_embeddings", "text_content_id", "text_content", "id"),
            ("images", "file_id", "files", "id"),
            ("image_embeddings", "image_id", "images", "id"),
            ("file_tags", "file_id", "files", "id"),
            ("file_tags", "tag_id", "tags", "id"),
        ]

        for child_table, child_col, parent_table, parent_col in orphan_checks:
            result = await conn.execute(
                text(f"""
                SELECT COUNT(*)
                FROM {child_table} c
                LEFT JOIN {parent_table} p ON c.{child_col} = p.{parent_col}
                WHERE p.{parent_col} IS NULL AND c.{child_col} IS NOT NULL
            """)
            )
            count = result.scalar()
            if count > 0:
                errors.append(
                    f"   ❌ {child_table}.{child_col} has {count} orphaned records"
                )
            else:
                print(f"   ✅ {child_table}.{child_col} - No orphaned records")

        print()

        # 4. Check primary keys
        print("4. Verifying primary keys...")
        pk_checks = [
            ("watch_folders", "watch_folders_pkey", "id"),
            ("files", "files_pkey", "id"),
            ("text_content", "text_content_pkey", "id"),
            ("text_embeddings", "text_embeddings_pkey", "id"),
            ("images", "images_pkey", "id"),
            ("image_embeddings", "image_embeddings_pkey", "id"),
            ("tags", "tags_pkey", "id"),
            ("file_tags", "file_tags_pkey", "id"),
            ("search_history", "search_history_pkey", "id"),
        ]

        for table, constraint, col in pk_checks:
            result = await conn.execute(
                text(f"""
                SELECT 1
                FROM information_schema.table_constraints tc
                WHERE tc.table_name = '{table}'
                  AND tc.constraint_name = '{constraint}'
                  AND tc.constraint_type = 'PRIMARY KEY'
            """)
            )
            if result.fetchone():
                print(f"   ✅ {constraint} exists on {table}.{col}")
            else:
                errors.append(f"   ❌ {constraint} missing on {table}")

        print()

        # 5. Check indexes
        print("5. Verifying indexes...")
        index_checks = [
            ("ix_watch_folders_id", "watch_folders"),
            ("ix_files_id", "files"),
            ("ix_text_content_id", "text_content"),
            ("ix_text_embeddings_id", "text_embeddings"),
            ("ix_images_id", "images"),
            ("ix_image_embeddings_id", "image_embeddings"),
            ("ix_tags_id", "tags"),
            ("ix_file_tags_id", "file_tags"),
            ("ix_search_history_id", "search_history"),
        ]

        for index, table in index_checks:
            result = await conn.execute(
                text(f"""
                SELECT 1
                FROM pg_indexes
                WHERE indexname = '{index}' AND tablename = '{table}'
            """)
            )
            if result.fetchone():
                print(f"   ✅ {index} exists on {table}")
            else:
                warnings.append(f"   ⚠️  {index} missing on {table}")

        print()

        # 6. Sample data verification
        print("6. Verifying sample data integrity...")
        
        # Check that we can query and join across all tables
        result = await conn.execute(
            text("""
            SELECT COUNT(*)
            FROM files f
            JOIN watch_folders wf ON f.watch_folder_id = wf.id
            JOIN text_content tc ON tc.file_id = f.id
            JOIN text_embeddings te ON te.text_content_id = tc.id
        """)
        )
        count = result.scalar()
        print(f"   ✅ Successfully joined files -> watch_folders -> text_content -> text_embeddings: {count} records")

        # Check file_tags join
        result = await conn.execute(
            text("""
            SELECT COUNT(*)
            FROM file_tags ft
            JOIN files f ON ft.file_id = f.id
            JOIN tags t ON ft.tag_id = t.id
        """)
        )
        count = result.scalar()
        print(f"   ✅ Successfully joined file_tags -> files -> tags: {count} records")

        # Check images join
        result = await conn.execute(
            text("""
            SELECT COUNT(*)
            FROM images i
            JOIN files f ON i.file_id = f.id
            JOIN image_embeddings ie ON ie.image_id = i.id
        """)
        )
        count = result.scalar()
        print(f"   ✅ Successfully joined images -> files -> image_embeddings: {count} records")

    await engine.dispose()

    # Summary
    print()
    print("=" * 80)
    print()

    if errors:
        print("❌ VERIFICATION FAILED")
        print()
        print("Errors:")
        for error in errors:
            print(error)
        if warnings:
            print()
            print("Warnings:")
            for warning in warnings:
                print(warning)
        return False
    elif warnings:
        print("⚠️  VERIFICATION PASSED WITH WARNINGS")
        print()
        print("Warnings:")
        for warning in warnings:
            print(warning)
        return True
    else:
        print("✅ VERIFICATION PASSED")
        print()
        print("All checks passed successfully!")
        return True


if __name__ == "__main__":
    success = asyncio.run(verify_migration())
    sys.exit(0 if success else 1)
