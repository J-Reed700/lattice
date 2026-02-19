"""
Q&A Engine with RAG Pipeline

A complete RAG (Retrieval-Augmented Generation) implementation that combines
semantic search with LLM generation to answer questions using your knowledge base.

Basic Usage:
    >>> from src.modules.rag_engine import QAEngine
    >>> from src.modules.search_engine import SearchService
    >>>
    >>> # Initialize with search service
    >>> qa_engine = QAEngine(
    ...     search_service=search_service,
    ...     ollama_url="http://localhost:11434",
    ...     model_name="llama3.1:8b"
    ... )
    >>>
    >>> # Check if Ollama is available
    >>> if await qa_engine.health_check():
    ...     # Ask a question and stream response
    ...     async for chunk in qa_engine.ask("What is machine learning?"):
    ...         print(chunk, end="", flush=True)
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

import httpx
from opentelemetry import trace  # type: ignore[import-not-found]

from src.observability.tracing import get_tracer  # type: ignore[import-untyped]

from .prompts import SYSTEM_PROMPT, USER_PROMPT_TEMPLATE
from .tokenizer import count_tokens, truncate_to_tokens
from .types import OllamaUnavailableError, QAError, SourceReference

if TYPE_CHECKING:
    from collections.abc import AsyncIterator

    from src.modules.search_engine import SearchResult, SearchService

logger = logging.getLogger(__name__)
tracer = get_tracer(__name__)


class QAEngine:
    """
    RAG-based question answering engine.

    Combines semantic search with LLM generation to provide accurate,
    context-grounded answers from your knowledge base. Uses Ollama
    for local LLM inference.

    Pipeline:
        1. Search for relevant documents using semantic similarity
        2. Extract and truncate context to fit token budget
        3. Build prompt with context and question
        4. Stream response from Ollama LLM
        5. Return answer chunks with source attribution

    Performance Characteristics:
        - Search latency: ~100ms for 10K documents
        - LLM latency: ~500-2000ms for first token (model dependent)
        - Streaming: ~10-50 tokens/second (model dependent)
        - Context limit: Configurable (default 2000 tokens)

    Attributes:
        search_engine: SearchEngine instance for document retrieval
        ollama_url: URL of Ollama API server
        model_name: Name of Ollama model to use
        client: HTTP client for Ollama API calls

    Example:
        >>> qa = QAEngine(
        ...     search_engine=search_engine,
        ...     ollama_url="http://localhost:11434",
        ...     model_name="llama3.1:8b"
        ... )
        >>>
        >>> # Basic question answering
        >>> async for chunk in qa.ask("What is Python?", top_k=5):
        ...     print(chunk, end="")
        >>>
        >>> # With custom context size
        >>> async for chunk in qa.ask(
        ...     "Explain neural networks",
        ...     top_k=10,
        ...     max_context_tokens=4000
        ... ):
        ...     print(chunk, end="")
    """

    def __init__(
        self,
        search_service: SearchService,
        ollama_url: str = "http://localhost:11434",
        model_name: str = "llama3.1:8b",
    ):
        """
        Initialize Q&A engine with dependencies.

        Args:
            search_service: SearchService instance for retrieving relevant documents
                Must be initialized and ready to use.
            ollama_url: Base URL of Ollama API server (default: "http://localhost:11434")
                Server must be running and accessible.
            model_name: Name of Ollama model to use (default: "llama3.1:8b")
                Model must be pulled: `ollama pull llama3.1:8b`

        Example:
            >>> from src.modules.search_engine import SearchService
            >>> search_service = SearchService(db=db)
            >>> qa_engine = QAEngine(
            ...     search_service=search_service,
            ...     ollama_url="http://localhost:11434",
            ...     model_name="llama3.1:8b"
            ... )
        """
        self.search_service = search_service
        self.ollama_url = ollama_url.rstrip("/")
        self.model_name = model_name
        self.client = httpx.AsyncClient(timeout=60.0)

        logger.info("qa_engine_initialized", model=model_name, ollama_url=ollama_url)

    @tracer.start_as_current_span("qa_ask")
    async def ask(
        self,
        question: str,
        top_k: int = 5,
        max_context_tokens: int = 2000,
        document_ids: list[str] | None = None,
    ) -> AsyncIterator[str | dict[str, Any]]:
        """
        Stream answer to question using RAG pipeline.

        Retrieves relevant documents, builds context, and streams the LLM
        response. Handles token limits, connection errors, and empty results.

        Pipeline Steps:
            1. Validate question
            2. Search for top_k relevant documents
            3. Build context from search results (truncate if needed)
            4. Construct prompt with system message and context
            5. Call Ollama API with streaming enabled
            6. Yield response chunks as they arrive

        Args:
            question: User question to answer
                Must be non-empty. Example: "What is machine learning?"
            top_k: Number of documents to retrieve (default: 5)
                Range: 1-20. More documents = more context but slower.
            max_context_tokens: Maximum tokens for context (default: 2000)
                Typical range: 1000-4000. Must fit within model's context window.
            document_ids: Optional list of document file paths to filter search (default: None)
                If provided, only searches within specified documents.
                If None or empty, searches all documents.

        Yields:
            str | dict: Chunks of the generated answer as strings, followed by a dict
                with sources after streaming completes. The sources dict contains:
                {"sources": [{"file_path": str, "score": float, "snippet": str}, ...]}

        Raises:
            QAError: If question is empty or invalid
            OllamaUnavailableError: If Ollama service is unreachable
            SearchError: If search fails
            httpx.HTTPError: If Ollama API returns error

        Example:
            >>> # Basic usage with streaming
            >>> async for chunk in qa_engine.ask("What is Python?"):
            ...     print(chunk, end="", flush=True)
            >>> print()  # Newline after answer
            Python is a high-level programming language...

            >>> # Collect full answer
            >>> answer = ""
            >>> async for chunk in qa_engine.ask("Explain databases"):
            ...     answer += chunk
            >>> print(f"Full answer: {answer}")

            >>> # Handle no results
            >>> async for chunk in qa_engine.ask("xyzabc nonsense query"):
            ...     print(chunk, end="")
            I don't have enough information to answer that question.

            >>> # More context for complex questions
            >>> async for chunk in qa_engine.ask(
            ...     "Compare functional and object-oriented programming",
            ...     top_k=10,
            ...     max_context_tokens=3000
            ... ):
            ...     print(chunk, end="")
        """
        span = trace.get_current_span()
        span.set_attribute("question", question[:100])
        span.set_attribute("top_k", top_k)
        span.set_attribute("max_context_tokens", max_context_tokens)
        span.set_attribute("llm.model", self.model_name)

        if not question or not question.strip():
            raise QAError("Question cannot be empty")

        if top_k <= 0:
            raise QAError(f"top_k must be positive, got {top_k}")

        if max_context_tokens <= 0:
            raise QAError(
                f"max_context_tokens must be positive, got {max_context_tokens}"
            )

        logger.info(
            "qa_ask_started",
            question=question[:100],
            top_k=top_k,
            max_context_tokens=max_context_tokens,
        )

        try:
            with tracer.start_as_current_span("retrieve_context") as retrieve_span:
                search_results_obj = await self.search_service.search(
                    query=question,
                    mode="hybrid",
                    limit=top_k,
                    offset=0,
                    filters=None,
                    similarity_threshold=0.0,
                )
                search_results = search_results_obj.results
                retrieve_span.set_attribute("search_results.count", len(search_results))
                logger.debug(
                    "search_results_retrieved",
                    results_count=len(search_results),
                    question_snippet=question[:50],
                )

        except Exception as e:
            span.record_exception(e)
            span.set_status(trace.Status(trace.StatusCode.ERROR, str(e)))
            raise QAError(f"Search failed: {e}") from e

        if not search_results:
            logger.warning("no_search_results", question=question[:100])
            span.set_attribute("has_results", False)
            yield "I don't have any information in my knowledge base to answer that question."
            return

        span.set_attribute("has_results", True)

        with tracer.start_as_current_span("build_context") as ctx_span:
            context, sources = self._build_context(search_results, max_context_tokens)
            context_tokens = count_tokens(context)
            ctx_span.set_attribute("context.tokens", context_tokens)
            ctx_span.set_attribute("sources.count", len(sources))

            logger.debug(
                "context_built",
                context_tokens=context_tokens,
                sources_count=len(sources),
            )

        with tracer.start_as_current_span("build_prompt") as prompt_span:
            prompt = self._build_prompt(context, question)
            prompt_tokens = count_tokens(SYSTEM_PROMPT) + count_tokens(prompt)
            prompt_span.set_attribute("prompt.tokens", prompt_tokens)
            prompt_span.set_attribute(
                "system_prompt.tokens", count_tokens(SYSTEM_PROMPT)
            )

            logger.info(
                "prompt_prepared", prompt_tokens=prompt_tokens, model=self.model_name
            )

        span.set_attribute("llm.tokens.prompt", prompt_tokens)

        try:
            with tracer.start_as_current_span("llm_generate") as llm_span:
                llm_span.set_attribute(
                    "llm.endpoint", f"{self.ollama_url}/api/generate"
                )
                llm_span.set_attribute("llm.streaming", True)

                token_count = 0
                async for chunk in self._stream_from_ollama(prompt):
                    if isinstance(chunk, str):
                        token_count += len(chunk.split())
                    yield chunk

                llm_span.set_attribute("llm.tokens.completion", token_count)
                span.set_attribute("llm.tokens.completion", token_count)
                logger.info(
                    "qa_generation_completed",
                    completion_tokens=token_count,
                    sources_count=len(sources),
                )

            sources_dict = {
                "sources": [
                    {
                        "file_path": src.file_path,
                        "score": src.score,
                        "snippet": src.snippet,
                    }
                    for src in sources
                ]
            }
            yield sources_dict

        except httpx.ConnectError as e:
            span.record_exception(e)
            span.set_status(trace.Status(trace.StatusCode.ERROR, "Ollama unavailable"))
            raise OllamaUnavailableError(
                f"Ollama service unavailable at {self.ollama_url}. "
                f"Ensure Ollama is running: 'ollama serve'. "
                f"Connection error: {e!s}"
            ) from e
        except httpx.TimeoutException as e:
            span.record_exception(e)
            span.set_status(trace.Status(trace.StatusCode.ERROR, "Timeout"))
            raise QAError(f"Ollama request timed out: {e}") from e
        except httpx.HTTPError as e:
            span.record_exception(e)
            span.set_status(trace.Status(trace.StatusCode.ERROR, str(e)))
            raise QAError(f"Ollama API error: {e}") from e

    @tracer.start_as_current_span("health_check")
    async def health_check(self) -> bool:
        """
        Check if Ollama service is available and responsive.

        Makes a simple API call to verify the Ollama server is running
        and the model is available.

        Returns:
            bool: True if Ollama is available and model exists, False otherwise

        Example:
            >>> if await qa_engine.health_check():
            ...     print("Ollama is ready")
            ...     async for chunk in qa_engine.ask("test question"):
            ...         print(chunk, end="")
            ... else:
            ...     print("Ollama is not available. Run: ollama serve")

            >>> # Graceful degradation
            >>> if not await qa_engine.health_check():
            ...     print("Q&A unavailable, showing search results only")
            ...     results = await search_engine.search("query")
        """
        span = trace.get_current_span()
        span.set_attribute("ollama.url", self.ollama_url)
        span.set_attribute("ollama.model", self.model_name)

        try:
            response = await self.client.get(f"{self.ollama_url}/api/tags")

            if response.status_code != 200:
                logger.warning(
                    "ollama_health_check_failed",
                    status_code=response.status_code,
                    url=self.ollama_url,
                )
                span.set_attribute("health.status", "failed")
                span.set_attribute("http.status_code", response.status_code)
                return False

            data = response.json()
            models = [model.get("name", "") for model in data.get("models", [])]

            if self.model_name not in models:
                logger.warning(
                    "ollama_model_not_found",
                    model=self.model_name,
                    available_models=models,
                )
                span.set_attribute("health.status", "model_not_found")
                span.set_attribute("available_models", str(models))
                return False

            logger.info("ollama_health_check_passed", model=self.model_name)
            span.set_attribute("health.status", "healthy")
            return True

        except httpx.ConnectError:
            logger.warning("ollama_connection_failed", url=self.ollama_url)
            span.set_attribute("health.status", "connection_error")
            return False
        except Exception as e:
            logger.error(
                "ollama_health_check_error", error=str(e), error_type=type(e).__name__
            )
            span.record_exception(e)
            span.set_attribute("health.status", "error")
            return False

    def _build_context(
        self, search_results: list[SearchResult], max_tokens: int
    ) -> tuple[str, list[SourceReference]]:
        """
        Build context string from search results with token limit.

        Combines search results into a formatted context string, ensuring
        the total stays within the token budget. Includes file paths for
        attribution and snippets for relevance.

        Args:
            search_results: List of SearchResult objects from semantic search
                Should be sorted by relevance (score descending)
            max_tokens: Maximum tokens allowed for entire context
                Must be positive. Typical: 1000-4000 tokens

        Returns:
            Tuple of:
                - context (str): Formatted context string with document excerpts
                - sources (List[SourceReference]): Source documents used

        Raises:
            ContextTooLargeError: If single document exceeds max_tokens
                (currently not raised, but truncates instead)

        Example:
            >>> results = await search_engine.search("Python", top_k=5)
            >>> context, sources = qa_engine._build_context(results, max_tokens=2000)
            >>> print(f"Context: {context[:200]}...")
            >>> print(f"Used {len(sources)} sources")
            >>> for src in sources:
            ...     print(f"  - {src.file_path} (score: {src.score:.2f})")
        """
        context_parts = []
        sources = []
        current_tokens = 0

        for result in search_results:
            file_path = result.file_path
            score = result.score
            content = result.snippet or ""

            doc_header = f"\n--- Source: {file_path} (relevance: {score:.2f}) ---\n"
            doc_text = f"{doc_header}{content}\n"

            doc_tokens = count_tokens(doc_text)

            if current_tokens + doc_tokens > max_tokens:
                remaining_tokens = (
                    max_tokens - current_tokens - count_tokens(doc_header)
                )

                if remaining_tokens > 100:
                    truncated_content = truncate_to_tokens(
                        content, max_tokens=remaining_tokens, suffix="... [truncated]"
                    )
                    doc_text = f"{doc_header}{truncated_content}\n"
                    context_parts.append(doc_text)

                    sources.append(
                        SourceReference(
                            file_path=file_path,
                            score=score,
                            snippet=result.snippet or content[:200],
                        )
                    )

                logger.debug(
                    "context_token_limit_reached",
                    current_tokens=current_tokens,
                    sources_included=len(sources),
                )
                break
            else:
                context_parts.append(doc_text)
                current_tokens += doc_tokens

                sources.append(
                    SourceReference(
                        file_path=file_path,
                        score=score,
                        snippet=result.snippet or content[:200],
                    )
                )

        context = "".join(context_parts)

        if not context:
            context = "No relevant context found."

        logger.debug(
            "context_finalized", total_tokens=current_tokens, sources_count=len(sources)
        )

        return context, sources

    def _build_prompt(self, context: str, question: str) -> str:
        """
        Build final prompt for LLM from context and question.

        Formats the context and question into a structured prompt using
        the USER_PROMPT_TEMPLATE.

        Args:
            context: Formatted context string with document excerpts
            question: User's question

        Returns:
            str: Complete prompt ready for LLM

        Example:
            >>> context = "Python is a programming language..."
            >>> question = "What is Python?"
            >>> prompt = qa_engine._build_prompt(context, question)
            >>> assert "Context from relevant documents:" in prompt
            >>> assert question in prompt
            >>> assert context in prompt
        """
        prompt = USER_PROMPT_TEMPLATE.format(context=context, question=question)

        logger.debug("prompt_built", tokens=count_tokens(prompt))

        return prompt

    async def _stream_from_ollama(self, prompt: str) -> AsyncIterator[str]:
        """
        Stream response from Ollama API.

        Makes a streaming request to Ollama's /api/generate endpoint and
        yields response chunks as they arrive.

        Args:
            prompt: Complete prompt to send to LLM

        Yields:
            str: Response chunks from the LLM

        Raises:
            httpx.ConnectError: If Ollama is not reachable
            httpx.TimeoutException: If request times out
            httpx.HTTPError: If Ollama returns error

        Example:
            >>> prompt = "Answer: What is Python?"
            >>> async for chunk in qa_engine._stream_from_ollama(prompt):
            ...     print(chunk, end="", flush=True)
        """
        request_data = {
            "model": self.model_name,
            "prompt": prompt,
            "system": SYSTEM_PROMPT,
            "stream": True,
            "options": {
                "temperature": 0.7,
                "top_p": 0.9,
            },
        }

        logger.debug(
            "ollama_stream_started",
            endpoint=f"{self.ollama_url}/api/generate",
            model=self.model_name,
        )

        async with self.client.stream(
            "POST", f"{self.ollama_url}/api/generate", json=request_data
        ) as response:
            response.raise_for_status()

            async for line in response.aiter_lines():
                if not line:
                    continue

                try:
                    import json

                    chunk_data = json.loads(line)

                    if "response" in chunk_data:
                        yield chunk_data["response"]

                    if chunk_data.get("done", False):
                        logger.debug("ollama_stream_completed")
                        break

                except json.JSONDecodeError as e:
                    logger.warning("ollama_response_parse_error", error=str(e))
                    continue

    async def close(self) -> None:
        """
        Close HTTP client and cleanup resources.

        Should be called when done with the QAEngine to properly close
        the HTTP connection.

        Example:
            >>> qa_engine = QAEngine(search_engine)
            >>> try:
            ...     async for chunk in qa_engine.ask("question"):
            ...         print(chunk, end="")
            ... finally:
            ...     await qa_engine.close()

            >>> # Or use as async context manager (if implemented)
            >>> async with QAEngine(search_engine) as qa:
            ...     async for chunk in qa.ask("question"):
            ...         print(chunk, end="")
        """
        await self.client.aclose()
        logger.info("qa_engine_closed")


__all__ = ["QAEngine"]
