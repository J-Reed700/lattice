from httpx import AsyncClient
import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from src.models import File


@pytest.mark.integration()
class TestRerankingIntegration:
    async def test_search_with_reranking_enabled(
        self, client: AsyncClient, indexed_files_batch, mock_reranker, mocker
    ):
        mocker.patch("src.services.search.SearchService._get_reranker", return_value=mock_reranker)

        search_payload = {
            "query": "python programming",
            "mode": "text",
            "limit": 10,
            "offset": 0,
            "rerank": True,
        }

        response = await client.post("/api/v1/search", json=search_payload)

        assert response.status_code == 200
        data = response.json()
        assert "results" in data
        assert len(data["results"]) > 0

        mock_reranker.rerank.assert_called_once()

    async def test_search_with_reranking_disabled(
        self, client: AsyncClient, indexed_files_batch, mock_reranker, mocker
    ):
        mocker.patch("src.services.search.SearchService._get_reranker", return_value=mock_reranker)

        search_payload = {
            "query": "javascript guide",
            "mode": "text",
            "limit": 10,
            "offset": 0,
            "rerank": False,
        }

        response = await client.post("/api/v1/search", json=search_payload)

        assert response.status_code == 200
        data = response.json()
        assert "results" in data

        mock_reranker.rerank.assert_not_called()

    async def test_reranking_improves_relevance_scores(
        self, client: AsyncClient, indexed_files_batch, mock_reranker, mocker
    ):
        mocker.patch("src.services.search.SearchService._get_reranker", return_value=mock_reranker)

        without_rerank = {
            "query": "data science tools",
            "mode": "text",
            "limit": 10,
            "offset": 0,
            "rerank": False,
        }

        response1 = await client.post("/api/v1/search", json=without_rerank)
        data1 = response1.json()

        with_rerank = {
            "query": "data science tools",
            "mode": "text",
            "limit": 10,
            "offset": 0,
            "rerank": True,
        }

        response2 = await client.post("/api/v1/search", json=with_rerank)
        data2 = response2.json()

        assert response1.status_code == 200
        assert response2.status_code == 200

        if len(data1["results"]) > 0 and len(data2["results"]) > 0:
            original_score = data1["results"][0]["score"]
            reranked_score = data2["results"][0]["score"]

            assert reranked_score >= original_score * 1.0


@pytest.mark.integration()
class TestRerankingWithDifferentModes:
    async def test_vector_search_with_reranking(
        self,
        client: AsyncClient,
        indexed_files_batch,
        mock_embedding_service,
        mock_reranker,
        mocker,
    ):
        mocker.patch(
            "src.services.search.SearchService._get_embedding_service",
            return_value=mock_embedding_service,
        )
        mocker.patch("src.services.search.SearchService._get_reranker", return_value=mock_reranker)

        search_payload = {
            "query": "semantic search for documents",
            "mode": "vector",
            "limit": 10,
            "offset": 0,
            "rerank": True,
        }

        response = await client.post("/api/v1/search", json=search_payload)

        assert response.status_code == 200
        data = response.json()
        assert data["mode"] == "vector"
        assert "results" in data

    async def test_hybrid_search_with_reranking(
        self,
        client: AsyncClient,
        indexed_files_batch,
        mock_embedding_service,
        mock_reranker,
        mock_hybrid_engine,
        mocker,
    ):
        mocker.patch(
            "src.services.search.SearchService._get_embedding_service",
            return_value=mock_embedding_service,
        )
        mocker.patch("src.services.search.SearchService._get_reranker", return_value=mock_reranker)
        mocker.patch("src.api.dependencies.get_hybrid_engine", return_value=mock_hybrid_engine)

        search_payload = {
            "query": "hybrid search test",
            "mode": "hybrid",
            "limit": 10,
            "offset": 0,
            "rerank": True,
        }

        response = await client.post("/api/v1/search", json=search_payload)

        assert response.status_code == 200
        data = response.json()
        assert data["mode"] == "hybrid"


@pytest.mark.integration()
class TestRerankingErrorHandling:
    async def test_reranking_graceful_fallback_on_error(
        self, client: AsyncClient, indexed_files_batch, mocker
    ):
        from src.modules.reranker import Reranker

        mock_failing_reranker = mocker.Mock(spec=Reranker)
        mock_failing_reranker.rerank = mocker.AsyncMock(
            side_effect=Exception("Reranker service unavailable")
        )

        mocker.patch(
            "src.services.search.SearchService._get_reranker", return_value=mock_failing_reranker
        )

        search_payload = {
            "query": "test query",
            "mode": "text",
            "limit": 10,
            "offset": 0,
            "rerank": True,
        }

        response = await client.post("/api/v1/search", json=search_payload)

        assert response.status_code == 200
        data = response.json()
        assert "results" in data

    async def test_reranking_with_empty_results(self, client: AsyncClient, mock_reranker, mocker):
        mocker.patch("src.services.search.SearchService._get_reranker", return_value=mock_reranker)

        search_payload = {
            "query": "nonexistent_query_xyz123",
            "mode": "text",
            "limit": 10,
            "offset": 0,
            "rerank": True,
        }

        response = await client.post("/api/v1/search", json=search_payload)

        assert response.status_code == 200
        data = response.json()
        assert data["total"] == 0
        assert len(data["results"]) == 0


@pytest.mark.integration()
class TestRerankingPerformance:
    async def test_reranking_execution_time(
        self, client: AsyncClient, indexed_files_batch, mock_reranker, mocker
    ):
        mocker.patch("src.services.search.SearchService._get_reranker", return_value=mock_reranker)

        search_payload = {
            "query": "performance test",
            "mode": "text",
            "limit": 10,
            "offset": 0,
            "rerank": True,
        }

        response = await client.post("/api/v1/search", json=search_payload)

        assert response.status_code == 200
        data = response.json()
        assert "took_ms" in data
        assert data["took_ms"] > 0

    @pytest.mark.slow()
    async def test_reranking_with_large_result_set(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        watch_folder,
        temp_dir,
        mock_reranker,
        mocker,
    ):
        from datetime import datetime
        from uuid import uuid4

        mocker.patch("src.services.search.SearchService._get_reranker", return_value=mock_reranker)

        for i in range(50):
            file_path = temp_dir / f"large_set_{i}.txt"
            file_path.write_text(f"Large result set content {i}")

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

        search_payload = {
            "query": "content",
            "mode": "text",
            "limit": 50,
            "offset": 0,
            "rerank": True,
        }

        response = await client.post("/api/v1/search", json=search_payload)

        assert response.status_code == 200
        data = response.json()
        assert "took_ms" in data
        assert data["took_ms"] < 10000


@pytest.mark.integration()
class TestRerankingWithFilters:
    async def test_reranking_respects_mime_type_filter(
        self, client: AsyncClient, indexed_files_batch, mock_reranker, mocker
    ):
        mocker.patch("src.services.search.SearchService._get_reranker", return_value=mock_reranker)

        search_payload = {
            "query": "test",
            "mode": "text",
            "limit": 10,
            "offset": 0,
            "rerank": True,
            "filters": {"mime_types": ["text/plain"]},
        }

        response = await client.post("/api/v1/search", json=search_payload)

        assert response.status_code == 200
        data = response.json()

        for result in data["results"]:
            assert result["mime_type"] == "text/plain"

    async def test_reranking_respects_extension_filter(
        self, client: AsyncClient, indexed_files_batch, mock_reranker, mocker
    ):
        mocker.patch("src.services.search.SearchService._get_reranker", return_value=mock_reranker)

        search_payload = {
            "query": "test",
            "mode": "text",
            "limit": 10,
            "offset": 0,
            "rerank": True,
            "filters": {"extensions": ["md"]},
        }

        response = await client.post("/api/v1/search", json=search_payload)

        assert response.status_code == 200
        data = response.json()

        for result in data["results"]:
            assert result["extension"] == "md"


@pytest.mark.integration()
class TestRerankingScoreOrdering:
    async def test_reranked_results_are_ordered_by_score(
        self, client: AsyncClient, indexed_files_batch, mock_reranker, mocker
    ):
        mocker.patch("src.services.search.SearchService._get_reranker", return_value=mock_reranker)

        search_payload = {
            "query": "programming language",
            "mode": "text",
            "limit": 10,
            "offset": 0,
            "rerank": True,
        }

        response = await client.post("/api/v1/search", json=search_payload)

        assert response.status_code == 200
        data = response.json()

        if len(data["results"]) > 1:
            scores = [result["score"] for result in data["results"]]
            assert scores == sorted(scores, reverse=True)

    async def test_reranking_preserves_top_results(
        self, client: AsyncClient, indexed_files_batch, mock_reranker, mocker
    ):
        mocker.patch("src.services.search.SearchService._get_reranker", return_value=mock_reranker)

        search_payload = {
            "query": "javascript",
            "mode": "text",
            "limit": 3,
            "offset": 0,
            "rerank": True,
        }

        response = await client.post("/api/v1/search", json=search_payload)

        assert response.status_code == 200
        data = response.json()
        assert len(data["results"]) <= 3


@pytest.mark.integration()
class TestRerankingConfiguration:
    async def test_reranking_with_custom_weights(
        self, client: AsyncClient, indexed_files_batch, mock_reranker, mocker
    ):
        mocker.patch("src.services.search.SearchService._get_reranker", return_value=mock_reranker)

        search_payload = {
            "query": "test query",
            "mode": "hybrid",
            "limit": 10,
            "offset": 0,
            "rerank": True,
            "semantic_weight": 0.7,
            "keyword_weight": 0.3,
        }

        response = await client.post("/api/v1/search", json=search_payload)

        assert response.status_code == 200
        data = response.json()
        assert "results" in data

    async def test_reranking_with_different_strategies(
        self, client: AsyncClient, indexed_files_batch, mock_reranker, mock_hybrid_engine, mocker
    ):
        mocker.patch("src.services.search.SearchService._get_reranker", return_value=mock_reranker)
        mocker.patch("src.api.dependencies.get_hybrid_engine", return_value=mock_hybrid_engine)

        strategies = ["rrf", "weighted"]

        for strategy in strategies:
            search_payload = {
                "query": "test",
                "mode": "hybrid",
                "limit": 10,
                "offset": 0,
                "rerank": True,
                "hybrid_strategy": strategy,
            }

            response = await client.post("/api/v1/search", json=search_payload)

            assert response.status_code == 200
            data = response.json()
            assert "results" in data
