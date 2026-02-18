"""Integration tests for LLMService event emission."""

from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from src.events.domain.llm_events import (
    LLMCacheInvalidated,
    LLMHealthCheckPerformed,
    LLMModelsListed,
    QuestionAnswered,
    QuestionAnsweringFailed,
    QuestionAsked,
)
from src.modules.search_engine import SearchResult, SearchResults
from src.services.llm.service import LLMService


class TestLLMServiceEventIntegration:
    """Test LLMService integration with EventBus."""

    @pytest.fixture
    def mock_event_bus(self) -> AsyncMock:
        """Create a mock EventBus."""
        event_bus = AsyncMock()
        event_bus.emit = AsyncMock()
        return event_bus

    @pytest.fixture
    def mock_search_service(self) -> AsyncMock:
        """Create a mock SearchService."""
        search_service = AsyncMock()
        # Default search results
        search_service.search.return_value = SearchResults(
            results=[
                SearchResult(
                    id=1,
                    file_path="/test/doc1.txt",
                    filename="doc1.txt",
                    content="Test content",
                    snippet="Test snippet",
                    score=0.95,
                    modified_at=None,
                )
            ],
            total=1,
        )
        return search_service

    @pytest.fixture
    def mock_ollama_service(self) -> MagicMock:
        """Create a mock OllamaService."""
        ollama_service = MagicMock()
        ollama_service.generate_text = AsyncMock(return_value="This is the answer.")
        ollama_service.generate_text_stream = AsyncMock()
        ollama_service.health_check = AsyncMock(return_value=True)
        ollama_service.list_available_models = AsyncMock(return_value=["llama2", "mistral"])
        ollama_service.invalidate_cache = MagicMock()
        ollama_service.get_cache_metrics = MagicMock(
            return_value={"cache_size": 100, "hits": 50, "misses": 25}
        )
        return ollama_service

    @pytest.fixture
    async def llm_service(
        self,
        mock_search_service: AsyncMock,
        mock_event_bus: AsyncMock,
        mock_ollama_service: MagicMock,
    ) -> LLMService:
        """Create LLMService with mocked dependencies."""
        service = LLMService(
            search_service=mock_search_service,
            event_bus=mock_event_bus,
        )
        # Inject mock ollama service
        service._ollama_service = mock_ollama_service
        return service

    @pytest.mark.asyncio
    async def test_ask_question_with_sources_emits_asked_and_answered_events(
        self,
        llm_service: LLMService,
        mock_event_bus: AsyncMock,
    ) -> None:
        """Test that successful Q&A emits QuestionAsked and QuestionAnswered events."""
        await llm_service.ask_question_with_sources(
            question="What is ML?",
            user_id="user123",
            max_context_docs=5,
            search_mode="hybrid",
            model="llama2",
        )

        # Verify two events were emitted (asked + answered)
        assert mock_event_bus.emit.call_count == 2

        # Verify QuestionAsked event
        first_event = mock_event_bus.emit.call_args_list[0][0][0]
        assert isinstance(first_event, QuestionAsked)
        assert first_event.question == "What is ML?"
        assert first_event.user_id == "user123"
        assert first_event.search_mode == "hybrid"
        assert first_event.max_context_docs == 5
        assert first_event.model == "llama2"

        # Verify QuestionAnswered event
        second_event = mock_event_bus.emit.call_args_list[1][0][0]
        assert isinstance(second_event, QuestionAnswered)
        assert second_event.question == "What is ML?"
        assert second_event.answer_length == len("This is the answer.")
        assert second_event.sources_count == 1
        assert second_event.user_id == "user123"
        assert second_event.execution_time_ms > 0

    @pytest.mark.asyncio
    async def test_ask_question_with_sources_emits_failure_event_on_error(
        self,
        llm_service: LLMService,
        mock_event_bus: AsyncMock,
        mock_search_service: AsyncMock,
    ) -> None:
        """Test that failed Q&A emits QuestionAnsweringFailed event."""
        # Configure mock to raise exception
        mock_search_service.search.side_effect = Exception("Search service error")

        with pytest.raises(Exception, match="Search service error"):
            await llm_service.ask_question_with_sources(
                question="Test?",
                user_id="user123",
            )

        # Verify two events were emitted (asked + failed)
        assert mock_event_bus.emit.call_count == 2

        # Verify QuestionAsked event
        first_event = mock_event_bus.emit.call_args_list[0][0][0]
        assert isinstance(first_event, QuestionAsked)

        # Verify QuestionAnsweringFailed event
        second_event = mock_event_bus.emit.call_args_list[1][0][0]
        assert isinstance(second_event, QuestionAnsweringFailed)
        assert second_event.question == "Test?"
        assert "Search service error" in second_event.error
        assert second_event.error_type == "search"

    @pytest.mark.asyncio
    async def test_ask_question_streaming_emits_events(
        self,
        llm_service: LLMService,
        mock_event_bus: AsyncMock,
        mock_ollama_service: MagicMock,
    ) -> None:
        """Test that streaming Q&A emits events after completion."""

        # Configure mock to return stream
        async def mock_stream():
            yield "Hello "
            yield "world!"

        mock_ollama_service.generate_text_stream.return_value = mock_stream()

        # Consume the stream
        chunks = []
        async for chunk in llm_service.ask_question(
            question="Test?",
            user_id="user123",
        ):
            chunks.append(chunk)

        # Verify events were emitted (asked + answered)
        assert mock_event_bus.emit.call_count == 2

        # Verify QuestionAsked event
        first_event = mock_event_bus.emit.call_args_list[0][0][0]
        assert isinstance(first_event, QuestionAsked)
        assert first_event.question == "Test?"

        # Verify QuestionAnswered event
        second_event = mock_event_bus.emit.call_args_list[1][0][0]
        assert isinstance(second_event, QuestionAnswered)
        assert second_event.answer_length == len("Hello world!")
        assert second_event.sources_count == 1

    @pytest.mark.asyncio
    async def test_ask_question_streaming_emits_failure_on_error(
        self,
        llm_service: LLMService,
        mock_event_bus: AsyncMock,
        mock_ollama_service: MagicMock,
    ) -> None:
        """Test that streaming errors emit QuestionAnsweringFailed event."""

        # Configure mock to raise exception during streaming
        async def mock_stream_error():
            yield "Hello "
            raise Exception("Generation error")

        mock_ollama_service.generate_text_stream.return_value = mock_stream_error()

        # Consume the stream
        chunks = []
        async for chunk in llm_service.ask_question(question="Test?"):
            chunks.append(chunk)

        # Verify events were emitted (asked + failed)
        assert mock_event_bus.emit.call_count == 2

        # Verify QuestionAnsweringFailed event
        second_event = mock_event_bus.emit.call_args_list[1][0][0]
        assert isinstance(second_event, QuestionAnsweringFailed)
        assert "Generation error" in second_event.error
        assert second_event.error_type == "other"

    @pytest.mark.asyncio
    async def test_health_check_emits_event(
        self,
        llm_service: LLMService,
        mock_event_bus: AsyncMock,
    ) -> None:
        """Test that health check emits LLMHealthCheckPerformed event."""
        is_healthy = await llm_service.health_check()

        assert is_healthy is True

        # Verify event was emitted
        mock_event_bus.emit.assert_called_once()
        emitted_event = mock_event_bus.emit.call_args[0][0]

        assert isinstance(emitted_event, LLMHealthCheckPerformed)
        assert emitted_event.is_healthy is True
        assert emitted_event.response_time_ms > 0

    @pytest.mark.asyncio
    async def test_health_check_emits_failure_event(
        self,
        llm_service: LLMService,
        mock_event_bus: AsyncMock,
        mock_ollama_service: MagicMock,
    ) -> None:
        """Test that failed health check emits event with is_healthy=False."""
        # Configure mock to fail
        mock_ollama_service.health_check.side_effect = Exception("Connection refused")

        is_healthy = await llm_service.health_check()

        assert is_healthy is False

        # Verify event was emitted
        mock_event_bus.emit.assert_called_once()
        emitted_event = mock_event_bus.emit.call_args[0][0]

        assert isinstance(emitted_event, LLMHealthCheckPerformed)
        assert emitted_event.is_healthy is False
        assert "Connection refused" in emitted_event.metadata["error"]

    @pytest.mark.asyncio
    async def test_list_available_models_emits_event(
        self,
        llm_service: LLMService,
        mock_event_bus: AsyncMock,
    ) -> None:
        """Test that listing models emits LLMModelsListed event."""
        models = await llm_service.list_available_models()

        assert models == ["llama2", "mistral"]

        # Verify event was emitted
        mock_event_bus.emit.assert_called_once()
        emitted_event = mock_event_bus.emit.call_args[0][0]

        assert isinstance(emitted_event, LLMModelsListed)
        assert emitted_event.models_count == 2
        assert emitted_event.models == ["llama2", "mistral"]

    @pytest.mark.asyncio
    async def test_invalidate_cache_emits_event(
        self,
        llm_service: LLMService,
        mock_event_bus: AsyncMock,
    ) -> None:
        """Test that cache invalidation emits LLMCacheInvalidated event."""
        # Patch asyncio.create_task to execute immediately
        with patch("asyncio.create_task") as mock_create_task:
            llm_service.invalidate_cache()

            # Verify create_task was called
            mock_create_task.assert_called_once()

            # Get the coroutine that was passed to create_task
            coro = mock_create_task.call_args[0][0]

            # Execute the coroutine
            await coro

            # Verify event was emitted
            mock_event_bus.emit.assert_called_once()
            emitted_event = mock_event_bus.emit.call_args[0][0]

            assert isinstance(emitted_event, LLMCacheInvalidated)
            assert emitted_event.reason == "manual"
            assert emitted_event.cache_size_before == 100

    @pytest.mark.asyncio
    async def test_service_without_event_bus(
        self,
        mock_search_service: AsyncMock,
        mock_ollama_service: MagicMock,
    ) -> None:
        """Test that service works without event_bus (backward compatibility)."""
        service = LLMService(
            search_service=mock_search_service,
            event_bus=None,  # No event bus
        )
        service._ollama_service = mock_ollama_service

        # All methods should work without raising exceptions
        result = await service.ask_question_with_sources(question="Test?")
        assert "answer" in result

        is_healthy = await service.health_check()
        assert is_healthy is True

        models = await service.list_available_models()
        assert len(models) == 2

        service.invalidate_cache()  # Should not raise

    @pytest.mark.asyncio
    async def test_streaming_tracks_metrics_correctly(
        self,
        llm_service: LLMService,
        mock_event_bus: AsyncMock,
        mock_ollama_service: MagicMock,
    ) -> None:
        """Test that streaming correctly tracks answer length."""

        async def mock_stream():
            yield "Part "
            yield "1 "
            yield "Part "
            yield "2"

        mock_ollama_service.generate_text_stream.return_value = mock_stream()

        # Consume the stream
        chunks = []
        async for chunk in llm_service.ask_question(question="Test?"):
            chunks.append(chunk)

        # Verify QuestionAnswered event has correct answer length
        second_event = mock_event_bus.emit.call_args_list[1][0][0]
        assert isinstance(second_event, QuestionAnswered)
        assert second_event.answer_length == len("Part 1 Part 2")

    @pytest.mark.asyncio
    async def test_default_model_used_when_not_specified(
        self,
        llm_service: LLMService,
        mock_event_bus: AsyncMock,
    ) -> None:
        """Test that default model is used when not specified."""
        await llm_service.ask_question_with_sources(
            question="Test?",
            # model not specified
        )

        # Verify QuestionAsked event has default model
        first_event = mock_event_bus.emit.call_args_list[0][0][0]
        assert isinstance(first_event, QuestionAsked)
        assert first_event.model == "llama2"  # Default from service
