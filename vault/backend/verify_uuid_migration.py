"""Verification script for UUID migration.

Run this after migration to verify data integrity and UUID type correctness.
"""

import asyncio
import sys
from uuid import UUID

from sqlalchemy import select, text
from sqlalchemy.ext.asyncio import AsyncSession

from src.db.session import async_session_maker
from src.models.file import File
from src.models.watch import WatchFolder
from src.models.tag import Tag
from src.models.text_content import TextContent
from src.models.embedding import TextEmbedding, ImageEmbedding
from src.models.image import Image
from src.models.search_history import SearchHistory
from src.models.sync import Device


async def verify_table_counts(session: AsyncSession) -> dict[str, int]:
    """Get row counts for all migrated tables."""
    tables = [
        "files",
        "watch_folders",
        "tags",
        "file_tags",
        "text_content",
        "text_embeddings",
        "images",
        "image_embeddings",
        "search_history",
        "devices",
        "indexing_jobs",
    ]
    
    counts = {}
    for table in tables:
        result = await session.execute(text(f"SELECT COUNT(*) FROM {table}"))
        counts[table] = result.scalar()
    
    return counts


async def verify_uuid_types(session: AsyncSession) -> list[str]:
    """Verify that UUID columns have correct PostgreSQL type."""
    errors = []
    
    uuid_columns = [
        ("files", "id"),
        ("files", "watch_folder_id"),
        ("watch_folders", "id"),
        ("tags", "id"),
        ("file_tags", "id"),
        ("file_tags", "file_id"),
        ("file_tags", "tag_id"),
        ("text_content", "id"),
        ("text_content", "file_id"),
        ("text_embeddings", "id"),
        ("text_embeddings", "text_content_id"),
        ("images", "id"),
        ("images", "file_id"),
        ("image_embeddings", "id"),
        ("image_embeddings", "image_id"),
        ("search_history", "id"),
        ("devices", "device_id"),
        ("indexing_jobs", "watch_folder_id"),
    ]
    
    for table, column in uuid_columns:
        result = await session.execute(text(f"""
            SELECT data_type 
            FROM information_schema.columns 
            WHERE table_name = '{table}' AND column_name = '{column}'
        """))
        data_type = result.scalar()
        
        if data_type != "uuid":
            errors.append(f"{table}.{column}: Expected 'uuid', got '{data_type}'")
    
    return errors


async def verify_sample_data(session: AsyncSession) -> list[str]:
    """Verify sample records can be queried and have UUID types."""
    errors = []
    
    try:
        # Test File model
        result = await session.execute(select(File).limit(1))
        file = result.scalar_one_or_none()
        if file:
            if not isinstance(file.id, UUID):
                errors.append(f"File.id is {type(file.id)}, expected UUID")
            if not isinstance(file.watch_folder_id, UUID):
                errors.append(f"File.watch_folder_id is {type(file.watch_folder_id)}, expected UUID")
        
        # Test WatchFolder model
        result = await session.execute(select(WatchFolder).limit(1))
        watch_folder = result.scalar_one_or_none()
        if watch_folder:
            if not isinstance(watch_folder.id, UUID):
                errors.append(f"WatchFolder.id is {type(watch_folder.id)}, expected UUID")
        
        # Test Tag model
        result = await session.execute(select(Tag).limit(1))
        tag = result.scalar_one_or_none()
        if tag:
            if not isinstance(tag.id, UUID):
                errors.append(f"Tag.id is {type(tag.id)}, expected UUID")
        
        # Test TextContent model
        result = await session.execute(select(TextContent).limit(1))
        text_content = result.scalar_one_or_none()
        if text_content:
            if not isinstance(text_content.id, UUID):
                errors.append(f"TextContent.id is {type(text_content.id)}, expected UUID")
            if not isinstance(text_content.file_id, UUID):
                errors.append(f"TextContent.file_id is {type(text_content.file_id)}, expected UUID")
        
        # Test Device model
        result = await session.execute(select(Device).limit(1))
        device = result.scalar_one_or_none()
        if device:
            if not isinstance(device.device_id, UUID):
                errors.append(f"Device.device_id is {type(device.device_id)}, expected UUID")
        
    except Exception as e:
        errors.append(f"Error querying models: {e}")
    
    return errors


async def verify_foreign_keys(session: AsyncSession) -> list[str]:
    """Verify foreign key constraints are properly recreated."""
    errors = []
    
    result = await session.execute(text("""
        SELECT
            tc.table_name,
            kcu.column_name,
            ccu.table_name AS foreign_table_name,
            ccu.column_name AS foreign_column_name
        FROM information_schema.table_constraints AS tc
        JOIN information_schema.key_column_usage AS kcu
          ON tc.constraint_name = kcu.constraint_name
        JOIN information_schema.constraint_column_usage AS ccu
          ON ccu.constraint_name = tc.constraint_name
        WHERE tc.constraint_type = 'FOREIGN KEY'
          AND tc.table_name IN (
              'files', 'text_content', 'text_embeddings', 
              'images', 'image_embeddings', 'file_tags', 'indexing_jobs'
          )
        ORDER BY tc.table_name, kcu.column_name
    """))
    
    fks = result.fetchall()
    
    expected_fks = [
        ("files", "watch_folder_id", "watch_folders", "id"),
        ("text_content", "file_id", "files", "id"),
        ("text_embeddings", "text_content_id", "text_content", "id"),
        ("images", "file_id", "files", "id"),
        ("image_embeddings", "image_id", "images", "id"),
        ("file_tags", "file_id", "files", "id"),
        ("file_tags", "tag_id", "tags", "id"),
    ]
    
    actual_fks = [(fk[0], fk[1], fk[2], fk[3]) for fk in fks]
    
    for expected_fk in expected_fks:
        if expected_fk not in actual_fks:
            errors.append(f"Missing FK: {expected_fk[0]}.{expected_fk[1]} -> {expected_fk[2]}.{expected_fk[3]}")
    
    return errors


async def main() -> int:
    """Run all verification checks."""
    print("=" * 80)
    print("UUID Migration Verification")
    print("=" * 80)
    print()
    
    async with async_session_maker() as session:
        # 1. Verify table counts
        print("📊 Checking table counts...")
        counts = await verify_table_counts(session)
        for table, count in counts.items():
            print(f"  {table:20s}: {count:>6,} rows")
        print()
        
        # 2. Verify UUID data types in schema
        print("🔍 Verifying UUID column types...")
        type_errors = await verify_uuid_types(session)
        if type_errors:
            print("  ❌ Type errors found:")
            for error in type_errors:
                print(f"     - {error}")
            print()
        else:
            print("  ✅ All columns have correct UUID type")
            print()
        
        # 3. Verify sample data
        print("📝 Checking sample data...")
        data_errors = await verify_sample_data(session)
        if data_errors:
            print("  ❌ Data errors found:")
            for error in data_errors:
                print(f"     - {error}")
            print()
        else:
            print("  ✅ Sample data has correct UUID Python types")
            print()
        
        # 4. Verify foreign keys
        print("🔗 Verifying foreign key constraints...")
        fk_errors = await verify_foreign_keys(session)
        if fk_errors:
            print("  ❌ Foreign key errors found:")
            for error in fk_errors:
                print(f"     - {error}")
            print()
        else:
            print("  ✅ All foreign keys properly recreated")
            print()
    
    # Summary
    print("=" * 80)
    all_errors = type_errors + data_errors + fk_errors
    if all_errors:
        print("❌ VERIFICATION FAILED")
        print(f"   Found {len(all_errors)} error(s)")
        print()
        print("Please review errors above and fix before proceeding.")
        return 1
    else:
        print("✅ VERIFICATION PASSED")
        print()
        print("Migration successful! All UUID columns migrated correctly.")
        print()
        print("Next steps:")
        print("  1. Run test suite: poetry run pytest")
        print("  2. Check API responses: curl http://localhost:8000/api/v1/files")
        print("  3. Monitor logs for UUID-related errors")
        return 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
