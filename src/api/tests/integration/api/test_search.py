from unittest.mock import AsyncMock, Mock, patch
from uuid import uuid4

from httpx import AsyncClient
import pytest

from src.models import File


class TestSearchEndpoint:
    @pytest.mark.asyncio()
    async def test_search_vector_mode_success(
        self, client: AsyncClient, indexed_files_batch: list[File]
    ):
        with patch("src.api.routes.search.SearchService") as mock_search_service:
            mock_service_instance = AsyncMock()
            mock_service_instance.search = AsyncMock(return_value=[])
            mock_service_instance.count_results = AsyncMock(return_value=0)
            mock_search_service.return_value = mock_service_instance

            response = await client.post(
                "/api/v1/search", json={"query": "machine learning", "mode": "vector", "limit": 20}
            )

            assert response.status_code == 200
            data = response.json()
            assert "results" in data
            assert "total" in data
            assert "query" in data
            assert "mode" in data
            assert "took_ms" in data
            assert data["mode"] == "vector"
            assert data["query"] == "machine learning"

    @pytest.mark.asyncio()
    async def test_search_bm25_mode(self, client: AsyncClient):
        with patch("src.api.routes.search.get_bm25_engine") as mock_bm25_dep:
            mock_engine = AsyncMock()
            mock_engine.search = AsyncMock(
                return_value=[
                    {
                        "document_id": str(uuid4()),
                        "file_path": "/test/file1.txt",
                        "filename": "file1.txt",
                        "bm25_score": 2.5,
                    }
                ]
            )
            mock_bm25_dep.return_value = mock_engine

            response = await client.post(
                "/api/v1/search", json={"query": "test query", "mode": "bm25", "limit": 10}
            )

            assert response.status_code == 200
            data = response.json()
            assert data["mode"] == "bm25"

    @pytest.mark.asyncio()
    async def test_search_hybrid_mode(self, client: AsyncClient):
        with patch("src.api.routes.search.get_hybrid_engine") as mock_hybrid_dep, patch(
            "src.api.routes.search.get_settings"
        ) as mock_settings:
            mock_settings_obj = Mock()
            mock_settings_obj.hybrid_search_enabled = True
            mock_settings_obj.hybrid_default_strategy = "rrf"
            mock_settings.return_value = mock_settings_obj

            mock_engine = AsyncMock()
            mock_engine.search = AsyncMock(
                return_value=[
                    {
                        "file_id": str(uuid4()),
                        "file_path": "/test/file1.txt",
                        "filename": "file1.txt",
                        "score": 0.92,
                    }
                ]
            )
            mock_hybrid_dep.return_value = mock_engine

            response = await client.post(
                "/api/v1/search",
                json={"query": "hybrid search test", "mode": "hybrid", "limit": 15},
            )

            assert response.status_code == 200
            data = response.json()
            assert data["mode"] == "hybrid"
            assert "fusion_strategy" in data

    @pytest.mark.asyncio()
    async def test_search_with_pagination(self, client: AsyncClient):
        with patch("src.api.routes.search.SearchService") as mock_search_service:
            mock_service_instance = AsyncMock()
            mock_service_instance.search = AsyncMock(return_value=[])
            mock_service_instance.count_results = AsyncMock(return_value=100)
            mock_search_service.return_value = mock_service_instance

            response = await client.post(
                "/api/v1/search",
                json={"query": "test", "mode": "vector", "limit": 10, "offset": 20},
            )

            assert response.status_code == 200
            data = response.json()
            assert data["limit"] == 10
            assert data["offset"] == 20
            assert data["has_more"] is True

    @pytest.mark.asyncio()
    async def test_search_validation_empty_query(self, client: AsyncClient):
        response = await client.post("/api/v1/search", json={"query": "", "mode": "vector"})

        assert response.status_code == 422

    @pytest.mark.asyncio()
    async def test_search_validation_query_too_long(self, client: AsyncClient):
        long_query = "a" * 501

        response = await client.post("/api/v1/search", json={"query": long_query, "mode": "vector"})

        assert response.status_code == 422

    @pytest.mark.asyncio()
    async def test_search_validation_invalid_mode(self, client: AsyncClient):
        response = await client.post(
            "/api/v1/search", json={"query": "test", "mode": "invalid_mode"}
        )

        assert response.status_code == 422

    @pytest.mark.asyncio()
    async def test_search_validation_limit_boundaries(self, client: AsyncClient):
        with patch("src.api.routes.search.SearchService") as mock_search_service:
            mock_service_instance = AsyncMock()
            mock_service_instance.search = AsyncMock(return_value=[])
            mock_service_instance.count_results = AsyncMock(return_value=0)
            mock_search_service.return_value = mock_service_instance

            response = await client.post("/api/v1/search", json={"query": "test", "limit": 0})
            assert response.status_code == 422

            response = await client.post("/api/v1/search", json={"query": "test", "limit": 101})
            assert response.status_code == 422

            response = await client.post("/api/v1/search", json={"query": "test", "limit": 50})
            assert response.status_code == 200

    @pytest.mark.asyncio()
    async def test_search_with_filters(self, client: AsyncClient):
        with patch("src.api.routes.search.SearchService") as mock_search_service:
            mock_service_instance = AsyncMock()
            mock_service_instance.search = AsyncMock(return_value=[])
            mock_service_instance.count_results = AsyncMock(return_value=0)
            mock_search_service.return_value = mock_service_instance

            response = await client.post(
                "/api/v1/search",
                json={
                    "query": "test",
                    "mode": "vector",
                    "filters": {
                        "mime_types": ["application/pdf"],
                        "extensions": ["pdf"],
                        "min_size": 1024,
                        "max_size": 10485760,
                    },
                },
            )

            assert response.status_code == 200
            call_args = mock_service_instance.search.call_args
            assert call_args.kwargs["filters"] is not None

    @pytest.mark.asyncio()
    async def test_search_with_rerank(self, client: AsyncClient):
        with patch("src.api.routes.search.SearchService") as mock_search_service:
            mock_service_instance = AsyncMock()
            mock_service_instance.search = AsyncMock(return_value=[])
            mock_service_instance.count_results = AsyncMock(return_value=0)
            mock_search_service.return_value = mock_service_instance

            response = await client.post(
                "/api/v1/search", json={"query": "test", "mode": "vector", "rerank": True}
            )

            assert response.status_code == 200
            call_args = mock_service_instance.search.call_args
            assert call_args.kwargs["rerank"] is True

    @pytest.mark.asyncio()
    async def test_search_with_custom_weights(self, client: AsyncClient):
        with patch("src.api.routes.search.SearchService") as mock_search_service:
            mock_service_instance = AsyncMock()
            mock_service_instance.search = AsyncMock(return_value=[])
            mock_service_instance.count_results = AsyncMock(return_value=0)
            mock_search_service.return_value = mock_service_instance

            response = await client.post(
                "/api/v1/search",
                json={
                    "query": "test",
                    "mode": "vector",
                    "semantic_weight": 0.8,
                    "keyword_weight": 0.2,
                },
            )

            assert response.status_code == 200

    @pytest.mark.asyncio()
    async def test_search_weights_validation_sum_not_one(self, client: AsyncClient):
        response = await client.post(
            "/api/v1/search",
            json={"query": "test", "mode": "hybrid", "semantic_weight": 0.5, "keyword_weight": 0.3},
        )

        assert response.status_code == 422

    @pytest.mark.asyncio()
    async def test_search_hybrid_strategy_rrf(self, client: AsyncClient):
        with patch("src.api.routes.search.get_hybrid_engine") as mock_hybrid_dep, patch(
            "src.api.routes.search.get_settings"
        ) as mock_settings:
            mock_settings_obj = Mock()
            mock_settings_obj.hybrid_search_enabled = True
            mock_settings_obj.hybrid_default_strategy = "rrf"
            mock_settings.return_value = mock_settings_obj

            mock_engine = AsyncMock()
            mock_engine.search = AsyncMock(return_value=[])
            mock_hybrid_dep.return_value = mock_engine

            response = await client.post(
                "/api/v1/search", json={"query": "test", "mode": "hybrid", "hybrid_strategy": "rrf"}
            )

            assert response.status_code == 200
            call_args = mock_engine.search.call_args
            assert call_args.kwargs["strategy"] == "rrf"

    @pytest.mark.asyncio()
    async def test_search_hybrid_strategy_weighted(self, client: AsyncClient):
        with patch("src.api.routes.search.get_hybrid_engine") as mock_hybrid_dep, patch(
            "src.api.routes.search.get_settings"
        ) as mock_settings:
            mock_settings_obj = Mock()
            mock_settings_obj.hybrid_search_enabled = True
            mock_settings_obj.hybrid_default_strategy = "weighted"
            mock_settings.return_value = mock_settings_obj

            mock_engine = AsyncMock()
            mock_engine.search = AsyncMock(return_value=[])
            mock_hybrid_dep.return_value = mock_engine

            response = await client.post(
                "/api/v1/search",
                json={"query": "test", "mode": "hybrid", "hybrid_strategy": "weighted"},
            )

            assert response.status_code == 200
            call_args = mock_engine.search.call_args
            assert call_args.kwargs["strategy"] == "weighted"

    @pytest.mark.asyncio()
    async def test_search_records_history(self, client: AsyncClient):
        with patch("src.api.routes.search.SearchService") as mock_search_service, patch(
            "src.api.routes.search.SearchHistoryService"
        ) as mock_history_service:
            mock_service_instance = AsyncMock()
            mock_service_instance.search = AsyncMock(return_value=[])
            mock_service_instance.count_results = AsyncMock(return_value=0)
            mock_search_service.return_value = mock_service_instance

            mock_history_instance = AsyncMock()
            mock_history_instance.record_search = AsyncMock()
            mock_history_service.return_value = mock_history_instance

            response = await client.post("/api/v1/search", json={"query": "test query"})

            assert response.status_code == 200
            mock_history_instance.record_search.assert_called_once()

    @pytest.mark.asyncio()
    async def test_search_error_handling(self, client: AsyncClient):
        with patch("src.api.routes.search.SearchService") as mock_search_service:
            mock_service_instance = AsyncMock()
            mock_service_instance.search = AsyncMock(side_effect=Exception("Database error"))
            mock_search_service.return_value = mock_service_instance

            response = await client.post("/api/v1/search", json={"query": "test"})

            assert response.status_code == 500
            data = response.json()
            assert "Search failed" in data["detail"]

    @pytest.mark.asyncio()
    async def test_search_observability_tracing(self, client: AsyncClient):
        with patch("src.api.routes.search.SearchService") as mock_search_service, patch(
            "src.api.routes.search.tracer"
        ) as mock_tracer:
            mock_service_instance = AsyncMock()
            mock_service_instance.search = AsyncMock(return_value=[])
            mock_service_instance.count_results = AsyncMock(return_value=0)
            mock_search_service.return_value = mock_service_instance

            response = await client.post("/api/v1/search", json={"query": "test", "mode": "vector"})

            assert response.status_code == 200
            mock_tracer.start_as_current_span.assert_called()


class TestSearchSuggestionsEndpoint:
    @pytest.mark.asyncio()
    async def test_suggestions_success(self, client: AsyncClient):
        with patch("src.api.routes.search.SearchHistoryService") as mock_history_service:
            mock_service_instance = AsyncMock()
            mock_service_instance.get_recent_queries = AsyncMock(
                return_value=["machine learning", "python tutorial", "data science"]
            )
            mock_service_instance.get_popular_queries = AsyncMock(
                return_value=["ai research", "neural networks"]
            )
            mock_history_service.return_value = mock_service_instance

            response = await client.get("/api/v1/search/suggestions")

            assert response.status_code == 200
            data = response.json()
            assert "recent" in data
            assert "popular" in data
            assert len(data["recent"]) == 3
            assert len(data["popular"]) == 2
            assert "machine learning" in data["recent"]
            assert "ai research" in data["popular"]

    @pytest.mark.asyncio()
    async def test_suggestions_empty_results(self, client: AsyncClient):
        with patch("src.api.routes.search.SearchHistoryService") as mock_history_service:
            mock_service_instance = AsyncMock()
            mock_service_instance.get_recent_queries = AsyncMock(return_value=[])
            mock_service_instance.get_popular_queries = AsyncMock(return_value=[])
            mock_history_service.return_value = mock_service_instance

            response = await client.get("/api/v1/search/suggestions")

            assert response.status_code == 200
            data = response.json()
            assert data["recent"] == []
            assert data["popular"] == []

    @pytest.mark.asyncio()
    async def test_suggestions_error_handling(self, client: AsyncClient):
        with patch("src.api.routes.search.SearchHistoryService") as mock_history_service:
            mock_service_instance = AsyncMock()
            mock_service_instance.get_recent_queries = AsyncMock(
                side_effect=Exception("Database error")
            )
            mock_history_service.return_value = mock_service_instance

            response = await client.get("/api/v1/search/suggestions")

            assert response.status_code == 200
            data = response.json()
            assert data["recent"] == []
            assert data["popular"] == []

    @pytest.mark.asyncio()
    async def test_suggestions_default_limit(self, client: AsyncClient):
        with patch("src.api.routes.search.SearchHistoryService") as mock_history_service:
            mock_service_instance = AsyncMock()
            mock_service_instance.get_recent_queries = AsyncMock(return_value=[])
            mock_service_instance.get_popular_queries = AsyncMock(return_value=[])
            mock_history_service.return_value = mock_service_instance

            response = await client.get("/api/v1/search/suggestions")

            assert response.status_code == 200
            mock_service_instance.get_recent_queries.assert_called_with(limit=5)
            mock_service_instance.get_popular_queries.assert_called_with(limit=5)


class TestSearchHistoryEndpoint:
    @pytest.mark.asyncio()
    async def test_history_success(self, client: AsyncClient):
        response = await client.get("/api/v1/search/history")

        assert response.status_code == 200
        data = response.json()
        assert "queries" in data
        assert "total" in data
        assert "limit" in data
        assert "offset" in data
        assert isinstance(data["queries"], list)

    @pytest.mark.asyncio()
    async def test_history_with_pagination(self, client: AsyncClient):
        response = await client.get("/api/v1/search/history?limit=10&offset=5")

        assert response.status_code == 200
        data = response.json()
        assert data["limit"] == 10
        assert data["offset"] == 5

    @pytest.mark.asyncio()
    async def test_history_default_values(self, client: AsyncClient):
        response = await client.get("/api/v1/search/history")

        assert response.status_code == 200
        data = response.json()
        assert data["limit"] == 20
        assert data["offset"] == 0

    @pytest.mark.asyncio()
    async def test_history_custom_limit(self, client: AsyncClient):
        response = await client.get("/api/v1/search/history?limit=50")

        assert response.status_code == 200
        data = response.json()
        assert data["limit"] == 50

    @pytest.mark.asyncio()
    async def test_history_empty_results(self, client: AsyncClient):
        response = await client.get("/api/v1/search/history")

        assert response.status_code == 200
        data = response.json()
        assert data["total"] == 0
        assert data["queries"] == []
