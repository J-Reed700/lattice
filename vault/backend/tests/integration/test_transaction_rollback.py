from pathlib import Path
from uuid import uuid4

from httpx import AsyncClient
import pytest
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from src.models import File, TextContent


@pytest.mark.integration()
class TestUploadRollback:
    async def test_rollback_on_embedding_failure(
        self,
        db_session: AsyncSession,
        watch_folder,
        sample_text_file: Path,
        mock_embedding_service,
        mocker,
    ):
        from src.services.indexing import IndexingService

        mock_embedding_service.text_generator.generate_from_text.side_effect = Exception(
            "Embedding generation failed"
        )

        mock_storage = mocker.Mock()
        mock_storage.store_file = mocker.AsyncMock(return_value=("http://storage.url", None))

        indexing_service = IndexingService(db_session)
        mocker.patch.object(
            indexing_service, "_get_embedding_service", return_value=mock_embedding_service
        )
        mocker.patch.object(indexing_service, "_get_storage_service", return_value=mock_storage)

        with pytest.raises(Exception):
            await indexing_service.index_file(
                file_path=str(sample_text_file), watch_folder_id=watch_folder.id, force=False
            )

        result = await db_session.execute(select(File).where(File.path == str(sample_text_file)))
        file_record = result.scalar_one_or_none()

        assert file_record is None or file_record.indexed_at is None

    async def test_rollback_on_storage_failure(
        self,
        db_session: AsyncSession,
        watch_folder,
        sample_text_file: Path,
        mock_embedding_service,
        mocker,
    ):
        from src.services.indexing import IndexingService

        mock_storage = mocker.Mock()
        mock_storage.store_file = mocker.AsyncMock(
            side_effect=Exception("Storage service unavailable")
        )

        indexing_service = IndexingService(db_session)
        mocker.patch.object(
            indexing_service, "_get_embedding_service", return_value=mock_embedding_service
        )
        mocker.patch.object(indexing_service, "_get_storage_service", return_value=mock_storage)

        with pytest.raises(Exception):
            await indexing_service.index_file(
                file_path=str(sample_text_file), watch_folder_id=watch_folder.id, force=False
            )

        result = await db_session.execute(select(File).where(File.path == str(sample_text_file)))
        file_record = result.scalar_one_or_none()

        if file_record:
            text_result = await db_session.execute(
                select(TextContent).where(TextContent.file_id == file_record.id)
            )
            text_content = text_result.scalar_one_or_none()
            assert text_content is None

    async def test_partial_batch_rollback(
        self, db_session: AsyncSession, watch_folder, temp_dir: Path, mock_embedding_service, mocker
    ):
        from src.services.indexing import IndexingService

        files = []
        for i in range(5):
            file_path = temp_dir / f"batch_{i}.txt"
            file_path.write_text(f"Content {i}")
            files.append(file_path)

        call_count = 0

        def mock_generate(*args, **kwargs):
            nonlocal call_count
            call_count += 1
            if call_count == 3:
                raise Exception("Embedding failed on third file")
            return [0.1] * 384

        mock_embedding_service.text_generator.generate_from_text.side_effect = mock_generate

        mock_storage = mocker.Mock()
        mock_storage.store_file = mocker.AsyncMock(return_value=("http://storage.url", None))

        indexing_service = IndexingService(db_session)
        mocker.patch.object(
            indexing_service, "_get_embedding_service", return_value=mock_embedding_service
        )
        mocker.patch.object(indexing_service, "_get_storage_service", return_value=mock_storage)

        for file_path in files:
            try:
                await indexing_service.index_file(
                    file_path=str(file_path), watch_folder_id=watch_folder.id, force=False
                )
            except Exception:
                pass

        result = await db_session.execute(select(File))
        indexed_files = result.scalars().all()

        assert len(indexed_files) <= 2


@pytest.mark.integration()
class TestSearchRollback:
    async def test_search_history_rollback_on_error(
        self, db_session: AsyncSession, indexed_file_with_content, mocker
    ):
        from src.services.search_history import SearchHistoryService

        service = SearchHistoryService(db_session)

        original_commit = db_session.commit

        async def failing_commit(*args, **kwargs):
            raise Exception("Commit failed")

        mocker.patch.object(db_session, "commit", side_effect=failing_commit)

        with pytest.raises(Exception):
            await service.record_search(
                query="test query", search_type="text", result_count=1, execution_time_ms=100
            )

        mocker.patch.object(db_session, "commit", side_effect=original_commit)

        from src.models.search_history import SearchHistory

        result = await db_session.execute(
            select(SearchHistory).where(SearchHistory.query == "test query")
        )
        history = result.scalar_one_or_none()

        assert history is None


@pytest.mark.integration()
class TestDeleteRollback:
    async def test_rollback_on_cascade_delete_failure(
        self, db_session: AsyncSession, indexed_file_with_content
    ):
        file_id = indexed_file_with_content.id

        result = await db_session.execute(select(TextContent).where(TextContent.file_id == file_id))
        content_before = result.scalar_one_or_none()
        assert content_before is not None

        original_delete = db_session.delete

        async def failing_delete(*args, **kwargs):
            await original_delete(*args, **kwargs)
            raise Exception("Delete operation failed")

        from unittest.mock import patch

        with patch.object(db_session, "delete", side_effect=failing_delete):
            try:
                await db_session.delete(indexed_file_with_content)
                await db_session.commit()
            except Exception:
                await db_session.rollback()

        result = await db_session.execute(select(File).where(File.id == file_id))
        file_after = result.scalar_one_or_none()

        result = await db_session.execute(select(TextContent).where(TextContent.file_id == file_id))
        content_after = result.scalar_one_or_none()

        assert file_after is not None
        assert content_after is not None


@pytest.mark.integration()
class TestConstraintViolationRollback:
    async def test_rollback_on_duplicate_hash(
        self, db_session: AsyncSession, watch_folder, sample_text_file: Path
    ):
        file1 = File(
            id=uuid4(),
            watch_folder_id=watch_folder.id,
            path=str(sample_text_file),
            filename=sample_text_file.name,
            extension="txt",
            size_bytes=100,
            mime_type="text/plain",
            hash_sha256="duplicate_hash" + "0" * 50,
            indexed_at=None,
        )
        db_session.add(file1)
        await db_session.commit()

        file2 = File(
            id=uuid4(),
            watch_folder_id=watch_folder.id,
            path=str(sample_text_file) + "_copy",
            filename=sample_text_file.name + "_copy",
            extension="txt",
            size_bytes=100,
            mime_type="text/plain",
            hash_sha256="duplicate_hash" + "0" * 50,
            indexed_at=None,
        )
        db_session.add(file2)

        try:
            await db_session.commit()
        except IntegrityError:
            await db_session.rollback()

        result = await db_session.execute(select(File))
        files = result.scalars().all()

        assert len(files) == 1

    async def test_rollback_on_foreign_key_violation(self, db_session: AsyncSession):
        invalid_file = File(
            id=uuid4(),
            watch_folder_id=uuid4(),
            path="/invalid/path.txt",
            filename="invalid.txt",
            extension="txt",
            size_bytes=100,
            mime_type="text/plain",
            hash_sha256="invalid_hash" + "0" * 52,
            indexed_at=None,
        )
        db_session.add(invalid_file)

        with pytest.raises(IntegrityError):
            await db_session.commit()

        await db_session.rollback()

        result = await db_session.execute(select(File).where(File.path == "/invalid/path.txt"))
        file_record = result.scalar_one_or_none()

        assert file_record is None


@pytest.mark.integration()
class TestConcurrentTransactionConflicts:
    async def test_optimistic_locking_conflict(
        self, db_session: AsyncSession, indexed_file_with_content
    ):
        import os

        from sqlalchemy.ext.asyncio import async_sessionmaker, create_async_engine

        TEST_DATABASE_URL = os.getenv(
            "TEST_DATABASE_URL", "postgresql+asyncpg://vault:vault@localhost:5432/vault_test"
        )

        engine2 = create_async_engine(TEST_DATABASE_URL, poolclass=None)
        async_session2 = async_sessionmaker(engine2, class_=AsyncSession)

        async with async_session2() as session2:
            result1 = await db_session.execute(
                select(File).where(File.id == indexed_file_with_content.id)
            )
            file1 = result1.scalar_one()

            result2 = await session2.execute(
                select(File).where(File.id == indexed_file_with_content.id)
            )
            file2 = result2.scalar_one()

            file1.size_bytes = 1000
            file2.size_bytes = 2000

            await db_session.commit()

            try:
                await session2.commit()
            except Exception:
                await session2.rollback()

        await engine2.dispose()

        result = await db_session.execute(
            select(File).where(File.id == indexed_file_with_content.id)
        )
        final_file = result.scalar_one()

        assert final_file.size_bytes == 1000


@pytest.mark.integration()
class TestNestedTransactionRollback:
    async def test_nested_savepoint_rollback(
        self, db_session: AsyncSession, watch_folder, temp_dir: Path
    ):
        file1_path = temp_dir / "file1.txt"
        file1_path.write_text("Content 1")

        file1 = File(
            id=uuid4(),
            watch_folder_id=watch_folder.id,
            path=str(file1_path),
            filename="file1.txt",
            extension="txt",
            size_bytes=100,
            mime_type="text/plain",
            hash_sha256="hash1" + "0" * 59,
            indexed_at=None,
        )
        db_session.add(file1)
        await db_session.flush()

        savepoint = await db_session.begin_nested()

        file2_path = temp_dir / "file2.txt"
        file2_path.write_text("Content 2")

        file2 = File(
            id=uuid4(),
            watch_folder_id=uuid4(),
            path=str(file2_path),
            filename="file2.txt",
            extension="txt",
            size_bytes=100,
            mime_type="text/plain",
            hash_sha256="hash2" + "0" * 59,
            indexed_at=None,
        )
        db_session.add(file2)

        await savepoint.rollback()

        await db_session.commit()

        result = await db_session.execute(select(File))
        files = result.scalars().all()

        assert len(files) == 1
        assert files[0].filename == "file1.txt"


@pytest.mark.integration()
class TestAPIRollbackBehavior:
    async def test_api_rollback_on_validation_error(self, client: AsyncClient, watch_folder):
        payload = {"watch_folder_id": str(watch_folder.id), "path": "", "force_reindex": False}

        response = await client.post("/api/v1/files", json=payload)

        assert response.status_code in [400, 422, 500]

    async def test_api_rollback_on_server_error(self, client: AsyncClient, watch_folder, mocker):
        from src.services.indexing import IndexingService

        mocker.patch.object(
            IndexingService, "index_file", side_effect=Exception("Internal server error")
        )

        payload = {
            "watch_folder_id": str(watch_folder.id),
            "path": "/test/file.txt",
            "force_reindex": False,
        }

        response = await client.post("/api/v1/files", json=payload)

        assert response.status_code == 500
