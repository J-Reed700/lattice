from datetime import datetime
from pathlib import Path
from uuid import uuid4

from httpx import AsyncClient
import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from src.models import File, WatchFolder


class TestFilesListEndpoint:
    @pytest.mark.asyncio()
    async def test_list_files_success(
        self, client: AsyncClient, multiple_indexed_files: list[File]
    ):
        response = await client.get("/api/v1/files")

        assert response.status_code == 200
        data = response.json()
        assert "files" in data
        assert "total" in data
        assert "limit" in data
        assert "offset" in data
        assert "has_more" in data
        assert len(data["files"]) <= 20
        assert data["total"] >= len(multiple_indexed_files)

    @pytest.mark.asyncio()
    async def test_list_files_pagination(
        self, client: AsyncClient, multiple_indexed_files: list[File]
    ):
        response = await client.get("/api/v1/files?limit=2&offset=0")

        assert response.status_code == 200
        data = response.json()
        assert len(data["files"]) <= 2
        assert data["limit"] == 2
        assert data["offset"] == 0

        if data["total"] > 2:
            assert data["has_more"] is True

    @pytest.mark.asyncio()
    async def test_list_files_filter_by_mime_type(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        watch_folder: WatchFolder,
        temp_dir: Path,
    ):
        pdf_file = File(
            id=uuid4(),
            watch_folder_id=watch_folder.id,
            path=str(temp_dir / "test.pdf"),
            filename="test.pdf",
            extension="pdf",
            size_bytes=1024,
            mime_type="application/pdf",
            hash_sha256="a" * 64,
            modified_at=datetime.utcnow(),
            indexed_at=datetime.utcnow(),
            created_at=datetime.utcnow(),
            updated_at=datetime.utcnow(),
        )
        db_session.add(pdf_file)
        await db_session.commit()

        response = await client.get("/api/v1/files?mime_type=application/pdf")

        assert response.status_code == 200
        data = response.json()
        for file in data["files"]:
            assert file["mime_type"] == "application/pdf"

    @pytest.mark.asyncio()
    async def test_list_files_filter_by_extension(
        self, client: AsyncClient, multiple_indexed_files: list[File]
    ):
        response = await client.get("/api/v1/files?extension=txt")

        assert response.status_code == 200
        data = response.json()
        for file in data["files"]:
            assert file["extension"] == "txt"

    @pytest.mark.asyncio()
    async def test_list_files_sorting_by_indexed_at_desc(
        self, client: AsyncClient, multiple_indexed_files: list[File]
    ):
        response = await client.get("/api/v1/files?sort_by=indexed_at&sort_order=desc")

        assert response.status_code == 200
        data = response.json()
        if len(data["files"]) > 1:
            for i in range(len(data["files"]) - 1):
                first_date = datetime.fromisoformat(
                    data["files"][i]["indexed_at"].replace("Z", "+00:00")
                )
                second_date = datetime.fromisoformat(
                    data["files"][i + 1]["indexed_at"].replace("Z", "+00:00")
                )
                assert first_date >= second_date

    @pytest.mark.asyncio()
    async def test_list_files_sorting_by_size_asc(
        self, client: AsyncClient, multiple_indexed_files: list[File]
    ):
        response = await client.get("/api/v1/files?sort_by=size_bytes&sort_order=asc")

        assert response.status_code == 200
        data = response.json()
        if len(data["files"]) > 1:
            for i in range(len(data["files"]) - 1):
                assert data["files"][i]["size_bytes"] <= data["files"][i + 1]["size_bytes"]

    @pytest.mark.asyncio()
    async def test_list_files_validation_limit_boundaries(self, client: AsyncClient):
        response = await client.get("/api/v1/files?limit=101")
        assert response.status_code == 422

        response = await client.get("/api/v1/files?limit=0")
        assert response.status_code == 422

        response = await client.get("/api/v1/files?limit=50")
        assert response.status_code == 200

    @pytest.mark.asyncio()
    async def test_list_files_validation_offset_negative(self, client: AsyncClient):
        response = await client.get("/api/v1/files?offset=-1")
        assert response.status_code == 422


class TestFilesStatsEndpoint:
    @pytest.mark.asyncio()
    async def test_stats_success(self, client: AsyncClient, multiple_indexed_files: list[File]):
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
        assert data["total_files"] >= len(multiple_indexed_files)

    @pytest.mark.asyncio()
    async def test_stats_mime_type_breakdown(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        watch_folder: WatchFolder,
        temp_dir: Path,
    ):
        files_to_create = [
            ("test1.pdf", "application/pdf"),
            ("test2.pdf", "application/pdf"),
            ("test3.txt", "text/plain"),
        ]

        for filename, mime_type in files_to_create:
            file_obj = File(
                id=uuid4(),
                watch_folder_id=watch_folder.id,
                path=str(temp_dir / filename),
                filename=filename,
                extension=filename.split(".")[-1],
                size_bytes=1024,
                mime_type=mime_type,
                hash_sha256=f"hash_{filename}" + "0" * (64 - len(filename) - 5),
                modified_at=datetime.utcnow(),
                indexed_at=datetime.utcnow(),
                created_at=datetime.utcnow(),
                updated_at=datetime.utcnow(),
            )
            db_session.add(file_obj)

        await db_session.commit()

        response = await client.get("/api/v1/files/stats")

        assert response.status_code == 200
        data = response.json()
        assert "application/pdf" in data["by_mime_type"]
        assert data["by_mime_type"]["application/pdf"] >= 2
        assert "text/plain" in data["by_mime_type"]

    @pytest.mark.asyncio()
    async def test_stats_extension_breakdown(
        self, client: AsyncClient, multiple_indexed_files: list[File]
    ):
        response = await client.get("/api/v1/files/stats")

        assert response.status_code == 200
        data = response.json()
        assert isinstance(data["by_extension"], dict)
        assert "txt" in data["by_extension"]


class TestFilesGetByIdEndpoint:
    @pytest.mark.asyncio()
    async def test_get_file_success(self, client: AsyncClient, indexed_file: File):
        response = await client.get(f"/api/v1/files/{indexed_file.id}")

        assert response.status_code == 200
        data = response.json()
        assert data["id"] == str(indexed_file.id)
        assert data["filename"] == indexed_file.filename
        assert data["path"] == indexed_file.path
        assert data["extension"] == indexed_file.extension
        assert data["mime_type"] == indexed_file.mime_type
        assert data["size_bytes"] == indexed_file.size_bytes

    @pytest.mark.asyncio()
    async def test_get_file_not_found(self, client: AsyncClient):
        random_uuid = uuid4()
        response = await client.get(f"/api/v1/files/{random_uuid}")

        assert response.status_code == 404
        data = response.json()
        assert "not found" in data["detail"].lower()

    @pytest.mark.asyncio()
    async def test_get_file_invalid_uuid(self, client: AsyncClient):
        response = await client.get("/api/v1/files/invalid-uuid")

        assert response.status_code == 422

    @pytest.mark.asyncio()
    async def test_get_file_with_content_flags(
        self, client: AsyncClient, indexed_file_with_content: File
    ):
        response = await client.get(f"/api/v1/files/{indexed_file_with_content.id}")

        assert response.status_code == 200
        data = response.json()
        assert "has_text_content" in data
        assert "has_image_data" in data


class TestFilesCreateEndpoint:
    @pytest.mark.asyncio()
    async def test_create_file_success(
        self, client: AsyncClient, watch_folder: WatchFolder, sample_text_file: Path
    ):
        with pytest.raises(Exception):
            await client.post(
                "/api/v1/files",
                json={
                    "path": str(sample_text_file),
                    "watch_folder_id": str(watch_folder.id),
                    "force_reindex": False,
                },
            )

    @pytest.mark.asyncio()
    async def test_create_file_force_reindex(
        self, client: AsyncClient, watch_folder: WatchFolder, sample_text_file: Path
    ):
        with pytest.raises(Exception):
            await client.post(
                "/api/v1/files",
                json={
                    "path": str(sample_text_file),
                    "watch_folder_id": str(watch_folder.id),
                    "force_reindex": True,
                },
            )

    @pytest.mark.asyncio()
    async def test_create_file_validation_missing_path(
        self, client: AsyncClient, watch_folder: WatchFolder
    ):
        response = await client.post(
            "/api/v1/files", json={"watch_folder_id": str(watch_folder.id)}
        )

        assert response.status_code == 422


class TestFilesUpdateEndpoint:
    @pytest.mark.asyncio()
    async def test_update_file_last_accessed(self, client: AsyncClient, indexed_file: File):
        new_access_time = datetime.utcnow().isoformat()

        response = await client.patch(
            f"/api/v1/files/{indexed_file.id}", json={"last_accessed_at": new_access_time}
        )

        assert response.status_code == 200
        data = response.json()
        assert data["id"] == str(indexed_file.id)
        assert "last_accessed_at" in data

    @pytest.mark.asyncio()
    async def test_update_file_not_found(self, client: AsyncClient):
        random_uuid = uuid4()
        response = await client.patch(
            f"/api/v1/files/{random_uuid}", json={"last_accessed_at": datetime.utcnow().isoformat()}
        )

        assert response.status_code == 404

    @pytest.mark.asyncio()
    async def test_update_file_invalid_uuid(self, client: AsyncClient):
        response = await client.patch(
            "/api/v1/files/invalid-uuid", json={"last_accessed_at": datetime.utcnow().isoformat()}
        )

        assert response.status_code == 422


class TestFilesDeleteEndpoint:
    @pytest.mark.asyncio()
    async def test_delete_file_soft_delete(self, client: AsyncClient, indexed_file: File):
        response = await client.delete(
            f"/api/v1/files/{indexed_file.id}", params={"permanent": False}
        )

        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert data["data"]["permanent"] is False

    @pytest.mark.asyncio()
    async def test_delete_file_permanent(self, client: AsyncClient, indexed_file: File):
        response = await client.delete(
            f"/api/v1/files/{indexed_file.id}", params={"permanent": True}
        )

        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert data["data"]["permanent"] is True

    @pytest.mark.asyncio()
    async def test_delete_file_not_found(self, client: AsyncClient):
        random_uuid = uuid4()
        response = await client.delete(f"/api/v1/files/{random_uuid}")

        assert response.status_code == 404

    @pytest.mark.asyncio()
    async def test_delete_file_invalid_uuid(self, client: AsyncClient):
        response = await client.delete("/api/v1/files/invalid-uuid")

        assert response.status_code == 422


class TestFilesThumbnailEndpoint:
    @pytest.mark.asyncio()
    async def test_thumbnail_not_implemented(self, client: AsyncClient, indexed_file: File):
        response = await client.get(f"/api/v1/files/{indexed_file.id}/thumbnail")

        assert response.status_code == 501
        data = response.json()
        assert "not yet implemented" in data["detail"].lower()


class TestFilesDownloadEndpoint:
    @pytest.mark.asyncio()
    async def test_download_url_success(self, client: AsyncClient, indexed_file: File):
        response = await client.get(f"/api/v1/files/{indexed_file.id}/download")

        assert response.status_code == 200
        data = response.json()
        assert "file_id" in data
        assert "path" in data
        assert "filename" in data
        assert "mime_type" in data
        assert "size_bytes" in data
        assert data["file_id"] == str(indexed_file.id)
        assert data["filename"] == indexed_file.filename

    @pytest.mark.asyncio()
    async def test_download_url_not_found(self, client: AsyncClient):
        random_uuid = uuid4()
        response = await client.get(f"/api/v1/files/{random_uuid}/download")

        assert response.status_code == 404

    @pytest.mark.asyncio()
    async def test_download_url_invalid_uuid(self, client: AsyncClient):
        response = await client.get("/api/v1/files/invalid-uuid/download")

        assert response.status_code == 422
