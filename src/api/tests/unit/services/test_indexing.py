from unittest.mock import AsyncMock, Mock

import pytest

from src.services.indexing.processor import FileProcessor
from src.services.indexing.service import IndexingService


@pytest.mark.unit()
class TestFileProcessor:
    @pytest.fixture()
    def file_processor(self, db_session, mock_embedding_service):
        mock_storage = Mock()
        mock_storage.store_file = AsyncMock(return_value=("http://cloud.url", None))

        return FileProcessor(
            session=db_session,
            embedding_service=mock_embedding_service,
            storage_service=mock_storage,
        )

    async def test_process_file_not_found(self, file_processor):
        success, error = await file_processor.process_file("/nonexistent/file.txt")
        assert success is False
        assert "not found" in error.lower()

    async def test_process_file_not_a_file(self, file_processor, temp_dir):
        success, error = await file_processor.process_file(str(temp_dir))
        assert success is False
        assert "not a file" in error.lower()

    async def test_hash_file(self, file_processor, sample_text_file):
        hash1 = file_processor._hash_file(str(sample_text_file))
        assert len(hash1) == 64
        assert all(c in "0123456789abcdef" for c in hash1)

        hash2 = file_processor._hash_file(str(sample_text_file))
        assert hash1 == hash2

    async def test_hash_file_different_content(self, file_processor, sample_text_file, temp_dir):
        hash1 = file_processor._hash_file(str(sample_text_file))

        other_file = temp_dir / "other.txt"
        other_file.write_text("Different content")
        hash2 = file_processor._hash_file(str(other_file))

        assert hash1 != hash2

    async def test_is_indexed_false(self, file_processor):
        result = await file_processor._is_indexed("nonexistent_hash")
        assert result is False

    async def test_reindex_file_not_found(self, file_processor):
        success, error = await file_processor.reindex_file("/nonexistent/file.txt")
        assert success is False
        assert "not found" in error.lower()


@pytest.mark.unit()
class TestIndexingService:
    @pytest.fixture()
    def indexing_service(self, db_session, mock_embedding_service):
        mock_storage = Mock()
        mock_storage.store_file = AsyncMock(return_value=("http://cloud.url", None))

        return IndexingService(
            session=db_session,
            embedding_service=mock_embedding_service,
            storage_service=mock_storage,
            max_concurrent=3,
        )

    async def test_index_directory_no_files(self, indexing_service, temp_dir):
        with pytest.raises(ValueError, match="No supported files found"):
            await indexing_service.index_directory(str(temp_dir))

    async def test_index_file_not_found(self, indexing_service):
        success, error = await indexing_service.index_file("/nonexistent/file.txt")
        assert success is False
        assert error is not None

    async def test_get_progress_invalid_task(self, indexing_service):
        with pytest.raises(ValueError, match="Unknown task ID"):
            await indexing_service.get_progress("invalid-task-id")

    async def test_cancel_indexing_invalid_task(self, indexing_service):
        with pytest.raises(ValueError, match="Unknown task ID"):
            await indexing_service.cancel_indexing("invalid-task-id")

    async def test_wait_for_completion_invalid_task(self, indexing_service):
        with pytest.raises(ValueError, match="Unknown task ID"):
            await indexing_service.wait_for_completion("invalid-task-id")

    async def test_get_all_tasks_empty(self, indexing_service):
        tasks = indexing_service.get_all_tasks()
        assert tasks == {}

    async def test_cleanup_completed_tasks_empty(self, indexing_service):
        removed = indexing_service.cleanup_completed_tasks()
        assert removed == 0


@pytest.mark.unit()
class TestIndexingTaskTracking:
    @pytest.fixture()
    def indexing_service(self, db_session, mock_embedding_service):
        mock_storage = Mock()
        mock_storage.store_file = AsyncMock(return_value=("http://cloud.url", None))

        return IndexingService(
            session=db_session,
            embedding_service=mock_embedding_service,
            storage_service=mock_storage,
            max_concurrent=2,
        )

    async def test_index_directory_creates_task(self, indexing_service, temp_dir, sample_text_file):
        task_id = await indexing_service.index_directory(str(temp_dir), recursive=False)
        assert task_id is not None
        assert task_id in indexing_service._tasks

        progress = await indexing_service.get_progress(task_id)
        assert progress["task_id"] == task_id
        assert progress["total_files"] == 1
        assert progress["status"] in ["running", "completed", "failed"]


@pytest.mark.unit()
class TestFileValidation:
    @pytest.fixture()
    def file_processor(self, db_session, mock_embedding_service):
        mock_storage = Mock()
        mock_storage.store_file = AsyncMock(return_value=("http://cloud.url", None))

        return FileProcessor(
            session=db_session,
            embedding_service=mock_embedding_service,
            storage_service=mock_storage,
        )

    async def test_process_empty_file(self, file_processor, temp_dir):
        empty_file = temp_dir / "empty.txt"
        empty_file.write_text("")

        hash_value = file_processor._hash_file(str(empty_file))
        assert len(hash_value) == 64

    async def test_process_large_file(self, file_processor, large_text_file):
        hash_value = file_processor._hash_file(str(large_text_file))
        assert len(hash_value) == 64


@pytest.mark.unit()
class TestMimeTypeDetection:
    @pytest.fixture()
    def file_processor(self, db_session, mock_embedding_service):
        mock_storage = Mock()
        mock_storage.store_file = AsyncMock(return_value=("http://cloud.url", None))

        return FileProcessor(
            session=db_session,
            embedding_service=mock_embedding_service,
            storage_service=mock_storage,
        )

    async def test_detect_text_mime_type(self, file_processor, sample_text_file):
        import mimetypes

        mime_type, _ = mimetypes.guess_type(str(sample_text_file))
        assert mime_type == "text/plain"

    async def test_detect_markdown_mime_type(self, file_processor, sample_markdown_file):
        import mimetypes

        mime_type, _ = mimetypes.guess_type(str(sample_markdown_file))
        assert mime_type in ["text/markdown", "text/x-markdown", None]

    async def test_detect_json_mime_type(self, file_processor, sample_json_file):
        import mimetypes

        mime_type, _ = mimetypes.guess_type(str(sample_json_file))
        assert mime_type == "application/json"


@pytest.mark.unit()
class TestErrorHandling:
    @pytest.fixture()
    def file_processor(self, db_session, mock_embedding_service):
        mock_storage = Mock()
        mock_storage.store_file = AsyncMock(side_effect=Exception("Storage error"))

        return FileProcessor(
            session=db_session,
            embedding_service=mock_embedding_service,
            storage_service=mock_storage,
        )

    async def test_process_file_storage_error(self, file_processor, sample_text_file):
        success, error = await file_processor.process_file(str(sample_text_file))
        assert success is False
        assert error is not None
        assert "storage error" in error.lower()


@pytest.mark.unit()
class TestModifiedFileDetection:
    @pytest.fixture()
    def file_processor(self, db_session, mock_embedding_service):
        mock_storage = Mock()
        mock_storage.store_file = AsyncMock(return_value=("http://cloud.url", None))

        return FileProcessor(
            session=db_session,
            embedding_service=mock_embedding_service,
            storage_service=mock_storage,
        )

    async def test_check_and_update_modified_new_file(self, file_processor, sample_text_file):
        success, message = await file_processor.check_and_update_modified(str(sample_text_file))
        assert message is None or "not found" in message.lower()

    async def test_check_and_update_modified_file_not_found(self, file_processor):
        success, error = await file_processor.check_and_update_modified("/nonexistent/file.txt")
        assert success is False
        assert "not found" in error.lower()
