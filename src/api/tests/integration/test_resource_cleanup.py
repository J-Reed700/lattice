"""
Tests for proper resource cleanup in service dependencies.

Validates that services are properly initialized and cleaned up
to prevent resource leaks.
"""

from unittest.mock import AsyncMock, patch

import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.routes.agentic_rag import (
    get_agentic_rag_service,
    get_simple_llm_service,
)
from src.api.v1.llm import get_llm_service
from src.config import Settings
from src.services.llm.ollama_service import OllamaService
from src.services.llm.service import LLMService


@pytest.mark.integration()
class TestServiceResourceCleanup:
    """Test that service dependencies properly clean up resources."""

    @pytest.mark.asyncio()
    async def test_agentic_rag_service_cleanup(
        self, db_session: AsyncSession, test_settings: Settings
    ):
        """Test that get_agentic_rag_service properly cleans up resources."""
        cleanup_called = False

        # Mock OllamaService to track cleanup
        with patch("src.api.routes.agentic_rag.OllamaService") as mock_ollama_class:
            mock_ollama = AsyncMock(spec=OllamaService)
            mock_ollama.initialize = AsyncMock()

            async def mock_cleanup():
                nonlocal cleanup_called
                cleanup_called = True

            mock_ollama.cleanup = mock_cleanup
            mock_ollama_class.return_value = mock_ollama

            # Use the dependency as an async generator
            async for service in get_agentic_rag_service(
                session=db_session, settings=test_settings
            ):
                # Service should be initialized
                assert mock_ollama.initialize.called
                assert not cleanup_called

            # After exiting the context, cleanup should be called
            assert cleanup_called

    @pytest.mark.asyncio()
    async def test_agentic_rag_service_cleanup_on_error(
        self, db_session: AsyncSession, test_settings: Settings
    ):
        """Test cleanup is called even when an error occurs."""
        cleanup_called = False

        with patch("src.api.routes.agentic_rag.OllamaService") as mock_ollama_class:
            mock_ollama = AsyncMock(spec=OllamaService)
            mock_ollama.initialize = AsyncMock()

            async def mock_cleanup():
                nonlocal cleanup_called
                cleanup_called = True

            mock_ollama.cleanup = mock_cleanup
            mock_ollama_class.return_value = mock_ollama

            # Simulate an error during service usage
            with pytest.raises(ValueError):
                async for service in get_agentic_rag_service(
                    session=db_session, settings=test_settings
                ):
                    raise ValueError("Simulated error")

            # Cleanup should still be called
            assert cleanup_called

    @pytest.mark.asyncio()
    async def test_simple_llm_service_cleanup(
        self, db_session: AsyncSession, test_settings: Settings
    ):
        """Test that get_simple_llm_service properly cleans up resources."""
        cleanup_called = False

        with patch("src.api.routes.agentic_rag.LLMService") as mock_llm_class:
            mock_llm = AsyncMock(spec=LLMService)
            mock_llm.initialize = AsyncMock()

            async def mock_cleanup():
                nonlocal cleanup_called
                cleanup_called = True

            mock_llm.cleanup = mock_cleanup
            mock_llm_class.return_value = mock_llm

            async for service in get_simple_llm_service(session=db_session, settings=test_settings):
                assert mock_llm.initialize.called
                assert not cleanup_called

            assert cleanup_called

    @pytest.mark.asyncio()
    async def test_llm_service_cleanup(self, db_session: AsyncSession):
        """Test that get_llm_service properly cleans up resources."""
        cleanup_called = False

        with patch("src.api.v1.llm.LLMService") as mock_llm_class:
            mock_llm = AsyncMock(spec=LLMService)
            mock_llm.initialize = AsyncMock()
            mock_llm.model = "llama2"

            async def mock_cleanup():
                nonlocal cleanup_called
                cleanup_called = True

            mock_llm.cleanup = mock_cleanup
            mock_llm_class.return_value = mock_llm

            async for service in get_llm_service(session=db_session):
                assert mock_llm.initialize.called
                assert not cleanup_called

            assert cleanup_called

    @pytest.mark.asyncio()
    async def test_multiple_concurrent_requests_cleanup(
        self, db_session: AsyncSession, test_settings: Settings
    ):
        """Test that multiple concurrent requests each get their own service instance."""
        import asyncio

        cleanup_count = 0
        init_count = 0

        with patch("src.api.routes.agentic_rag.OllamaService") as mock_ollama_class:

            def create_mock_ollama():
                nonlocal init_count, cleanup_count
                mock = AsyncMock(spec=OllamaService)

                async def mock_init():
                    nonlocal init_count
                    init_count += 1

                async def mock_cleanup():
                    nonlocal cleanup_count
                    cleanup_count += 1

                mock.initialize = mock_init
                mock.cleanup = mock_cleanup
                return mock

            mock_ollama_class.side_effect = create_mock_ollama

            async def use_service():
                async for service in get_agentic_rag_service(
                    session=db_session, settings=test_settings
                ):
                    await asyncio.sleep(0.01)  # Simulate some work
                    return service

            # Run 3 concurrent requests
            await asyncio.gather(use_service(), use_service(), use_service())

            # Each request should have its own service instance
            assert init_count == 3
            assert cleanup_count == 3

    @pytest.mark.asyncio()
    async def test_ollama_service_context_manager(self):
        """Test OllamaService async context manager."""
        service = OllamaService(
            base_url="http://localhost:11434",
            default_model="llama2",
        )

        # Track initialization and cleanup
        init_called = False
        cleanup_called = False

        original_init = service.initialize
        original_cleanup = service.cleanup

        async def track_init():
            nonlocal init_called
            init_called = True
            await original_init()

        async def track_cleanup():
            nonlocal cleanup_called
            cleanup_called = True
            await original_cleanup()

        service.initialize = track_init
        service.cleanup = track_cleanup

        async with service:
            assert init_called
            assert not cleanup_called

        assert cleanup_called

    @pytest.mark.asyncio()
    async def test_ollama_service_manual_lifecycle(self):
        """Test OllamaService manual initialization and cleanup."""
        service = OllamaService(
            base_url="http://localhost:11434",
            default_model="llama2",
        )

        # Initialize
        await service.initialize()
        assert service._client is not None

        # Cleanup
        await service.cleanup()
        assert service._client is None

    @pytest.mark.asyncio()
    async def test_llm_service_cleanup_cleans_ollama(self):
        """Test that LLMService.cleanup also cleans up its OllamaService."""
        from src.services.embeddings import EmbeddingService
        from src.services.search import SearchService

        # Create mock session
        mock_session = AsyncMock(spec=AsyncSession)
        embedding_service = EmbeddingService()
        search_service = SearchService(mock_session, embedding_service)

        llm_service = LLMService(
            search_service=search_service,
            ollama_url="http://localhost:11434",
            model="llama2",
        )

        # Initialize
        await llm_service.initialize()
        assert llm_service._ollama_service is not None

        # Get reference to ollama service
        ollama_service = llm_service._ollama_service

        # Cleanup
        await llm_service.cleanup()

        # Both should be cleaned up
        assert llm_service._ollama_service is None
        assert ollama_service._client is None


@pytest.mark.integration()
class TestResourceLeakPrevention:
    """Test that resources don't leak under various scenarios."""

    @pytest.mark.asyncio()
    async def test_no_leak_on_request_cancellation(
        self, db_session: AsyncSession, test_settings: Settings
    ):
        """Test cleanup happens even if request is cancelled."""
        import asyncio

        cleanup_called = False

        with patch("src.api.routes.agentic_rag.OllamaService") as mock_ollama_class:
            mock_ollama = AsyncMock(spec=OllamaService)
            mock_ollama.initialize = AsyncMock()

            async def mock_cleanup():
                nonlocal cleanup_called
                cleanup_called = True

            mock_ollama.cleanup = mock_cleanup
            mock_ollama_class.return_value = mock_ollama

            async def use_service_then_cancel():
                async for service in get_agentic_rag_service(
                    session=db_session, settings=test_settings
                ):
                    # Simulate cancellation during service usage
                    raise asyncio.CancelledError()

            with pytest.raises(asyncio.CancelledError):
                await use_service_then_cancel()

            # Cleanup should still be called
            assert cleanup_called

    @pytest.mark.asyncio()
    async def test_no_leak_on_multiple_errors(
        self, db_session: AsyncSession, test_settings: Settings
    ):
        """Test that multiple failed requests don't leak resources."""
        cleanup_count = 0

        with patch("src.api.routes.agentic_rag.OllamaService") as mock_ollama_class:

            def create_mock():
                nonlocal cleanup_count
                mock = AsyncMock(spec=OllamaService)
                mock.initialize = AsyncMock()

                async def mock_cleanup():
                    nonlocal cleanup_count
                    cleanup_count += 1

                mock.cleanup = mock_cleanup
                return mock

            mock_ollama_class.side_effect = create_mock

            # Simulate 5 failed requests
            for _ in range(5):
                with pytest.raises(ValueError):
                    async for service in get_agentic_rag_service(
                        session=db_session, settings=test_settings
                    ):
                        raise ValueError("Simulated error")

            # All 5 should have been cleaned up
            assert cleanup_count == 5
