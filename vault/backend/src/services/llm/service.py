"""LLM service for RAG Q&A with search integration.

This service orchestrates the RAG (Retrieval-Augmented Generation) pipeline:
1. Search for relevant documents (via SearchService)
2. Build context from search results (via shared prompts)
3. Create RAG prompt (via shared prompts)
4. Generate LLM response (via OllamaService)

The service is a THIN ORCHESTRATOR - it coordinates search and LLM generation,
but does not contain the core RAG logic (which lives in domain modules).
"""

from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator
import logging
import time
from typing import TYPE_CHECKING, Any

from src.config.settings import get_settings
from src.events.domain.llm_events import (
    LLMCacheInvalidated,
    LLMHealthCheckPerformed,
    LLMModelsListed,
    QuestionAnswered,
    QuestionAnsweringFailed,
    QuestionAsked,
)
from src.modules.search_engine import SearchFilters, SearchService

from .ollama_client import OllamaAPIError, OllamaConnectionError, OllamaTimeoutError
from .ollama_service import OllamaService
from .prompts import create_context_from_results, create_rag_prompt, create_rag_system_prompt

if TYPE_CHECKING:
    from src.events.bus import EventBus

logger = logging.getLogger(__name__)


class LLMService:
    """Service for LLM-powered Q&A with RAG (Retrieval-Augmented Generation)."""

    def __init__(
        self,
        search_service: SearchService,
        ollama_url: str = "http://localhost:11434",
        model: str = "llama2",
        timeout: int = 120,
        stream_timeout: int = 300,
        event_bus: EventBus | None = None,
    ):
        """
        Initialize LLM service with search integration.

        Args:
            search_service: SearchService instance for retrieving relevant documents
            ollama_url: URL of Ollama server
            model: Default model to use for generation
            timeout: Timeout for non-streaming requests
            stream_timeout: Timeout for streaming requests
            event_bus: Optional event bus for emitting events (backward compatible)
        """
        self.search_service = search_service
        self.model = model
        self._ollama_service: OllamaService | None = None
        self.ollama_url = ollama_url
        self.timeout = timeout
        self.stream_timeout = stream_timeout
        self.event_bus = event_bus
        self.max_parallel_requests = get_settings().ollama_max_parallel_requests

    async def initialize(self) -> None:
        """Initialize the Ollama service."""
        if self._ollama_service is None:
            self._ollama_service = OllamaService(
                base_url=self.ollama_url,
                default_model=self.model,
                timeout=self.timeout,
                stream_timeout=self.stream_timeout,
                max_parallel_requests=self.max_parallel_requests,
            )
            await self._ollama_service.initialize()
            logger.info("LLM service initialized")

    async def cleanup(self) -> None:
        """Cleanup the Ollama service."""
        if self._ollama_service is not None:
            await self._ollama_service.cleanup()
            self._ollama_service = None
            logger.info("LLM service cleaned up")

    @property
    def ollama_service(self) -> OllamaService:
        """Get the Ollama service instance."""
        if self._ollama_service is None:
            raise RuntimeError("LLM service not initialized. Call initialize() first.")
        return self._ollama_service

    async def ask_question(
        self,
        question: str,
        user_id: str | None = None,
        max_context_docs: int = 5,
        search_mode: str = "hybrid",
        filters: SearchFilters | None = None,
        model: str | None = None,
        temperature: float | None = 0.7,
        max_tokens: int | None = 2000,
    ) -> AsyncIterator[str]:
        """
        Answer question using RAG with streaming response.

        Process:
        1. Search for relevant documents using SearchService
        2. Build context from top search results
        3. Create RAG prompt with context
        4. Stream LLM response

        Args:
            question: User's question
            user_id: Optional user ID for filtering (not currently used)
            max_context_docs: Maximum number of documents to include in context
            search_mode: Search mode ("vector", "text", or "hybrid")
            filters: Optional search filters
            model: Optional model override
            temperature: Sampling temperature (0.0 to 1.0)
            max_tokens: Maximum tokens to generate

        Yields:
            Chunks of the LLM response as they are generated

        Raises:
            OllamaConnectionError: If Ollama is not available
            OllamaAPIError: If generation fails
            SearchError: If search fails
        """
        logger.info(f"Processing question: '{question[:100]}...'")
        start_time = time.time()
        used_model = model or self.model

        # Emit QuestionAsked event (fire-and-forget)
        if self.event_bus:
            event = QuestionAsked(
                question=question,
                user_id=user_id,
                search_mode=search_mode,
                max_context_docs=max_context_docs,
                model=used_model,
                metadata={
                    "temperature": temperature,
                    "max_tokens": max_tokens,
                    "has_filters": filters is not None,
                },
            )
            await self.event_bus.emit(event)

        # Step 1: Search for relevant documents
        try:
            search_results = await self.search_service.search(
                query=question,
                mode=search_mode,
                limit=max_context_docs,
                offset=0,
                filters=filters,
                rerank=True,
            )
            logger.info(f"Found {len(search_results.results)} relevant documents")
        except Exception as e:
            logger.error(f"Search failed: {e}", exc_info=True)

            # Emit failure event (fire-and-forget)
            if self.event_bus:
                event = QuestionAnsweringFailed(
                    question=question,
                    error=str(e),
                    error_type="search",
                    search_mode=search_mode,
                    model=used_model,
                    user_id=user_id,
                )
                await self.event_bus.emit(event)

            # Yield error message
            yield f"\n\nError searching documents: {e!s}\n"
            return

        # Step 2: Build context from search results
        context = create_context_from_results(search_results.results)

        # Step 3: Create RAG prompt
        prompt = create_rag_prompt(question, context)
        system_prompt = create_rag_system_prompt()

        # Step 4: Stream LLM response with caching - track metrics
        answer_chunks = []
        error_occurred = False
        error_type = "other"
        error_message = ""

        try:
            async for chunk in self.ollama_service.generate_text_stream(
                prompt=prompt,
                system=system_prompt,
                model=model,
                temperature=temperature,
                max_tokens=max_tokens,
                context_text=context,
            ):
                answer_chunks.append(chunk)
                yield chunk
        except OllamaTimeoutError as e:
            logger.error(f"LLM generation timeout: {e}")
            error_occurred = True
            error_type = "timeout"
            error_message = str(e)
            yield "\n\nError: Request timed out. Please try again.\n"
        except OllamaAPIError as e:
            logger.error(f"LLM API error: {e}")
            error_occurred = True
            error_type = "generation"
            error_message = str(e)
            yield f"\n\nError: LLM generation failed. {e!s}\n"
        except OllamaConnectionError as e:
            logger.error(f"Ollama connection error: {e}")
            error_occurred = True
            error_type = "connection"
            error_message = str(e)
            yield "\n\nError: Cannot connect to Ollama server. Please ensure Ollama is running.\n"
        except Exception as e:
            logger.exception(f"Unexpected error during LLM generation: {e}")
            error_occurred = True
            error_type = "other"
            error_message = str(e)
            yield f"\n\nError: Unexpected error occurred. {e!s}\n"
        finally:
            # Emit appropriate event after streaming completes
            execution_time_ms = (time.time() - start_time) * 1000

            if self.event_bus:
                if error_occurred:
                    # Emit failure event
                    event = QuestionAnsweringFailed(
                        question=question,
                        error=error_message,
                        error_type=error_type,
                        search_mode=search_mode,
                        model=used_model,
                        user_id=user_id,
                        metadata={"execution_time_ms": execution_time_ms},
                    )
                    await self.event_bus.emit(event)
                else:
                    # Emit success event
                    answer = "".join(answer_chunks)
                    event = QuestionAnswered(
                        question=question,
                        answer_length=len(answer),
                        sources_count=len(search_results.results),
                        search_mode=search_mode,
                        model=used_model,
                        execution_time_ms=execution_time_ms,
                        user_id=user_id,
                        metadata={
                            "temperature": temperature,
                            "max_tokens": max_tokens,
                        },
                    )
                    await self.event_bus.emit(event)

    async def ask_question_with_sources(
        self,
        question: str,
        user_id: str | None = None,
        max_context_docs: int = 5,
        search_mode: str = "hybrid",
        filters: SearchFilters | None = None,
        model: str | None = None,
        temperature: float | None = 0.7,
        max_tokens: int | None = 2000,
    ) -> dict[str, Any]:
        """
        Answer question and return both the answer and source documents.

        Same as ask_question but returns structured data instead of streaming.

        Args:
            question: User's question
            user_id: Optional user ID for filtering
            max_context_docs: Maximum number of documents to include
            search_mode: Search mode
            filters: Optional search filters
            model: Optional model override
            temperature: Sampling temperature
            max_tokens: Maximum tokens to generate

        Returns:
            Dictionary with 'answer', 'sources', and metadata
        """
        logger.info(f"Processing question with sources: '{question[:100]}...'")
        start_time = time.time()
        used_model = model or self.model

        # Emit QuestionAsked event (fire-and-forget)
        if self.event_bus:
            event = QuestionAsked(
                question=question,
                user_id=user_id,
                search_mode=search_mode,
                max_context_docs=max_context_docs,
                model=used_model,
                metadata={
                    "temperature": temperature,
                    "max_tokens": max_tokens,
                    "has_filters": filters is not None,
                    "with_sources": True,
                },
            )
            await self.event_bus.emit(event)

        try:
            # Search for relevant documents
            search_results = await self.search_service.search(
                query=question,
                mode=search_mode,
                limit=max_context_docs,
                offset=0,
                filters=filters,
                rerank=True,
            )

            # Build context
            context = create_context_from_results(search_results.results)
            prompt = create_rag_prompt(question, context)
            system_prompt = create_rag_system_prompt()

            # Generate answer (non-streaming) with caching
            answer = await self.ollama_service.generate_text(
                prompt=prompt,
                system=system_prompt,
                model=model,
                temperature=temperature,
                max_tokens=max_tokens,
                context_text=context,
            )

            # Format sources
            sources = []
            for i, result in enumerate(search_results.results, 1):
                sources.append(
                    {
                        "id": i,
                        "file_path": result.file_path,
                        "filename": result.filename,
                        "score": result.score,
                        "snippet": result.snippet,
                        "modified_at": result.modified_at.isoformat() if result.modified_at else None,
                    }
                )

            # Emit success event (fire-and-forget)
            execution_time_ms = (time.time() - start_time) * 1000
            if self.event_bus:
                event = QuestionAnswered(
                    question=question,
                    answer_length=len(answer),
                    sources_count=len(sources),
                    search_mode=search_mode,
                    model=used_model,
                    execution_time_ms=execution_time_ms,
                    user_id=user_id,
                    metadata={
                        "temperature": temperature,
                        "max_tokens": max_tokens,
                        "with_sources": True,
                    },
                )
                await self.event_bus.emit(event)

            return {
                "answer": answer,
                "sources": sources,
                "metadata": {
                    "question": question,
                    "model": used_model,
                    "search_mode": search_mode,
                    "num_sources": len(sources),
                },
            }

        except Exception as e:
            # Emit failure event (fire-and-forget)
            execution_time_ms = (time.time() - start_time) * 1000
            if self.event_bus:
                error_type = "search" if "search" in str(e).lower() else "generation"
                event = QuestionAnsweringFailed(
                    question=question,
                    error=str(e),
                    error_type=error_type,
                    search_mode=search_mode,
                    model=used_model,
                    user_id=user_id,
                    metadata={"execution_time_ms": execution_time_ms, "with_sources": True},
                )
                await self.event_bus.emit(event)
            raise

    async def health_check(self) -> bool:
        """
        Check if Ollama is available and responding.

        Returns:
            True if Ollama is healthy, False otherwise
        """
        start_time = time.time()
        try:
            is_healthy = await self.ollama_service.health_check()
            response_time_ms = (time.time() - start_time) * 1000

            # Emit health check event (fire-and-forget)
            if self.event_bus:
                event = LLMHealthCheckPerformed(
                    is_healthy=is_healthy,
                    response_time_ms=response_time_ms,
                )
                await self.event_bus.emit(event)

            return is_healthy
        except Exception as e:
            logger.error(f"Health check failed: {e}")
            response_time_ms = (time.time() - start_time) * 1000

            # Emit health check event with failure (fire-and-forget)
            if self.event_bus:
                event = LLMHealthCheckPerformed(
                    is_healthy=False,
                    response_time_ms=response_time_ms,
                    metadata={"error": str(e)},
                )
                await self.event_bus.emit(event)

            return False

    async def list_available_models(self) -> list[str]:
        """
        List available models from Ollama.

        Returns:
            List of available model names

        Raises:
            OllamaConnectionError: If cannot connect to Ollama
            OllamaAPIError: If API request fails
        """
        try:
            models = await self.ollama_service.list_available_models()

            # Emit models listed event (fire-and-forget)
            if self.event_bus:
                event = LLMModelsListed(
                    models_count=len(models),
                    models=models,
                )
                await self.event_bus.emit(event)

            return models
        except Exception as e:
            logger.error(f"Failed to list models: {e}")
            raise

    async def is_model_available(self, model_name: str) -> bool:
        """
        Check if a specific model is available.

        Args:
            model_name: Name of the model to check

        Returns:
            True if model is available, False otherwise
        """
        try:
            return await self.ollama_service.is_model_available(model_name)
        except Exception as e:
            logger.error(f"Failed to check model availability: {e}")
            return False

    def invalidate_cache(self):
        """
        Invalidate the prompt cache.

        Call this when the document corpus changes to ensure
        cached contexts don't become stale.

        Note: This is a sync method that emits an async event using asyncio.create_task.
        """
        # Get cache size before invalidation
        cache_metrics = self.ollama_service.get_cache_metrics()
        cache_size_before = cache_metrics.get("cache_size", 0)

        # Invalidate cache
        self.ollama_service.invalidate_cache()
        logger.info("LLM cache invalidated")

        # Emit cache invalidation event (fire-and-forget via async task)
        if self.event_bus:
            event = LLMCacheInvalidated(
                reason="manual",
                cache_size_before=cache_size_before,
            )
            # Create async task to emit event (sync-to-async pattern)
            asyncio.create_task(self.event_bus.emit(event))

    def get_cache_metrics(self) -> dict[Any, Any]:
        """
        Get cache performance metrics.

        Returns:
            Dictionary with cache statistics including hits, misses, hit rate,
            and estimated time saved.
        """
        return self.ollama_service.get_cache_metrics()
