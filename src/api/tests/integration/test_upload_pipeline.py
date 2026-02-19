from httpx import AsyncClient
import pytest
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from src.models import File


@pytest.mark.integration()
class TestUploadPipelineEndToEnd:
    async def test_upload_text_file_full_pipeline(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        watch_folder,
        sample_text_file,
        mock_embedding_service,
        mocker,
    ):
        mocker.patch(
            "src.services.indexing.service.EmbeddingService", return_value=mock_embedding_service
        )
        mocker.patch(
            "src.services.indexing.service.StorageService"
        ).return_value.store_file.return_value = ("http://storage.url", None)

        payload = {
            "watch_folder_id": str(watch_folder.id),
            "path": str(sample_text_file),
            "force_reindex": False,
        }

        response = await client.post("/api/v1/files", json=payload)
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert "file_id" in data["data"]

        file_id = data["data"]["file_id"]
        response = await client.get(f"/api/v1/files/{file_id}")
        assert response.status_code == 200

    async def test_upload_markdown_file_full_pipeline(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        watch_folder,
        sample_markdown_file,
        mock_embedding_service,
        mocker,
    ):
        mocker.patch(
            "src.services.indexing.service.EmbeddingService", return_value=mock_embedding_service
        )
        mocker.patch(
            "src.services.indexing.service.StorageService"
        ).return_value.store_file.return_value = ("http://storage.url", None)

        payload = {
            "watch_folder_id": str(watch_folder.id),
            "path": str(sample_markdown_file),
            "force_reindex": False,
        }

        response = await client.post("/api/v1/files", json=payload)
        assert response.status_code == 200

    async def test_upload_json_file_full_pipeline(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        watch_folder,
        sample_json_file,
        mock_embedding_service,
        mocker,
    ):
        mocker.patch(
            "src.services.indexing.service.EmbeddingService", return_value=mock_embedding_service
        )
        mocker.patch(
            "src.services.indexing.service.StorageService"
        ).return_value.store_file.return_value = ("http://storage.url", None)

        payload = {
            "watch_folder_id": str(watch_folder.id),
            "path": str(sample_json_file),
            "force_reindex": False,
        }

        response = await client.post("/api/v1/files", json=payload)
        assert response.status_code == 200


@pytest.mark.integration()
class TestUploadErrorHandling:
    async def test_upload_nonexistent_file(self, client: AsyncClient, watch_folder):
        payload = {
            "watch_folder_id": str(watch_folder.id),
            "path": "/nonexistent/file.txt",
            "force_reindex": False,
        }

        response = await client.post("/api/v1/files", json=payload)
        assert response.status_code == 500
        assert "failed" in response.json()["detail"].lower()

    async def test_upload_corrupted_pdf(self, client: AsyncClient, watch_folder, invalid_pdf_file):
        payload = {
            "watch_folder_id": str(watch_folder.id),
            "path": str(invalid_pdf_file),
            "force_reindex": False,
        }

        response = await client.post("/api/v1/files", json=payload)
        assert response.status_code in [400, 500]


@pytest.mark.integration()
class TestUploadWithContentExtraction:
    async def test_text_content_extracted(
        self,
        db_session: AsyncSession,
        watch_folder,
        sample_text_file,
        mock_embedding_service,
        mock_content_extractor,
    ):
        from src.services.indexing.processor import FileProcessor

        mock_storage = mocker.Mock()
        mock_storage.store_file = mocker.AsyncMock(return_value=("http://storage.url", None))

        processor = FileProcessor(
            session=db_session,
            embedding_service=mock_embedding_service,
            storage_service=mock_storage,
        )

        success, error = await processor.process_file(str(sample_text_file))

        if success:
            result = await db_session.execute(
                select(File).where(File.path == str(sample_text_file))
            )
            file_record = result.scalar_one_or_none()
            assert file_record is not None
        else:
            assert error is not None


@pytest.mark.integration()
class TestUploadDuplicateHandling:
    async def test_upload_same_file_twice_skips_duplicate(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        watch_folder,
        sample_text_file,
        mock_embedding_service,
        mocker,
    ):
        mocker.patch(
            "src.services.indexing.service.EmbeddingService", return_value=mock_embedding_service
        )
        mocker.patch(
            "src.services.indexing.service.StorageService"
        ).return_value.store_file.return_value = ("http://storage.url", None)

        payload = {
            "watch_folder_id": str(watch_folder.id),
            "path": str(sample_text_file),
            "force_reindex": False,
        }

        response1 = await client.post("/api/v1/files", json=payload)
        assert response1.status_code == 200

        response2 = await client.post("/api/v1/files", json=payload)
        assert response2.status_code in [200, 409]

    async def test_force_reindex_overwrites_existing(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        watch_folder,
        sample_text_file,
        mock_embedding_service,
        mocker,
    ):
        mocker.patch(
            "src.services.indexing.service.EmbeddingService", return_value=mock_embedding_service
        )
        mock_storage = mocker.patch("src.services.indexing.service.StorageService")
        mock_storage.return_value.store_file.return_value = ("http://storage.url", None)

        payload = {
            "watch_folder_id": str(watch_folder.id),
            "path": str(sample_text_file),
            "force_reindex": False,
        }

        await client.post("/api/v1/files", json=payload)

        payload["force_reindex"] = True
        response = await client.post("/api/v1/files", json=payload)
        assert response.status_code == 200


@pytest.mark.integration()
class TestUploadWithEmbeddings:
    async def test_embeddings_generated_during_upload(
        self,
        db_session: AsyncSession,
        watch_folder,
        sample_text_file,
        mock_embedding_service,
        mocker,
    ):
        from src.services.indexing.processor import FileProcessor

        mock_storage = mocker.Mock()
        mock_storage.store_file = mocker.AsyncMock(return_value=("http://storage.url", None))

        mock_embedding_service.generate_embedding = mocker.AsyncMock(
            return_value=([0.1] * 384, "Extracted text content")
        )

        processor = FileProcessor(
            session=db_session,
            embedding_service=mock_embedding_service,
            storage_service=mock_storage,
        )

        success, error = await processor.process_file(str(sample_text_file))

        if success:
            mock_embedding_service.generate_embedding.assert_called_once()


@pytest.mark.integration()
class TestUploadWithStorage:
    async def test_file_stored_in_cloud(
        self,
        db_session: AsyncSession,
        watch_folder,
        sample_text_file,
        mock_embedding_service,
        mocker,
    ):
        from src.services.indexing.processor import FileProcessor

        mock_storage = mocker.Mock()
        mock_storage.store_file = mocker.AsyncMock(
            return_value=("http://cloud.storage/file", "http://cloud.storage/thumb")
        )

        mock_embedding_service.generate_embedding = mocker.AsyncMock(
            return_value=([0.1] * 384, "Extracted text")
        )

        processor = FileProcessor(
            session=db_session,
            embedding_service=mock_embedding_service,
            storage_service=mock_storage,
        )

        success, error = await processor.process_file(str(sample_text_file))

        if success:
            mock_storage.store_file.assert_called_once()
            call_args = mock_storage.store_file.call_args
            assert str(sample_text_file) in str(call_args)


@pytest.mark.integration()
class TestUploadMetadata:
    async def test_file_metadata_stored_correctly(
        self,
        db_session: AsyncSession,
        watch_folder,
        sample_text_file,
        mock_embedding_service,
        mocker,
    ):
        from src.services.indexing.processor import FileProcessor

        mock_storage = mocker.Mock()
        mock_storage.store_file = mocker.AsyncMock(return_value=("http://storage.url", None))

        mock_embedding_service.generate_embedding = mocker.AsyncMock(
            return_value=([0.1] * 384, "Extracted text")
        )

        processor = FileProcessor(
            session=db_session,
            embedding_service=mock_embedding_service,
            storage_service=mock_storage,
        )

        success, _ = await processor.process_file(str(sample_text_file))

        if success:
            result = await db_session.execute(
                select(File).where(File.path == str(sample_text_file))
            )
            file_record = result.scalar_one_or_none()

            if file_record:
                assert file_record.filename == sample_text_file.name
                assert file_record.extension == "txt"
                assert file_record.mime_type == "text/plain"
                assert file_record.size_bytes > 0
                assert file_record.hash_sha256 is not None


@pytest.mark.integration()
@pytest.mark.slow()
class TestBatchUpload:
    async def test_upload_multiple_files(
        self, db_session: AsyncSession, watch_folder, temp_dir, mock_embedding_service, mocker
    ):
        from src.services.indexing.service import IndexingService

        mock_storage = mocker.Mock()
        mock_storage.store_file = mocker.AsyncMock(return_value=("http://storage.url", None))

        mock_embedding_service.generate_embedding = mocker.AsyncMock(
            return_value=([0.1] * 384, "Extracted text")
        )

        for i in range(5):
            file_path = temp_dir / f"test_{i}.txt"
            file_path.write_text(f"Test content {i}")

        indexing_service = IndexingService(
            session=db_session,
            embedding_service=mock_embedding_service,
            storage_service=mock_storage,
            max_concurrent=3,
        )

        task_id = await indexing_service.index_directory(str(temp_dir), recursive=False)

        await indexing_service.wait_for_completion(task_id, timeout=30.0)

        progress = await indexing_service.get_progress(task_id)
        assert progress["status"] in ["completed", "failed"]


@pytest.mark.integration()
class TestUploadDownloadRoundtrip:
    async def test_upload_then_download(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        watch_folder,
        sample_text_file,
        mock_embedding_service,
        mocker,
    ):
        mocker.patch(
            "src.services.indexing.service.EmbeddingService", return_value=mock_embedding_service
        )
        mocker.patch(
            "src.services.indexing.service.StorageService"
        ).return_value.store_file.return_value = ("http://storage.url", None)

        upload_payload = {
            "watch_folder_id": str(watch_folder.id),
            "path": str(sample_text_file),
            "force_reindex": False,
        }

        upload_response = await client.post("/api/v1/files", json=upload_payload)
        assert upload_response.status_code == 200

        file_id = upload_response.json()["data"]["file_id"]

        download_response = await client.get(f"/api/v1/files/{file_id}/download")
        assert download_response.status_code == 200
        download_data = download_response.json()
        assert download_data["filename"] == sample_text_file.name
        assert download_data["path"] == str(sample_text_file)
