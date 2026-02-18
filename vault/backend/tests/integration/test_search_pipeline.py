from httpx import AsyncClient
import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from src.models import File, TextContent
from src.services.search.service import SearchService


@pytest.mark.integration()
class TestSearchPipelineEndToEnd:
    async def test_upload_and_search_text_file(
        self, client: AsyncClient, db_session: AsyncSession, indexed_file_with_content: File
    ):
        search_payload = {"query": "test document", "mode": "text", "limit": 10, "offset": 0}

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200
        data = response.json()
        assert "results" in data
        assert "total" in data
        assert data["mode"] == "text"

    async def test_vector_search_after_upload(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        indexed_file_with_content: File,
        mock_embedding_service,
        mocker,
    ):
        mocker.patch(
            "src.services.search.service.EmbeddingService", return_value=mock_embedding_service
        )

        search_payload = {
            "query": "semantic search functionality",
            "mode": "vector",
            "limit": 10,
            "offset": 0,
        }

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200
        data = response.json()
        assert data["mode"] == "vector"
        assert isinstance(data["results"], list)

    async def test_hybrid_search_after_upload(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        indexed_file_with_content: File,
        mock_embedding_service,
        mocker,
    ):
        mocker.patch(
            "src.services.search.service.EmbeddingService", return_value=mock_embedding_service
        )

        search_payload = {"query": "searchable text", "mode": "hybrid", "limit": 10, "offset": 0}

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200
        data = response.json()
        assert data["mode"] == "hybrid"


@pytest.mark.integration()
class TestSearchWithFilters:
    async def test_search_with_mime_type_filter(
        self, client: AsyncClient, db_session: AsyncSession, indexed_file_with_content: File
    ):
        search_payload = {
            "query": "test",
            "mode": "text",
            "limit": 10,
            "offset": 0,
            "filters": {"mime_types": ["text/plain"]},
        }

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200

    async def test_search_with_extension_filter(
        self, client: AsyncClient, db_session: AsyncSession, indexed_file_with_content: File
    ):
        search_payload = {
            "query": "test",
            "mode": "text",
            "limit": 10,
            "offset": 0,
            "filters": {"extensions": ["txt"]},
        }

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200

    async def test_search_with_date_range_filter(
        self, client: AsyncClient, db_session: AsyncSession, indexed_file_with_content: File
    ):
        from datetime import datetime, timedelta

        yesterday = (datetime.utcnow() - timedelta(days=1)).isoformat()
        tomorrow = (datetime.utcnow() + timedelta(days=1)).isoformat()

        search_payload = {
            "query": "test",
            "mode": "text",
            "limit": 10,
            "offset": 0,
            "filters": {"date_from": yesterday, "date_to": tomorrow},
        }

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200


@pytest.mark.integration()
class TestSearchPagination:
    async def test_search_pagination(
        self, client: AsyncClient, db_session: AsyncSession, multiple_indexed_files: list[File]
    ):
        for file in multiple_indexed_files:
            text_content = TextContent(
                file_id=file.id,
                content=f"Test content for {file.filename}",
                language="en",
                char_count=50,
                word_count=5,
                embedding=[0.1] * 384,
            )
            db_session.add(text_content)
        await db_session.commit()

        search_payload = {"query": "test", "mode": "text", "limit": 2, "offset": 0}

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200
        data = response.json()
        assert data["limit"] == 2
        assert data["offset"] == 0

        search_payload["offset"] = 2
        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200
        data = response.json()
        assert data["offset"] == 2


@pytest.mark.integration()
class TestSearchResultStructure:
    async def test_search_result_includes_metadata(
        self, client: AsyncClient, db_session: AsyncSession, indexed_file_with_content: File
    ):
        search_payload = {"query": "test", "mode": "text", "limit": 10, "offset": 0}

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200
        data = response.json()

        if data["total"] > 0:
            result = data["results"][0]
            assert "id" in result
            assert "filename" in result
            assert "file_path" in result
            assert "mime_type" in result
            assert "size_bytes" in result
            assert "score" in result

    async def test_search_result_includes_timestamps(
        self, client: AsyncClient, db_session: AsyncSession, indexed_file_with_content: File
    ):
        search_payload = {"query": "test", "mode": "text", "limit": 10, "offset": 0}

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200
        data = response.json()

        if data["total"] > 0:
            result = data["results"][0]
            assert "created_at" in result
            assert "modified_at" in result
            assert "indexed_at" in result


@pytest.mark.integration()
class TestSearchAccuracy:
    async def test_search_finds_exact_match(
        self, client: AsyncClient, db_session: AsyncSession, indexed_file_with_content: File
    ):
        search_payload = {"query": "searchable text", "mode": "text", "limit": 10, "offset": 0}

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200
        data = response.json()
        assert data["total"] >= 1

    async def test_search_returns_no_results_for_nonexistent(
        self, client: AsyncClient, db_session: AsyncSession, indexed_file_with_content: File
    ):
        search_payload = {
            "query": "nonexistent_content_xyz123",
            "mode": "text",
            "limit": 10,
            "offset": 0,
        }

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200
        data = response.json()
        assert data["total"] == 0


@pytest.mark.integration()
class TestSearchErrorHandling:
    async def test_search_with_empty_query(self, client: AsyncClient):
        search_payload = {"query": "", "mode": "text", "limit": 10, "offset": 0}

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200
        data = response.json()
        assert data["total"] == 0

    async def test_search_with_invalid_mode(self, client: AsyncClient):
        search_payload = {"query": "test", "mode": "invalid_mode", "limit": 10, "offset": 0}

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 422

    async def test_search_with_invalid_limit(self, client: AsyncClient):
        search_payload = {"query": "test", "mode": "text", "limit": -1, "offset": 0}

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 422

    async def test_search_with_invalid_offset(self, client: AsyncClient):
        search_payload = {"query": "test", "mode": "text", "limit": 10, "offset": -1}

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 422


@pytest.mark.integration()
class TestSearchPerformance:
    async def test_search_response_time(
        self, client: AsyncClient, db_session: AsyncSession, indexed_file_with_content: File
    ):
        search_payload = {"query": "test", "mode": "text", "limit": 10, "offset": 0}

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200
        data = response.json()
        assert "took_ms" in data
        assert data["took_ms"] >= 0

    @pytest.mark.slow()
    async def test_search_with_large_result_set(
        self, client: AsyncClient, db_session: AsyncSession, multiple_indexed_files: list[File]
    ):
        for file in multiple_indexed_files:
            text_content = TextContent(
                file_id=file.id,
                content="Common test content for searching",
                language="en",
                char_count=50,
                word_count=5,
                embedding=[0.1] * 384,
            )
            db_session.add(text_content)
        await db_session.commit()

        search_payload = {"query": "common test", "mode": "text", "limit": 100, "offset": 0}

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200
        data = response.json()
        assert data["took_ms"] < 5000


@pytest.mark.integration()
class TestSearchServiceDirect:
    async def test_search_service_text_mode(
        self, db_session: AsyncSession, indexed_file_with_content: File
    ):
        search_service = SearchService(db_session)

        results = await search_service.search(
            query="test document", mode="text", limit=10, offset=0
        )

        assert results is not None
        assert hasattr(results, "results")
        assert hasattr(results, "total")

    async def test_search_service_vector_mode(
        self, db_session: AsyncSession, indexed_file_with_content: File, mock_embedding_service
    ):
        search_service = SearchService(db_session, embedding_service=mock_embedding_service)

        results = await search_service.search(
            query="semantic search", mode="vector", limit=10, offset=0
        )

        assert results is not None

    async def test_search_service_hybrid_mode(
        self, db_session: AsyncSession, indexed_file_with_content: File, mock_embedding_service
    ):
        search_service = SearchService(db_session, embedding_service=mock_embedding_service)

        results = await search_service.search(query="searchable", mode="hybrid", limit=10, offset=0)

        assert results is not None


@pytest.mark.integration()
class TestSearchRanking:
    async def test_search_results_ordered_by_relevance(
        self, client: AsyncClient, db_session: AsyncSession, watch_folder, temp_dir
    ):
        from uuid import uuid4

        file1 = temp_dir / "highly_relevant.txt"
        file1.write_text("machine learning artificial intelligence")

        file2 = temp_dir / "less_relevant.txt"
        file2.write_text("the machine in the factory")

        for idx, file_path in enumerate([file1, file2]):
            file_record = File(
                id=uuid4(),
                watch_folder_id=watch_folder.id,
                path=str(file_path),
                filename=file_path.name,
                extension="txt",
                size_bytes=file_path.stat().st_size,
                mime_type="text/plain",
                hash_sha256=f"hash_{idx}" + "0" * 58,
                modified_at=datetime.utcnow(),
                indexed_at=datetime.utcnow(),
            )
            db_session.add(file_record)
            await db_session.commit()
            await db_session.refresh(file_record)

            text_content = TextContent(
                file_id=file_record.id,
                content=file_path.read_text(),
                language="en",
                char_count=len(file_path.read_text()),
                word_count=len(file_path.read_text().split()),
                embedding=[0.1] * 384,
            )
            db_session.add(text_content)

        await db_session.commit()

        search_payload = {"query": "machine learning", "mode": "text", "limit": 10, "offset": 0}

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200
        data = response.json()

        if len(data["results"]) >= 2:
            scores = [r["score"] for r in data["results"]]
            assert scores[0] >= scores[1]


@pytest.mark.integration()
class TestMultipleFileSearch:
    async def test_search_across_multiple_files(
        self, client: AsyncClient, db_session: AsyncSession, multiple_indexed_files: list[File]
    ):
        for idx, file in enumerate(multiple_indexed_files):
            text_content = TextContent(
                file_id=file.id,
                content=f"Test document {idx} with unique content",
                language="en",
                char_count=50,
                word_count=6,
                embedding=[0.1] * 384,
            )
            db_session.add(text_content)
        await db_session.commit()

        search_payload = {"query": "document", "mode": "text", "limit": 10, "offset": 0}

        response = await client.post("/api/v1/search", json=search_payload)
        assert response.status_code == 200
        data = response.json()
        assert data["total"] >= 1
