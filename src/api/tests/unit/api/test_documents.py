from datetime import UTC
from uuid import uuid4

from httpx import AsyncClient
import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from src.models import File


@pytest.mark.unit()
class TestListFiles:
    async def test_list_files_empty(self, client: AsyncClient):
        response = await client.get("/api/v1/files")
        assert response.status_code == 200
        data = response.json()
        assert data["total"] == 0
        assert data["files"] == []
        assert data["limit"] == 20
        assert data["offset"] == 0
        assert data["has_more"] is False

    async def test_list_files_with_data(
        self, client: AsyncClient, multiple_indexed_files: list[File]
    ):
        response = await client.get("/api/v1/files")
        assert response.status_code == 200
        data = response.json()
        assert data["total"] == 5
        assert len(data["files"]) == 5
        assert data["has_more"] is False

    async def test_list_files_pagination(
        self, client: AsyncClient, multiple_indexed_files: list[File]
    ):
        response = await client.get("/api/v1/files?limit=2&offset=0")
        assert response.status_code == 200
        data = response.json()
        assert data["total"] == 5
        assert len(data["files"]) == 2
        assert data["has_more"] is True
        assert data["limit"] == 2
        assert data["offset"] == 0

        response = await client.get("/api/v1/files?limit=2&offset=2")
        assert response.status_code == 200
        data = response.json()
        assert len(data["files"]) == 2
        assert data["has_more"] is True

        response = await client.get("/api/v1/files?limit=2&offset=4")
        assert response.status_code == 200
        data = response.json()
        assert len(data["files"]) == 1
        assert data["has_more"] is False

    async def test_list_files_filter_by_mime_type(
        self, client: AsyncClient, multiple_indexed_files: list[File]
    ):
        response = await client.get("/api/v1/files?mime_type=text/plain")
        assert response.status_code == 200
        data = response.json()
        assert data["total"] == 5
        for file in data["files"]:
            assert file["mime_type"] == "text/plain"

    async def test_list_files_filter_by_extension(
        self, client: AsyncClient, multiple_indexed_files: list[File]
    ):
        response = await client.get("/api/v1/files?extension=txt")
        assert response.status_code == 200
        data = response.json()
        assert data["total"] == 5
        for file in data["files"]:
            assert file["extension"] == "txt"

    async def test_list_files_sort_by_filename_asc(
        self, client: AsyncClient, multiple_indexed_files: list[File]
    ):
        response = await client.get("/api/v1/files?sort_by=filename&sort_order=asc")
        assert response.status_code == 200
        data = response.json()
        filenames = [f["filename"] for f in data["files"]]
        assert filenames == sorted(filenames)

    async def test_list_files_sort_by_filename_desc(
        self, client: AsyncClient, multiple_indexed_files: list[File]
    ):
        response = await client.get("/api/v1/files?sort_by=filename&sort_order=desc")
        assert response.status_code == 200
        data = response.json()
        filenames = [f["filename"] for f in data["files"]]
        assert filenames == sorted(filenames, reverse=True)

    async def test_list_files_invalid_limit(self, client: AsyncClient):
        response = await client.get("/api/v1/files?limit=0")
        assert response.status_code == 422

        response = await client.get("/api/v1/files?limit=101")
        assert response.status_code == 422

    async def test_list_files_invalid_offset(self, client: AsyncClient):
        response = await client.get("/api/v1/files?offset=-1")
        assert response.status_code == 422


@pytest.mark.unit()
class TestGetFile:
    async def test_get_file_success(self, client: AsyncClient, indexed_file: File):
        response = await client.get(f"/api/v1/files/{indexed_file.id}")
        assert response.status_code == 200
        data = response.json()
        assert data["id"] == str(indexed_file.id)
        assert data["filename"] == indexed_file.filename
        assert data["extension"] == indexed_file.extension
        assert data["mime_type"] == indexed_file.mime_type
        assert data["size_bytes"] == indexed_file.size_bytes

    async def test_get_file_not_found(self, client: AsyncClient):
        random_id = uuid4()
        response = await client.get(f"/api/v1/files/{random_id}")
        assert response.status_code == 404
        assert "not found" in response.json()["detail"].lower()

    async def test_get_file_invalid_uuid(self, client: AsyncClient):
        response = await client.get("/api/v1/files/invalid-uuid")
        assert response.status_code == 422

    async def test_get_file_with_text_content(
        self, client: AsyncClient, indexed_file_with_content: File
    ):
        response = await client.get(f"/api/v1/files/{indexed_file_with_content.id}")
        assert response.status_code == 200
        data = response.json()
        assert data["has_text_content"] is True


@pytest.mark.unit()
class TestCreateFile:
    async def test_create_file_missing_path(self, client: AsyncClient, watch_folder):
        payload = {"watch_folder_id": str(watch_folder.id), "force_reindex": False}
        response = await client.post("/api/v1/files", json=payload)
        assert response.status_code == 422

    async def test_create_file_missing_watch_folder_id(self, client: AsyncClient, sample_text_file):
        payload = {"path": str(sample_text_file), "force_reindex": False}
        response = await client.post("/api/v1/files", json=payload)
        assert response.status_code == 422

    async def test_create_file_invalid_watch_folder_id(self, client: AsyncClient, sample_text_file):
        payload = {
            "watch_folder_id": "invalid-uuid",
            "path": str(sample_text_file),
            "force_reindex": False,
        }
        response = await client.post("/api/v1/files", json=payload)
        assert response.status_code == 422


@pytest.mark.unit()
class TestUpdateFile:
    async def test_update_file_last_accessed(self, client: AsyncClient, indexed_file: File):
        from datetime import datetime

        new_time = datetime.now(UTC).isoformat()
        payload = {"last_accessed_at": new_time}

        response = await client.patch(f"/api/v1/files/{indexed_file.id}", json=payload)
        assert response.status_code == 200
        data = response.json()
        assert data["last_accessed_at"] is not None

    async def test_update_file_not_found(self, client: AsyncClient):
        from datetime import datetime

        random_id = uuid4()
        payload = {"last_accessed_at": datetime.now(UTC).isoformat()}

        response = await client.patch(f"/api/v1/files/{random_id}", json=payload)
        assert response.status_code == 404

    async def test_update_file_invalid_uuid(self, client: AsyncClient):
        payload = {"last_accessed_at": "2024-01-01T00:00:00Z"}
        response = await client.patch("/api/v1/files/invalid-uuid", json=payload)
        assert response.status_code == 422


@pytest.mark.unit()
class TestDeleteFile:
    async def test_delete_file_soft_delete(
        self, client: AsyncClient, indexed_file: File, db_session: AsyncSession
    ):
        file_id = indexed_file.id

        response = await client.delete(f"/api/v1/files/{file_id}")
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert data["data"]["permanent"] is False

    async def test_delete_file_permanent(
        self, client: AsyncClient, indexed_file: File, db_session: AsyncSession
    ):
        file_id = indexed_file.id

        response = await client.delete(f"/api/v1/files/{file_id}?permanent=true")
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert data["data"]["permanent"] is True

        response = await client.get(f"/api/v1/files/{file_id}")
        assert response.status_code == 404

    async def test_delete_file_not_found(self, client: AsyncClient):
        random_id = uuid4()
        response = await client.delete(f"/api/v1/files/{random_id}")
        assert response.status_code == 404

    async def test_delete_file_invalid_uuid(self, client: AsyncClient):
        response = await client.delete("/api/v1/files/invalid-uuid")
        assert response.status_code == 422


@pytest.mark.unit()
class TestGetDownloadUrl:
    async def test_get_download_url_success(self, client: AsyncClient, indexed_file: File):
        response = await client.get(f"/api/v1/files/{indexed_file.id}/download")
        assert response.status_code == 200
        data = response.json()
        assert data["file_id"] == str(indexed_file.id)
        assert data["path"] == indexed_file.path
        assert data["filename"] == indexed_file.filename
        assert data["mime_type"] == indexed_file.mime_type
        assert data["size_bytes"] == indexed_file.size_bytes

    async def test_get_download_url_not_found(self, client: AsyncClient):
        random_id = uuid4()
        response = await client.get(f"/api/v1/files/{random_id}/download")
        assert response.status_code == 404

    async def test_get_download_url_invalid_uuid(self, client: AsyncClient):
        response = await client.get("/api/v1/files/invalid-uuid/download")
        assert response.status_code == 422


@pytest.mark.unit()
class TestGetFileStats:
    async def test_get_file_stats_empty(self, client: AsyncClient):
        response = await client.get("/api/v1/files/stats")
        assert response.status_code == 200
        data = response.json()
        assert data["total_files"] == 0
        assert data["total_size_bytes"] == 0
        assert data["indexed_today"] == 0
        assert data["indexed_this_week"] == 0
        assert data["indexed_this_month"] == 0

    async def test_get_file_stats_with_data(
        self, client: AsyncClient, multiple_indexed_files: list[File]
    ):
        response = await client.get("/api/v1/files/stats")
        assert response.status_code == 200
        data = response.json()
        assert data["total_files"] == 5
        assert data["total_size_bytes"] > 0
        assert data["indexed_today"] == 5
        assert data["indexed_this_week"] == 5
        assert data["indexed_this_month"] == 5

    async def test_get_file_stats_by_mime_type(
        self, client: AsyncClient, multiple_indexed_files: list[File]
    ):
        response = await client.get("/api/v1/files/stats")
        assert response.status_code == 200
        data = response.json()
        assert "text/plain" in data["by_mime_type"]
        assert data["by_mime_type"]["text/plain"] == 5

    async def test_get_file_stats_by_extension(
        self, client: AsyncClient, multiple_indexed_files: list[File]
    ):
        response = await client.get("/api/v1/files/stats")
        assert response.status_code == 200
        data = response.json()
        assert "txt" in data["by_extension"]
        assert data["by_extension"]["txt"] == 5
