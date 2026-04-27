from datetime import datetime
from unittest.mock import AsyncMock, Mock, patch

from httpx import AsyncClient
import pytest


class MockAgenticResponse:
    def __init__(self, answer="Test answer", sources=None, corrected=False, iterations=3):
        self.answer = answer
        self.sources = sources or []
        self.reflection = "Answer looks good"
        self.corrected = corrected
        self.total_iterations = iterations
        self.total_time_ms = 5000
        self.metadata = {"model": "llama2", "search_mode": "hybrid"}
        self.reasoning_steps = []


class MockThought:
    def __init__(self):
        self.phase = "planning"
        self.content = "I need to search for documents"
        self.timestamp = datetime.utcnow()


class MockAction:
    def __init__(self):
        self.tool = Mock()
        self.tool.value = "search"
        self.tool_input = {"query": "test"}


class MockObservation:
    def __init__(self):
        self.success = True
        self.result = ["doc1", "doc2"]
        self.error = None


class MockReasoningStep:
    def __init__(self):
        self.thought = MockThought()
        self.action = MockAction()
        self.observation = MockObservation()
        self.iteration = 1


class MockSourceDocument:
    def __init__(self, file_path="/test/doc.txt", filename="doc.txt", score=0.85):
        self.file_path = file_path
        self.filename = filename
        self.score = score
        self.snippet = "Test snippet content"


class TestAgenticRAGAskEndpoint:
    @pytest.mark.asyncio()
    async def test_ask_success(self, client: AsyncClient):
        with patch("src.api.routes.agentic_rag.get_agentic_rag_service") as mock_service_dep:
            mock_response = MockAgenticResponse(
                answer="The answer is based on multiple documents",
                sources=[
                    MockSourceDocument("/test/doc1.txt", "doc1.txt", 0.92),
                    MockSourceDocument("/test/doc2.txt", "doc2.txt", 0.88),
                ],
                corrected=False,
                iterations=3,
            )
            mock_response.reasoning_steps = [MockReasoningStep()]

            mock_service = AsyncMock()
            mock_service.ask_question = AsyncMock(return_value=mock_response)
            mock_service_dep.return_value = mock_service

            response = await client.post(
                "/api/v1/agentic-rag/ask",
                json={
                    "query": "What are the main findings?",
                    "search_mode": "hybrid",
                    "streaming": False,
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "answer" in data
            assert "reasoning_steps" in data
            assert "sources" in data
            assert "reflection" in data
            assert "corrected" in data
            assert "total_iterations" in data
            assert "total_time_ms" in data
            assert data["answer"] == "The answer is based on multiple documents"
            assert len(data["sources"]) == 2
            assert data["corrected"] is False
            assert data["total_iterations"] == 3

    @pytest.mark.asyncio()
    async def test_ask_empty_query_validation(self, client: AsyncClient):
        response = await client.post(
            "/api/v1/agentic-rag/ask", json={"query": "", "search_mode": "hybrid"}
        )

        assert response.status_code == 400
        data = response.json()
        assert "Query cannot be empty" in data["detail"]["message"]

    @pytest.mark.asyncio()
    async def test_ask_whitespace_only_validation(self, client: AsyncClient):
        response = await client.post(
            "/api/v1/agentic-rag/ask", json={"query": "   ", "search_mode": "hybrid"}
        )

        assert response.status_code == 400

    @pytest.mark.asyncio()
    async def test_ask_with_vector_search_mode(self, client: AsyncClient):
        with patch("src.api.routes.agentic_rag.get_agentic_rag_service") as mock_service_dep:
            mock_response = MockAgenticResponse()
            mock_response.reasoning_steps = []

            mock_service = AsyncMock()
            mock_service.ask_question = AsyncMock(return_value=mock_response)
            mock_service_dep.return_value = mock_service

            response = await client.post(
                "/api/v1/agentic-rag/ask", json={"query": "Test query", "search_mode": "vector"}
            )

            assert response.status_code == 200
            call_args = mock_service.ask_question.call_args
            assert call_args.kwargs["search_mode"] == "vector"

    @pytest.mark.asyncio()
    async def test_ask_with_custom_temperature(self, client: AsyncClient):
        with patch("src.api.routes.agentic_rag.get_agentic_rag_service") as mock_service_dep:
            mock_response = MockAgenticResponse()
            mock_response.reasoning_steps = []

            mock_service = AsyncMock()
            mock_service.ask_question = AsyncMock(return_value=mock_response)
            mock_service_dep.return_value = mock_service

            response = await client.post(
                "/api/v1/agentic-rag/ask", json={"query": "Test query", "temperature": 0.9}
            )

            assert response.status_code == 200
            call_args = mock_service.ask_question.call_args
            assert call_args.kwargs["temperature"] == 0.9

    @pytest.mark.asyncio()
    async def test_ask_with_corrected_answer(self, client: AsyncClient):
        with patch("src.api.routes.agentic_rag.get_agentic_rag_service") as mock_service_dep:
            mock_response = MockAgenticResponse(
                answer="Corrected answer after reflection", corrected=True, iterations=5
            )
            mock_response.reasoning_steps = []

            mock_service = AsyncMock()
            mock_service.ask_question = AsyncMock(return_value=mock_response)
            mock_service_dep.return_value = mock_service

            response = await client.post(
                "/api/v1/agentic-rag/ask", json={"query": "Complex question"}
            )

            assert response.status_code == 200
            data = response.json()
            assert data["corrected"] is True
            assert data["total_iterations"] == 5

    @pytest.mark.asyncio()
    async def test_ask_service_error(self, client: AsyncClient):
        with patch("src.api.routes.agentic_rag.get_agentic_rag_service") as mock_service_dep:
            mock_service = AsyncMock()
            mock_service.ask_question = AsyncMock(side_effect=Exception("Service error"))
            mock_service_dep.return_value = mock_service

            response = await client.post("/api/v1/agentic-rag/ask", json={"query": "Test query"})

            assert response.status_code == 500
            data = response.json()
            assert "Agentic RAG failed" in data["detail"]

    @pytest.mark.asyncio()
    async def test_ask_validation_max_length(self, client: AsyncClient):
        long_query = "a" * 2001

        response = await client.post("/api/v1/agentic-rag/ask", json={"query": long_query})

        assert response.status_code == 422


class TestAgenticRAGCompareEndpoint:
    @pytest.mark.asyncio()
    async def test_compare_success(self, client: AsyncClient):
        with patch("src.api.routes.agentic_rag.get_agentic_rag_service") as mock_agentic_dep, patch(
            "src.api.routes.agentic_rag.get_simple_llm_service"
        ) as mock_simple_dep:
            mock_simple_result = {
                "answer": "Simple answer",
                "sources": [{"path": "/test/doc1.txt", "score": 0.85}],
                "metadata": {},
            }

            mock_agentic_result = MockAgenticResponse(
                answer="Comprehensive agentic answer",
                sources=[
                    MockSourceDocument("/test/doc1.txt", "doc1.txt", 0.92),
                    MockSourceDocument("/test/doc2.txt", "doc2.txt", 0.88),
                    MockSourceDocument("/test/doc3.txt", "doc3.txt", 0.82),
                ],
                corrected=True,
                iterations=4,
            )

            mock_simple_service = AsyncMock()
            mock_simple_service.ask_question_with_sources = AsyncMock(
                return_value=mock_simple_result
            )
            mock_simple_dep.return_value = mock_simple_service

            mock_agentic_service = AsyncMock()
            mock_agentic_service.ask_question = AsyncMock(return_value=mock_agentic_result)
            mock_agentic_dep.return_value = mock_agentic_service

            response = await client.post(
                "/api/v1/agentic-rag/compare",
                json={"query": "Compare Q3 and Q4", "search_mode": "hybrid"},
            )

            assert response.status_code == 200
            data = response.json()
            assert "query" in data
            assert "simple_rag" in data
            assert "agentic_rag" in data
            assert "comparison" in data

            assert data["simple_rag"]["answer"] == "Simple answer"
            assert data["agentic_rag"]["answer"] == "Comprehensive agentic answer"

            assert "time_multiplier" in data["comparison"]
            assert "additional_sources" in data["comparison"]
            assert "recommendation" in data["comparison"]

    @pytest.mark.asyncio()
    async def test_compare_empty_query_validation(self, client: AsyncClient):
        response = await client.post("/api/v1/agentic-rag/compare", json={"query": ""})

        assert response.status_code == 400
        data = response.json()
        assert "Query cannot be empty" in data["detail"]["message"]

    @pytest.mark.asyncio()
    async def test_compare_different_search_modes(self, client: AsyncClient):
        with patch("src.api.routes.agentic_rag.get_agentic_rag_service") as mock_agentic_dep, patch(
            "src.api.routes.agentic_rag.get_simple_llm_service"
        ) as mock_simple_dep:
            mock_simple_service = AsyncMock()
            mock_simple_service.ask_question_with_sources = AsyncMock(
                return_value={"answer": "Answer", "sources": [], "metadata": {}}
            )
            mock_simple_dep.return_value = mock_simple_service

            mock_agentic_result = MockAgenticResponse()
            mock_agentic_result.sources = []

            mock_agentic_service = AsyncMock()
            mock_agentic_service.ask_question = AsyncMock(return_value=mock_agentic_result)
            mock_agentic_dep.return_value = mock_agentic_service

            for mode in ["vector", "text", "hybrid"]:
                response = await client.post(
                    "/api/v1/agentic-rag/compare", json={"query": "Test query", "search_mode": mode}
                )

                assert response.status_code == 200

    @pytest.mark.asyncio()
    async def test_compare_service_error(self, client: AsyncClient):
        with patch("src.api.routes.agentic_rag.get_simple_llm_service") as mock_simple_dep:
            mock_simple_service = AsyncMock()
            mock_simple_service.ask_question_with_sources = AsyncMock(
                side_effect=Exception("Service error")
            )
            mock_simple_dep.return_value = mock_simple_service

            response = await client.post(
                "/api/v1/agentic-rag/compare", json={"query": "Test query"}
            )

            assert response.status_code == 500
            data = response.json()
            assert "Comparison failed" in data["detail"]

    @pytest.mark.asyncio()
    async def test_compare_recommendation_logic(self, client: AsyncClient):
        with patch("src.api.routes.agentic_rag.get_agentic_rag_service") as mock_agentic_dep, patch(
            "src.api.routes.agentic_rag.get_simple_llm_service"
        ) as mock_simple_dep:
            mock_simple_result = {
                "answer": "Simple answer",
                "sources": [{"path": "/test/doc1.txt", "score": 0.85}],
                "metadata": {},
            }

            mock_simple_service = AsyncMock()
            mock_simple_service.ask_question_with_sources = AsyncMock(
                return_value=mock_simple_result
            )
            mock_simple_dep.return_value = mock_simple_service

            mock_agentic_result = MockAgenticResponse(
                sources=[
                    MockSourceDocument("/test/doc1.txt", "doc1.txt", 0.92),
                    MockSourceDocument("/test/doc2.txt", "doc2.txt", 0.88),
                    MockSourceDocument("/test/doc3.txt", "doc3.txt", 0.82),
                    MockSourceDocument("/test/doc4.txt", "doc4.txt", 0.78),
                ],
                corrected=True,
                iterations=5,
            )

            mock_agentic_service = AsyncMock()
            mock_agentic_service.ask_question = AsyncMock(return_value=mock_agentic_result)
            mock_agentic_dep.return_value = mock_agentic_service

            response = await client.post(
                "/api/v1/agentic-rag/compare", json={"query": "Complex question requiring analysis"}
            )

            assert response.status_code == 200
            data = response.json()
            assert data["comparison"]["agentic_corrected"] is True
            assert data["comparison"]["additional_sources"] >= 3


class TestAgenticRAGHealthEndpoint:
    @pytest.mark.asyncio()
    async def test_health_ollama_available(self, client: AsyncClient):
        with patch("src.api.routes.agentic_rag.get_agentic_rag_service") as mock_service_dep:
            mock_ollama_service = AsyncMock()
            mock_ollama_service.health_check = AsyncMock(return_value=True)

            mock_service = Mock()
            mock_service.ollama_service = mock_ollama_service
            mock_service.max_iterations = 5
            mock_service.timeout = 60
            mock_service.enable_reflection = True
            mock_service.enable_correction = True

            mock_service_dep.return_value = mock_service

            response = await client.get("/api/v1/agentic-rag/health")

            assert response.status_code == 200
            data = response.json()
            assert data["status"] == "healthy"
            assert data["ollama_available"] is True
            assert data["max_iterations"] == 5
            assert data["timeout_seconds"] == 60
            assert data["reflection_enabled"] is True
            assert data["correction_enabled"] is True

    @pytest.mark.asyncio()
    async def test_health_ollama_unavailable(self, client: AsyncClient):
        with patch("src.api.routes.agentic_rag.get_agentic_rag_service") as mock_service_dep:
            mock_ollama_service = AsyncMock()
            mock_ollama_service.health_check = AsyncMock(return_value=False)

            mock_service = Mock()
            mock_service.ollama_service = mock_ollama_service
            mock_service.max_iterations = 5
            mock_service.timeout = 60
            mock_service.enable_reflection = True
            mock_service.enable_correction = True

            mock_service_dep.return_value = mock_service

            response = await client.get("/api/v1/agentic-rag/health")

            assert response.status_code == 200
            data = response.json()
            assert data["status"] == "unhealthy"
            assert data["ollama_available"] is False

    @pytest.mark.asyncio()
    async def test_health_service_error(self, client: AsyncClient):
        with patch("src.api.routes.agentic_rag.get_agentic_rag_service") as mock_service_dep:
            mock_ollama_service = AsyncMock()
            mock_ollama_service.health_check = AsyncMock(side_effect=Exception("Connection error"))

            mock_service = Mock()
            mock_service.ollama_service = mock_ollama_service

            mock_service_dep.return_value = mock_service

            response = await client.get("/api/v1/agentic-rag/health")

            assert response.status_code == 200
            data = response.json()
            assert data["status"] == "unhealthy"
            assert "error" in data

    @pytest.mark.asyncio()
    async def test_health_configuration_details(self, client: AsyncClient):
        with patch("src.api.routes.agentic_rag.get_agentic_rag_service") as mock_service_dep:
            mock_ollama_service = AsyncMock()
            mock_ollama_service.health_check = AsyncMock(return_value=True)

            mock_service = Mock()
            mock_service.ollama_service = mock_ollama_service
            mock_service.max_iterations = 10
            mock_service.timeout = 120
            mock_service.enable_reflection = False
            mock_service.enable_correction = False

            mock_service_dep.return_value = mock_service

            response = await client.get("/api/v1/agentic-rag/health")

            assert response.status_code == 200
            data = response.json()
            assert data["max_iterations"] == 10
            assert data["timeout_seconds"] == 120
            assert data["reflection_enabled"] is False
            assert data["correction_enabled"] is False
