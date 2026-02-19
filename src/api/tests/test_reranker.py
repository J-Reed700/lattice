import asyncio
import time
from unittest.mock import Mock, patch

import pytest

from src.modules.reranker.model import CrossEncoderModel
from src.modules.reranker.service import RerankService
from src.modules.search_engine.types import SearchResult


class TestCrossEncoderModel:
    """Test suite for CrossEncoderModel."""

    def test_model_initialization(self):
        model = CrossEncoderModel(
            model_name="cross-encoder/ms-marco-MiniLM-L-6-v2",
            device="cpu",
            max_length=512,
            cache_enabled=True,
            cache_ttl=3600,
        )

        assert model.model_name == "cross-encoder/ms-marco-MiniLM-L-6-v2"
        assert model.device == "cpu"
        assert model.max_length == 512
        assert model.cache_enabled is True
        assert model._model is None

    def test_cache_key_generation(self):
        model = CrossEncoderModel()

        query = "test query"
        documents = ["doc1", "doc2", "doc3"]

        key1 = model._get_cache_key(query, documents)
        key2 = model._get_cache_key(query, documents)
        key3 = model._get_cache_key("different", documents)

        assert key1 == key2
        assert key1 != key3
        assert isinstance(key1, str)
        assert len(key1) == 32

    def test_cache_cleaning(self):
        model = CrossEncoderModel(cache_ttl=1)

        model._score_cache["key1"] = ([0.5, 0.6], time.time() - 2)
        model._score_cache["key2"] = ([0.7, 0.8], time.time())

        model._clean_cache()

        assert "key1" not in model._score_cache
        assert "key2" in model._score_cache

    @patch("src.modules.reranker.model.CrossEncoder")
    def test_score_pairs_without_cache(self, mock_cross_encoder):
        mock_model = Mock()
        mock_model.predict.return_value = [1.0, 2.0, 3.0]
        mock_cross_encoder.return_value = mock_model

        model = CrossEncoderModel(cache_enabled=False)

        query = "find relevant documents"
        documents = ["doc1", "doc2", "doc3"]

        scores = model.score_pairs(query, documents, batch_size=2)

        assert len(scores) == 3
        assert all(0 <= s <= 1 for s in scores)
        mock_model.predict.assert_called_once()

    @patch("src.modules.reranker.model.CrossEncoder")
    def test_score_pairs_with_cache(self, mock_cross_encoder):
        mock_model = Mock()
        mock_model.predict.return_value = [1.0, 2.0]
        mock_cross_encoder.return_value = mock_model

        model = CrossEncoderModel(cache_enabled=True)

        query = "cached query"
        documents = ["doc1", "doc2"]

        scores1 = model.score_pairs(query, documents)
        scores2 = model.score_pairs(query, documents)

        assert scores1 == scores2
        assert mock_model.predict.call_count == 1

    def test_get_model_info(self):
        model = CrossEncoderModel(
            model_name="BAAI/bge-reranker-v2-m3", device="cpu", cache_enabled=True
        )

        info = model.get_model_info()

        assert info["model_name"] == "BAAI/bge-reranker-v2-m3"
        assert info["device"] == "cpu"
        assert info["cache_enabled"] is True
        assert info["cache_size"] == 0
        assert info["is_loaded"] is False


class TestRerankService:
    """Test suite for RerankService."""

    def test_service_initialization(self):
        service = RerankService(
            model_name="cross-encoder/ms-marco-MiniLM-L-6-v2",
            device="cpu",
            max_content_length=1500,
            batch_size=16,
            cache_enabled=True,
            cache_ttl=1800,
        )

        assert service.model_name == "cross-encoder/ms-marco-MiniLM-L-6-v2"
        assert service.device == "cpu"
        assert service.max_content_length == 1500
        assert service.batch_size == 16
        assert service.cache_enabled is True
        assert service.cache_ttl == 1800

    def test_singleton_model_per_name(self):
        service1 = RerankService(model_name="model_a")
        service2 = RerankService(model_name="model_a")
        service3 = RerankService(model_name="model_b")

        model1 = service1._get_model()
        model2 = service2._get_model()
        model3 = service3._get_model()

        assert model1 is model2
        assert model1 is not model3

    @pytest.mark.asyncio()
    async def test_load_document_content_success(self, tmp_path):
        test_file = tmp_path / "test.txt"
        test_content = "This is test content for reranking."
        test_file.write_text(test_content)

        service = RerankService()
        content = await service._load_document_content(str(test_file))

        assert content == test_content

    @pytest.mark.asyncio()
    async def test_load_document_content_failure(self):
        service = RerankService()
        content = await service._load_document_content("/nonexistent/file.txt")

        assert content == ""

    @pytest.mark.asyncio()
    async def test_load_document_content_truncation(self, tmp_path):
        test_file = tmp_path / "long.txt"
        test_content = "x" * 5000
        test_file.write_text(test_content)

        service = RerankService(max_content_length=1000)
        content = await service._load_document_content(str(test_file))

        assert len(content) == 1000

    @pytest.mark.asyncio()
    @patch("src.modules.reranker.service.RerankService._load_document_content")
    @patch("src.modules.reranker.model.CrossEncoderModel.score_pairs")
    async def test_rerank_batch(self, mock_score_pairs, mock_load_content):
        mock_load_content.return_value = "document content"
        mock_score_pairs.return_value = [0.9, 0.7, 0.8]

        service = RerankService()

        results = [
            SearchResult(file_id="1", file_path="/path1", score=0.5, metadata={}),
            SearchResult(file_id="2", file_path="/path2", score=0.6, metadata={}),
            SearchResult(file_id="3", file_path="/path3", score=0.4, metadata={}),
        ]

        reranked = await service._rerank_batch(query="test query", results=results, top_k=2)

        assert len(reranked) == 2
        assert reranked[0].file_id == "1"
        assert reranked[0].score == 0.9
        assert reranked[1].file_id == "3"
        assert reranked[1].score == 0.8

    @pytest.mark.asyncio()
    @patch("src.modules.reranker.service.RerankService._rerank_batch")
    async def test_rerank_with_timeout(self, mock_rerank_batch):
        async def slow_rerank(*args, **kwargs):
            await asyncio.sleep(2)
            return []

        mock_rerank_batch.side_effect = slow_rerank

        service = RerankService()
        results = [SearchResult(file_id="1", file_path="/path1", score=0.5, metadata={})]

        reranked = await service.rerank(query="test", results=results, top_k=10, timeout=0.1)

        assert reranked == results[:10]

    @pytest.mark.asyncio()
    async def test_rerank_empty_results(self):
        service = RerankService()

        reranked = await service.rerank(query="test", results=[], top_k=10)

        assert reranked == []

    def test_get_model_info_multiple_models(self):
        RerankService._instances.clear()

        service1 = RerankService(model_name="model_a")
        service2 = RerankService(model_name="model_b")

        service1._get_model()
        service2._get_model()

        info = RerankService.get_model_info()

        assert "model_a" in info
        assert "model_b" in info
        assert info["model_a"]["model_name"] == "model_a"
        assert info["model_b"]["model_name"] == "model_b"


class TestRerankingBenchmarks:
    """Benchmark tests for reranking performance."""

    @pytest.mark.benchmark()
    @pytest.mark.asyncio()
    @patch("src.modules.reranker.model.CrossEncoder")
    async def test_benchmark_small_result_set(self, mock_cross_encoder):
        mock_model = Mock()
        mock_model.predict.return_value = [float(i) for i in range(10)]
        mock_cross_encoder.return_value = mock_model

        service = RerankService(model_name="cross-encoder/ms-marco-MiniLM-L-6-v2", batch_size=32)

        results = [
            SearchResult(file_id=str(i), file_path=f"/path{i}", score=0.5, metadata={})
            for i in range(10)
        ]

        with patch.object(service, "_load_document_content", return_value="test content"):
            start = time.time()
            reranked = await service._rerank_batch("test query", results, top_k=10)
            duration = time.time() - start

        assert len(reranked) == 10
        assert duration < 1.0

    @pytest.mark.benchmark()
    @pytest.mark.asyncio()
    @patch("src.modules.reranker.model.CrossEncoder")
    async def test_benchmark_large_result_set(self, mock_cross_encoder):
        mock_model = Mock()
        mock_model.predict.return_value = [float(i) for i in range(100)]
        mock_cross_encoder.return_value = mock_model

        service = RerankService(model_name="BAAI/bge-reranker-v2-m3", batch_size=32)

        results = [
            SearchResult(file_id=str(i), file_path=f"/path{i}", score=0.5, metadata={})
            for i in range(100)
        ]

        with patch.object(service, "_load_document_content", return_value="test content"):
            start = time.time()
            reranked = await service._rerank_batch("test query", results, top_k=50)
            duration = time.time() - start

        assert len(reranked) == 50
        assert duration < 5.0

    @pytest.mark.benchmark()
    @pytest.mark.asyncio()
    @patch("src.modules.reranker.model.CrossEncoder")
    async def test_benchmark_cache_performance(self, mock_cross_encoder):
        mock_model = Mock()
        mock_model.predict.return_value = [float(i) for i in range(20)]
        mock_cross_encoder.return_value = mock_model

        service = RerankService(cache_enabled=True)

        results = [
            SearchResult(file_id=str(i), file_path=f"/path{i}", score=0.5, metadata={})
            for i in range(20)
        ]

        query = "benchmark cache test"

        with patch.object(service, "_load_document_content", return_value="test"):
            start1 = time.time()
            await service._rerank_batch(query, results, top_k=20)
            duration1 = time.time() - start1

            start2 = time.time()
            await service._rerank_batch(query, results, top_k=20)
            duration2 = time.time() - start2

        assert duration2 < duration1


class TestRerankingPrecision:
    """Tests for reranking precision improvements."""

    @pytest.mark.asyncio()
    @patch("src.modules.reranker.model.CrossEncoder")
    async def test_precision_improvement(self, mock_cross_encoder):
        mock_model = Mock()
        mock_model.predict.return_value = [0.9, 0.3, 0.8, 0.4, 0.7]
        mock_cross_encoder.return_value = mock_model

        service = RerankService()

        results = [
            SearchResult(file_id="1", file_path="/p1", score=0.5, metadata={}),
            SearchResult(file_id="2", file_path="/p2", score=0.6, metadata={}),
            SearchResult(file_id="3", file_path="/p3", score=0.4, metadata={}),
            SearchResult(file_id="4", file_path="/p4", score=0.55, metadata={}),
            SearchResult(file_id="5", file_path="/p5", score=0.45, metadata={}),
        ]

        with patch.object(service, "_load_document_content", return_value="content"):
            reranked = await service._rerank_batch(query="precision test", results=results, top_k=3)

        assert len(reranked) == 3
        assert reranked[0].file_id == "1"
        assert reranked[1].file_id == "3"
        assert reranked[2].file_id == "5"
        assert reranked[0].score > reranked[1].score > reranked[2].score

    @pytest.mark.asyncio()
    @patch("src.modules.reranker.model.CrossEncoder")
    async def test_ndcg_improvement_simulation(self, mock_cross_encoder):
        def calculate_ndcg(relevance_scores, k):
            dcg = sum(
                (2**rel - 1) / (i + 2).bit_length() for i, rel in enumerate(relevance_scores[:k])
            )
            ideal_scores = sorted(relevance_scores, reverse=True)
            idcg = sum(
                (2**rel - 1) / (i + 2).bit_length() for i, rel in enumerate(ideal_scores[:k])
            )
            return dcg / idcg if idcg > 0 else 0

        ground_truth_relevance = [3, 0, 2, 1, 3, 0, 2]

        initial_scores = [0.5, 0.6, 0.4, 0.55, 0.45, 0.7, 0.35]
        reranked_scores = [0.9, 0.2, 0.7, 0.4, 0.85, 0.1, 0.6]

        mock_model = Mock()
        mock_model.predict.return_value = reranked_scores
        mock_cross_encoder.return_value = mock_model

        service = RerankService()

        results = [
            SearchResult(file_id=str(i), file_path=f"/p{i}", score=s, metadata={})
            for i, s in enumerate(initial_scores)
        ]

        with patch.object(service, "_load_document_content", return_value="content"):
            reranked = await service._rerank_batch("test", results, top_k=5)

        initial_order = sorted(enumerate(initial_scores), key=lambda x: x[1], reverse=True)[:5]
        initial_relevance = [ground_truth_relevance[i] for i, _ in initial_order]

        reranked_order = [int(r.file_id) for r in reranked]
        reranked_relevance = [ground_truth_relevance[i] for i in reranked_order]

        initial_ndcg = calculate_ndcg(initial_relevance, 5)
        reranked_ndcg = calculate_ndcg(reranked_relevance, 5)

        print(f"Initial nDCG@5: {initial_ndcg:.4f}")
        print(f"Reranked nDCG@5: {reranked_ndcg:.4f}")
        print(f"Improvement: {((reranked_ndcg - initial_ndcg) / initial_ndcg * 100):.2f}%")

        assert reranked_ndcg >= initial_ndcg


if __name__ == "__main__":
    pytest.main([__file__, "-v", "--tb=short"])
