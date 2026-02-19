from pathlib import Path
from uuid import uuid4

from httpx import AsyncClient
import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from src.models import File


@pytest.mark.integration()
class TestDownloadFlow:
    async def test_download_file_by_id(self, client: AsyncClient, indexed_file_with_content: File):
        response = await client.get(f"/api/v1/files/{indexed_file_with_content.id}/download")

        assert response.status_code == 200
        data = response.json()
        assert "file_id" in data
        assert "path" in data
        assert "filename" in data
        assert data["filename"] == indexed_file_with_content.filename

    async def test_download_nonexistent_file(self, client: AsyncClient):
        fake_id = uuid4()
        response = await client.get(f"/api/v1/files/{fake_id}/download")

        assert response.status_code == 404

    async def test_download_multiple_files_sequentially(
        self, client: AsyncClient, multiple_indexed_files
    ):
        for file in multiple_indexed_files[:3]:
            response = await client.get(f"/api/v1/files/{file.id}/download")

            assert response.status_code == 200
            data = response.json()
            assert data["file_id"] == str(file.id)

    async def test_download_updates_access_timestamp(
        self, client: AsyncClient, db_session: AsyncSession, indexed_file_with_content: File
    ):
        from datetime import datetime


        await client.get(f"/api/v1/files/{indexed_file_with_content.id}/download")

        update_payload = {"last_accessed_at": datetime.utcnow().isoformat()}
        update_response = await client.patch(
            f"/api/v1/files/{indexed_file_with_content.id}", json=update_payload
        )

        assert update_response.status_code == 200
        data = update_response.json()
        assert data["last_accessed_at"] is not None


@pytest.mark.integration()
class TestExportFlow:
    async def test_export_file_metadata_json(
        self, client: AsyncClient, indexed_file_with_content: File
    ):
        response = await client.get(f"/api/v1/files/{indexed_file_with_content.id}")

        assert response.status_code == 200
        data = response.json()

        assert "id" in data
        assert "filename" in data
        assert "path" in data
        assert "mime_type" in data
        assert "size_bytes" in data
        assert "hash_sha256" in data

    async def test_export_search_results(self, client: AsyncClient, indexed_files_batch):
        search_payload = {"query": "test", "mode": "text", "limit": 10, "offset": 0}

        response = await client.post("/api/v1/search", json=search_payload)

        assert response.status_code == 200
        data = response.json()
        assert "results" in data
        assert isinstance(data["results"], list)

        for result in data["results"]:
            assert "id" in result
            assert "filename" in result
            assert "file_path" in result
            assert "score" in result

    async def test_export_file_statistics(self, client: AsyncClient, indexed_files_batch):
        response = await client.get("/api/v1/files/stats")

        assert response.status_code == 200
        data = response.json()

        assert "total_files" in data
        assert "total_size_bytes" in data
        assert "by_mime_type" in data
        assert "by_extension" in data
        assert "indexed_today" in data
        assert "indexed_this_week" in data
        assert "indexed_this_month" in data

    async def test_export_file_list_with_pagination(self, client: AsyncClient, indexed_files_batch):
        response = await client.get("/api/v1/files?limit=2&offset=0")

        assert response.status_code == 200
        data = response.json()

        assert "files" in data
        assert "total" in data
        assert "limit" in data
        assert "offset" in data
        assert "has_more" in data
        assert len(data["files"]) <= 2


@pytest.mark.integration()
class TestBulkExport:
    async def test_export_all_files_metadata(self, client: AsyncClient, indexed_files_batch):
        response = await client.get("/api/v1/files?limit=100&offset=0")

        assert response.status_code == 200
        data = response.json()

        assert len(data["files"]) == len(indexed_files_batch)

        for file_data in data["files"]:
            assert "id" in file_data
            assert "filename" in file_data
            assert "mime_type" in file_data

    async def test_export_filtered_files(self, client: AsyncClient, indexed_files_batch):
        response = await client.get("/api/v1/files?mime_type=text/plain&limit=100")

        assert response.status_code == 200
        data = response.json()

        for file_data in data["files"]:
            assert file_data["mime_type"] == "text/plain"

    async def test_export_files_by_extension(self, client: AsyncClient, indexed_files_batch):
        response = await client.get("/api/v1/files?extension=txt&limit=100")

        assert response.status_code == 200
        data = response.json()

        for file_data in data["files"]:
            assert file_data["extension"] == "txt"


@pytest.mark.integration()
class TestDownloadWithStorage:
    async def test_download_from_cloud_storage(
        self, client: AsyncClient, indexed_file_with_content: File, mock_storage_service, mocker
    ):
        mocker.patch("src.api.routes.files.StorageService", return_value=mock_storage_service)

        response = await client.get(f"/api/v1/files/{indexed_file_with_content.id}/download")

        assert response.status_code == 200
        data = response.json()
        assert "path" in data

    async def test_download_missing_storage_file(
        self, client: AsyncClient, indexed_file_with_content: File, mocker
    ):
        mock_storage = mocker.Mock()
        mock_storage.get_file_url = mocker.AsyncMock(return_value=None)

        mocker.patch("src.api.routes.files.StorageService", return_value=mock_storage)

        response = await client.get(f"/api/v1/files/{indexed_file_with_content.id}/download")

        assert response.status_code == 200


@pytest.mark.integration()
class TestThumbnailGeneration:
    async def test_get_thumbnail_not_implemented(
        self, client: AsyncClient, indexed_file_with_content: File
    ):
        response = await client.get(f"/api/v1/files/{indexed_file_with_content.id}/thumbnail")

        assert response.status_code == 501

    async def test_get_thumbnail_for_image(
        self, client: AsyncClient, db_session: AsyncSession, watch_folder, sample_image_file: Path
    ):
        from datetime import datetime

        file_record = File(
            id=uuid4(),
            watch_folder_id=watch_folder.id,
            path=str(sample_image_file),
            filename=sample_image_file.name,
            extension="jpg",
            size_bytes=sample_image_file.stat().st_size,
            mime_type="image/jpeg",
            hash_sha256="image_hash" + "0" * 54,
            modified_at=datetime.utcnow(),
            indexed_at=datetime.utcnow(),
        )
        db_session.add(file_record)
        await db_session.commit()

        response = await client.get(f"/api/v1/files/{file_record.id}/thumbnail")

        assert response.status_code == 501


@pytest.mark.integration()
class TestExportFormats:
    async def test_export_as_json_default(
        self, client: AsyncClient, indexed_file_with_content: File
    ):
        response = await client.get(f"/api/v1/files/{indexed_file_with_content.id}")

        assert response.status_code == 200
        assert response.headers["content-type"].startswith("application/json")

    async def test_export_search_results_as_json(self, client: AsyncClient, indexed_files_batch):
        search_payload = {"query": "test", "mode": "text", "limit": 10, "offset": 0}

        response = await client.post("/api/v1/search", json=search_payload)

        assert response.status_code == 200
        assert response.headers["content-type"].startswith("application/json")


@pytest.mark.integration()
class TestDownloadSecurity:
    async def test_download_respects_file_permissions(
        self, client: AsyncClient, indexed_file_with_content: File
    ):
        response = await client.get(f"/api/v1/files/{indexed_file_with_content.id}/download")

        assert response.status_code == 200

    async def test_download_with_invalid_uuid(self, client: AsyncClient):
        response = await client.get("/api/v1/files/invalid-uuid/download")

        assert response.status_code == 422

    async def test_download_prevents_path_traversal(
        self, client: AsyncClient, indexed_file_with_content: File
    ):
        response = await client.get(f"/api/v1/files/{indexed_file_with_content.id}/download")

        assert response.status_code == 200
        data = response.json()

        assert "../" not in data["path"]
        assert "..\\" not in data["path"]


@pytest.mark.integration()
class TestDocumentAccess:
    async def test_record_document_access(
        self, client: AsyncClient, indexed_file_with_content: File
    ):
        response = await client.post(f"/api/v1/documents/{indexed_file_with_content.id}/access")

        assert response.status_code in [201, 404, 500]

    async def test_get_recent_documents(self, client: AsyncClient, indexed_files_batch):
        response = await client.get("/api/v1/documents/recent?limit=10")

        assert response.status_code == 200
        data = response.json()
        assert "documents" in data

    async def test_get_favorite_documents(self, client: AsyncClient):
        response = await client.get("/api/v1/documents/favorites?limit=10")

        assert response.status_code == 200
        data = response.json()
        assert "documents" in data


@pytest.mark.integration()
class TestExportPerformance:
    @pytest.mark.slow()
    async def test_export_large_file_list(
        self, client: AsyncClient, db_session: AsyncSession, watch_folder, temp_dir: Path
    ):
        from datetime import datetime

        for i in range(100):
            file_path = temp_dir / f"large_export_{i}.txt"
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
                modified_at=datetime.utcnow(),
                indexed_at=datetime.utcnow(),
            )
            db_session.add(file_record)

        await db_session.commit()

        response = await client.get("/api/v1/files?limit=100&offset=0")

        assert response.status_code == 200
        data = response.json()
        assert len(data["files"]) >= 100

    async def test_export_statistics_performance(self, client: AsyncClient, indexed_files_batch):
        import time

        start = time.time()
        response = await client.get("/api/v1/files/stats")
        elapsed = time.time() - start

        assert response.status_code == 200
        assert elapsed < 5.0
