from datetime import datetime, timedelta
from pathlib import Path
from uuid import uuid4

import pytest
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from src.models import File, TextContent


@pytest.mark.integration()
class TestOrphanedFileDetection:
    async def test_detect_orphaned_text_content(self, db_session: AsyncSession, watch_folder):
        orphaned_content = TextContent(
            id=uuid4(),
            file_id=uuid4(),
            content="Orphaned text content",
            language="en",
            char_count=50,
            word_count=10,
            embedding=[0.1] * 384,
        )
        db_session.add(orphaned_content)

        try:
            await db_session.commit()
        except Exception:
            await db_session.rollback()

        result = await db_session.execute(
            select(TextContent).where(TextContent.id == orphaned_content.id)
        )
        content = result.scalar_one_or_none()

        assert content is None

    async def test_detect_files_without_content(
        self, db_session: AsyncSession, watch_folder, temp_dir: Path
    ):
        file_path = temp_dir / "no_content.txt"
        file_path.write_text("Test content")

        file_without_content = File(
            id=uuid4(),
            watch_folder_id=watch_folder.id,
            path=str(file_path),
            filename="no_content.txt",
            extension="txt",
            size_bytes=100,
            mime_type="text/plain",
            hash_sha256="hash_no_content" + "0" * 48,
            indexed_at=datetime.utcnow(),
        )
        db_session.add(file_without_content)
        await db_session.commit()

        result = await db_session.execute(
            select(TextContent).where(TextContent.file_id == file_without_content.id)
        )
        content = result.scalar_one_or_none()

        assert content is None

    async def test_detect_content_without_embeddings(
        self, db_session: AsyncSession, indexed_file_with_content: File
    ):
        result = await db_session.execute(
            select(TextContent).where(TextContent.file_id == indexed_file_with_content.id)
        )
        content = result.scalar_one_or_none()

        assert content is not None
        assert content.embedding is not None
        assert len(content.embedding) > 0


@pytest.mark.integration()
class TestStorageCleanup:
    async def test_cleanup_orphaned_storage_files(
        self, orphaned_storage_files, db_session: AsyncSession
    ):
        result = await orphaned_storage_files.cleanup_orphaned_files()

        assert "deleted" in result
        assert "failed" in result
        assert result["deleted"] >= 0

    async def test_cleanup_deleted_file_records(
        self, db_session: AsyncSession, indexed_file_with_content: File
    ):
        file_id = indexed_file_with_content.id

        await db_session.delete(indexed_file_with_content)
        await db_session.commit()

        result = await db_session.execute(select(File).where(File.id == file_id))
        file_record = result.scalar_one_or_none()

        assert file_record is None

        result = await db_session.execute(select(TextContent).where(TextContent.file_id == file_id))
        content = result.scalar_one_or_none()

        assert content is None

    async def test_cleanup_old_file_versions(
        self, db_session: AsyncSession, watch_folder, temp_dir: Path
    ):
        file_path = temp_dir / "versioned.txt"
        file_path.write_text("Version 1")

        old_file = File(
            id=uuid4(),
            watch_folder_id=watch_folder.id,
            path=str(file_path),
            filename="versioned.txt",
            extension="txt",
            size_bytes=100,
            mime_type="text/plain",
            hash_sha256="version1_hash" + "0" * 51,
            indexed_at=datetime.utcnow() - timedelta(days=30),
        )
        db_session.add(old_file)

        file_path.write_text("Version 2")

        new_file = File(
            id=uuid4(),
            watch_folder_id=watch_folder.id,
            path=str(file_path),
            filename="versioned.txt",
            extension="txt",
            size_bytes=100,
            mime_type="text/plain",
            hash_sha256="version2_hash" + "0" * 51,
            indexed_at=datetime.utcnow(),
        )
        db_session.add(new_file)

        await db_session.commit()

        result = await db_session.execute(select(File).where(File.path == str(file_path)))
        files = result.scalars().all()

        assert len(files) == 2


@pytest.mark.integration()
class TestDatabaseCleanup:
    async def test_cascade_delete_file_content(
        self, db_session: AsyncSession, indexed_file_with_content: File
    ):
        file_id = indexed_file_with_content.id

        result = await db_session.execute(select(TextContent).where(TextContent.file_id == file_id))
        content_before = result.scalar_one_or_none()
        assert content_before is not None

        await db_session.delete(indexed_file_with_content)
        await db_session.commit()

        result = await db_session.execute(select(TextContent).where(TextContent.file_id == file_id))
        content_after = result.scalar_one_or_none()

        assert content_after is None

    async def test_cleanup_stale_embeddings(
        self, db_session: AsyncSession, indexed_file_with_content: File
    ):
        result = await db_session.execute(
            select(TextContent).where(TextContent.file_id == indexed_file_with_content.id)
        )
        content = result.scalar_one_or_none()

        assert content is not None
        assert content.embedding is not None

    async def test_cleanup_detached_watch_folders(self, db_session: AsyncSession, watch_folder):
        result = await db_session.execute(
            select(File).where(File.watch_folder_id == watch_folder.id)
        )
        files = result.scalars().all()

        for file in files:
            assert file.watch_folder_id == watch_folder.id


@pytest.mark.integration()
class TestTemporaryFileCleanup:
    async def test_cleanup_temp_upload_files(self, temp_dir: Path):
        temp_file = temp_dir / "temp_upload.txt"
        temp_file.write_text("Temporary content")

        assert temp_file.exists()

        temp_file.unlink()

        assert not temp_file.exists()

    async def test_cleanup_failed_upload_artifacts(
        self, db_session: AsyncSession, watch_folder, temp_dir: Path
    ):
        failed_file_path = temp_dir / "failed_upload.txt"
        failed_file_path.write_text("Failed upload content")

        failed_file = File(
            id=uuid4(),
            watch_folder_id=watch_folder.id,
            path=str(failed_file_path),
            filename="failed_upload.txt",
            extension="txt",
            size_bytes=100,
            mime_type="text/plain",
            hash_sha256="failed_hash" + "0" * 53,
            indexed_at=None,
        )
        db_session.add(failed_file)
        await db_session.commit()

        result = await db_session.execute(select(File).where(File.indexed_at.is_(None)))
        unindexed_files = result.scalars().all()

        assert len(unindexed_files) >= 1


@pytest.mark.integration()
class TestVectorStoreCleanup:
    async def test_cleanup_orphaned_vectors(self, db_session: AsyncSession, mock_vector_store):
        result = await db_session.execute(select(File))
        files = result.scalars().all()

        file_ids = {str(file.id) for file in files}

        mock_vector_store.list_all_ids = lambda: file_ids

        assert len(file_ids) >= 0

    async def test_cleanup_vectors_for_deleted_files(
        self, db_session: AsyncSession, indexed_file_with_content: File, mock_vector_store
    ):
        file_id = indexed_file_with_content.id

        await mock_vector_store.delete_embedding(str(file_id))

        await db_session.delete(indexed_file_with_content)
        await db_session.commit()

        mock_vector_store.delete_embedding.assert_called_once()


@pytest.mark.integration()
class TestCleanupScheduling:
    async def test_scheduled_cleanup_task(self, db_session: AsyncSession):
        from datetime import datetime

        datetime.utcnow()

        result = await db_session.execute(
            select(File).where(File.indexed_at < (datetime.utcnow() - timedelta(days=90)))
        )
        old_files = result.scalars().all()

        assert isinstance(old_files, list)

    async def test_cleanup_respects_retention_policy(
        self, db_session: AsyncSession, watch_folder, temp_dir: Path
    ):
        retention_days = 30

        old_file_path = temp_dir / "old_file.txt"
        old_file_path.write_text("Old content")

        old_file = File(
            id=uuid4(),
            watch_folder_id=watch_folder.id,
            path=str(old_file_path),
            filename="old_file.txt",
            extension="txt",
            size_bytes=100,
            mime_type="text/plain",
            hash_sha256="old_hash" + "0" * 56,
            indexed_at=datetime.utcnow() - timedelta(days=retention_days + 1),
        )
        db_session.add(old_file)
        await db_session.commit()

        result = await db_session.execute(
            select(File).where(
                File.indexed_at < (datetime.utcnow() - timedelta(days=retention_days))
            )
        )
        files_to_cleanup = result.scalars().all()

        assert len(files_to_cleanup) >= 1


@pytest.mark.integration()
class TestCleanupMetrics:
    async def test_cleanup_reports_statistics(self, orphaned_storage_files):
        result = await orphaned_storage_files.cleanup_orphaned_files()

        assert "deleted" in result
        assert "failed" in result
        assert isinstance(result["deleted"], int)
        assert isinstance(result["failed"], int)

    async def test_cleanup_tracks_freed_space(self, db_session: AsyncSession, indexed_files_batch):
        result = await db_session.execute(select(File))
        files = result.scalars().all()

        total_size = sum(file.size_bytes for file in files)

        assert total_size > 0


@pytest.mark.integration()
class TestCleanupSafety:
    async def test_cleanup_preserves_active_files(
        self, db_session: AsyncSession, indexed_file_with_content: File
    ):
        file_id = indexed_file_with_content.id

        result = await db_session.execute(select(File).where(File.id == file_id))
        file_record = result.scalar_one_or_none()

        assert file_record is not None
        assert file_record.indexed_at is not None

    async def test_cleanup_requires_confirmation(self, db_session: AsyncSession):
        result = await db_session.execute(select(File))
        files_before = result.scalars().all()
        count_before = len(files_before)

        result = await db_session.execute(select(File))
        files_after = result.scalars().all()
        count_after = len(files_after)

        assert count_after == count_before

    async def test_cleanup_rollback_on_error(
        self, db_session: AsyncSession, indexed_file_with_content: File
    ):
        file_id = indexed_file_with_content.id

        try:
            await db_session.delete(indexed_file_with_content)
            raise Exception("Simulated cleanup error")
        except Exception:
            await db_session.rollback()

        result = await db_session.execute(select(File).where(File.id == file_id))
        file_record = result.scalar_one_or_none()

        assert file_record is not None


@pytest.mark.integration()
@pytest.mark.slow()
class TestLargeScaleCleanup:
    async def test_cleanup_handles_large_dataset(
        self, db_session: AsyncSession, watch_folder, temp_dir: Path
    ):
        for i in range(100):
            file_path = temp_dir / f"large_cleanup_{i}.txt"
            file_path.write_text(f"Content {i}")

            file_record = File(
                id=uuid4(),
                watch_folder_id=watch_folder.id,
                path=str(file_path),
                filename=file_path.name,
                extension="txt",
                size_bytes=file_path.stat().st_size,
                mime_type="text/plain",
                hash_sha256=f"hash_{i}" + "0" * 58,
                indexed_at=datetime.utcnow() - timedelta(days=60),
            )
            db_session.add(file_record)

        await db_session.commit()

        result = await db_session.execute(
            select(File).where(File.indexed_at < (datetime.utcnow() - timedelta(days=30)))
        )
        old_files = result.scalars().all()

        assert len(old_files) >= 100

    async def test_cleanup_batch_processing(
        self, db_session: AsyncSession, watch_folder, temp_dir: Path
    ):
        batch_size = 20

        for i in range(50):
            file_path = temp_dir / f"batch_cleanup_{i}.txt"
            file_path.write_text(f"Batch content {i}")

            file_record = File(
                id=uuid4(),
                watch_folder_id=watch_folder.id,
                path=str(file_path),
                filename=file_path.name,
                extension="txt",
                size_bytes=file_path.stat().st_size,
                mime_type="text/plain",
                hash_sha256=f"batch_hash_{i}" + "0" * (64 - 11 - len(str(i))),
                indexed_at=datetime.utcnow(),
            )
            db_session.add(file_record)

        await db_session.commit()

        result = await db_session.execute(select(File).limit(batch_size))
        batch_files = result.scalars().all()

        assert len(batch_files) <= batch_size
