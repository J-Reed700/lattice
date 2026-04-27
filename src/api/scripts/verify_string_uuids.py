"""Verify data integrity after downgrade (rollback to String(36) UUIDs)."""

import asyncio
import os
import sys
from pathlib import Path

# Add src to path
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

from sqlalchemy import text
from sqlalchemy.ext.asyncio import create_async_engine

from config.settings import get_settings


async def verify_rollback() -> bool:
    """Verify that downgrade restored String(36) UUIDs correctly."""

    settings = get_settings()
    database_url = os.environ.get("DATABASE_URL", settings.database_url)

    engine = create_async_engine(database_url, echo=False)

    errors = []
    warnings = []

    async with engine.connect() as conn:
        print("🔍 Verifying UUID Rollback (String(36) Restoration)")
        print("=" * 80)
        print()

        # 1. Check that all UUID columns are back to character varying
        print("1. Verifying UUID columns are back to String type...")
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
            elif row.data_type != "character varying":
                errors.append(
                    f"   ❌ {table}.{column} - Expected 'character varying', got '{row.data_type}'"
                )
            elif row.character_maximum_length != 36:
                errors.append(
                    f"   ❌ {table}.{column} - Expected length 36, got {row.character_maximum_length}"
                )
            else:
                print(f"   ✅ {table}.{column} - Type: character varying(36)")

        print()

        # 2. Check foreign key constraints still exist
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

        # 4. Verify UUID format (should be valid UUID strings)
        print("4. Verifying UUID string format...")
        
        # Sample check on a few tables
        sample_tables = ["watch_folders", "files", "text_content", "tags"]
        
        for table in sample_tables:
            result = await conn.execute(
                text(f"""
                SELECT id FROM {table} LIMIT 1
            """)
            )
            row = result.fetchone()
            if row:
                uuid_str = row.id
                # Check format: 8-4-4-4-12 characters
                parts = uuid_str.split("-")
                if len(parts) == 5 and len(parts[0]) == 8 and len(parts[1]) == 4 and len(parts[2]) == 4 and len(parts[3]) == 4 and len(parts[4]) == 12:
                    print(f"   ✅ {table}.id has valid UUID format: {uuid_str}")
                else:
                    errors.append(f"   ❌ {table}.id has invalid UUID format: {uuid_str}")

        print()

        # 5. Sample data verification
        print("5. Verifying sample data integrity...")
        
        # Check that we can still query and join across all tables
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
        print("❌ ROLLBACK VERIFICATION FAILED")
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
        print("⚠️  ROLLBACK VERIFICATION PASSED WITH WARNINGS")
        print()
        print("Warnings:")
        for warning in warnings:
            print(warning)
        return True
    else:
        print("✅ ROLLBACK VERIFICATION PASSED")
        print()
        print("All checks passed! String(36) UUIDs restored successfully.")
        return True


if __name__ == "__main__":
    success = asyncio.run(verify_rollback())
    sys.exit(0 if success else 1)
