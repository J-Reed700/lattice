"""
Q&A API endpoints for RAG-based question answering.

This module provides REST API endpoints for asking questions about documents
in the knowledge base using Retrieval-Augmented Generation (RAG).
"""

from __future__ import annotations

import logging
import time
from typing import TYPE_CHECKING, Any

from fastapi import APIRouter, Body, Depends, Request
from fastapi.responses import StreamingResponse
from slowapi import Limiter
from slowapi.util import get_remote_address

from src.api.dependencies import get_search_service
from src.api.errors import ValidationError
from src.auth.dependencies import get_current_active_user
from src.middleware.csrf import csrf_protect
from src.modules.rag_engine import OllamaUnavailableError, QAEngine, QAError
from src.schemas.llm import AskRequest, AskResponse, SourceDocument

if TYPE_CHECKING:
    from src.auth.models import User
    from src.modules.search_engine import SearchService

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/qa", tags=["qa"])
limiter = Limiter(key_func=get_remote_address)


async def get_qa_engine(
    search_service: SearchService = Depends(get_search_service),
) -> QAEngine:
    """
    Q&A engine dependency.

    Returns an initialized QAEngine instance with search service.

    Args:
        search_service: SearchService instance for document retrieval

    Returns:
        QAEngine: Initialized Q&A engine

    Usage:
        @router.post("/ask")
        async def ask(engine: QAEngine = Depends(get_qa_engine)):
            ...
    """
    return QAEngine(
        search_service=search_service,
        ollama_url="http://localhost:11434",
        model_name="llama3.1:8b",
    )


@router.post("/ask", response_model=AskResponse)
@limiter.limit("10/minute")
async def ask_question(
    http_request: Request,
    request: AskRequest = Body(...),
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    qa_engine: QAEngine = Depends(get_qa_engine),
) -> AskResponse:
    """
    Ask a question about your documents using RAG.

    This endpoint uses Retrieval-Augmented Generation to answer questions by:
    1. Searching for relevant documents using semantic search
    2. Building context from top search results
    3. Generating an answer using a local LLM (Ollama)

    Args:
        request: Question and search parameters
        current_user: Authenticated user (from JWT)
        qa_engine: Q&A engine instance (injected)

    Returns:
        AskResponse: Generated answer with source attribution

    Raises:
        ValidationError: If question is invalid or parameters are out of range
        HTTPException 503: If Ollama service is unavailable
        HTTPException 500: If answer generation fails

    Example:
        ```bash
        curl -X POST http://localhost:8000/api/v1/qa/ask \\
          -H "Authorization: Bearer $TOKEN" \\
          -H "Content-Type: application/json" \\
          -d '{
            "question": "What is machine learning?",
            "max_context_docs": 5,
            "search_mode": "hybrid",
            "temperature": 0.7
          }'
        ```
    """
    start = time.time()

    try:
        # Validate question
        if not request.question or not request.question.strip():
            raise ValidationError("Question cannot be empty")

        # Check Ollama availability
        is_healthy = await qa_engine.health_check()
        if not is_healthy:
            raise OllamaUnavailableError(
                f"Ollama service is not available at {qa_engine.ollama_url}. "
                "Please ensure Ollama is running."
            )

        # Collect answer and metadata
        answer_chunks: list[str] = []
        sources: list[SourceDocument] = []
        metadata: dict[str, Any] = {
            "model": request.model or qa_engine.model_name,
            "search_mode": request.search_mode,
            "max_context_docs": request.max_context_docs,
        }

        # Stream answer (but collect all chunks for non-streaming response)
        async for chunk in qa_engine.ask(
            question=request.question,
            top_k=request.max_context_docs,
            max_context_tokens=2000,
        ):
            if isinstance(chunk, dict):
                # Metadata chunk with sources
                if "sources" in chunk:
                    for idx, source in enumerate(chunk["sources"], 1):
                        sources.append(
                            SourceDocument(
                                id=idx,
                                file_path=source.get("file_path", ""),
                                filename=source.get("filename", ""),
                                score=source.get("score", 0.0),
                                snippet=source.get("snippet"),
                                modified_at=source.get("modified_at"),
                            )
                        )
                if "search_results" in chunk:
                    metadata["search_results_count"] = chunk["search_results"]
            else:
                # Text chunk
                answer_chunks.append(chunk)

        answer = "".join(answer_chunks)

        # Add timing metadata
        execution_time_ms = round((time.time() - start) * 1000, 2)
        metadata["execution_time_ms"] = execution_time_ms

        logger.info(
            "qa_question_answered",
            question=request.question[:100],
            answer_length=len(answer),
            sources_count=len(sources),
            execution_time_ms=execution_time_ms,
        )

        return AskResponse(answer=answer, sources=sources, metadata=metadata)

    except OllamaUnavailableError:
        logger.error(
            "ollama_unavailable",
            ollama_url=qa_engine.ollama_url,
            model=request.model or qa_engine.model_name,
        )
        raise
    except ValidationError:
        raise
    except QAError as e:
        logger.error("qa_error", error=str(e), question=request.question[:100], exc_info=True)
        raise ValidationError(f"Q&A operation failed: {e!s}") from e
    except Exception as e:
        logger.exception("unexpected_qa_error", question=request.question[:100])
        raise ValidationError("Q&A operation failed unexpectedly") from e


@router.post("/ask/stream")
@limiter.limit("10/minute")
async def ask_question_stream(
    http_request: Request,
    request: AskRequest = Body(...),
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    qa_engine: QAEngine = Depends(get_qa_engine),
) -> StreamingResponse:
    """
    Ask a question with streaming response.

    Similar to /ask but streams the answer as it's generated, providing
    a better user experience for long answers.

    Args:
        request: Question and search parameters
        current_user: Authenticated user (from JWT)
        qa_engine: Q&A engine instance (injected)

    Returns:
        StreamingResponse: Server-sent events with answer chunks

    Raises:
        ValidationError: If question is invalid
        HTTPException 503: If Ollama service is unavailable

    Example:
        ```bash
        curl -X POST http://localhost:8000/api/v1/qa/ask/stream \\
          -H "Authorization: Bearer $TOKEN" \\
          -H "Content-Type: application/json" \\
          -d '{"question": "Explain neural networks"}' \\
          --no-buffer
        ```
    """
    try:
        # Validate question
        if not request.question or not request.question.strip():
            raise ValidationError("Question cannot be empty")

        # Check Ollama availability
        is_healthy = await qa_engine.health_check()
        if not is_healthy:
            raise OllamaUnavailableError(
                f"Ollama service is not available at {qa_engine.ollama_url}"
            )

        async def stream_generator():
            """Generate SSE events for answer streaming."""
            try:
                async for chunk in qa_engine.ask(
                    question=request.question,
                    top_k=request.max_context_docs,
                    max_context_tokens=2000,
                ):
                    if isinstance(chunk, dict):
                        # Metadata event
                        yield f"event: metadata\ndata: {chunk}\n\n"
                    else:
                        # Text chunk event
                        yield f"event: answer\ndata: {chunk}\n\n"

                # Send completion event
                yield "event: done\ndata: {}\n\n"

            except Exception as e:
                logger.exception("streaming_error", question=request.question[:100])
                yield f'event: error\ndata: {{"error": "{e!s}"}}\n\n'

        logger.info("qa_streaming_started", question=request.question[:100])

        return StreamingResponse(
            stream_generator(),
            media_type="text/event-stream",
            headers={
                "Cache-Control": "no-cache",
                "Connection": "keep-alive",
                "X-Accel-Buffering": "no",
            },
        )

    except (OllamaUnavailableError, ValidationError):
        raise
    except Exception as e:
        logger.exception("unexpected_streaming_error", question=request.question[:100])
        raise ValidationError("Streaming Q&A operation failed") from e


@router.post("/batch", response_model=list[AskResponse])
@limiter.limit("5/minute")
async def ask_batch(
    request: Request,
    questions: list[AskRequest] = Body(..., min_length=1, max_length=10),
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    qa_engine: QAEngine = Depends(get_qa_engine),
) -> list[AskResponse]:
    """
    Answer multiple questions in a single batch request.

    Processes up to 10 questions in parallel for efficiency. Each question
    is answered independently using the same RAG pipeline.

    Args:
        questions: List of 1-10 questions to answer
        current_user: Authenticated user (from JWT)
        qa_engine: Q&A engine instance (injected)

    Returns:
        list[AskResponse]: Answers for each question in order

    Raises:
        ValidationError: If batch is empty or exceeds 10 questions
        HTTPException 503: If Ollama service is unavailable

    Example:
        ```bash
        curl -X POST http://localhost:8000/api/v1/qa/batch \\
          -H "Authorization: Bearer $TOKEN" \\
          -H "Content-Type: application/json" \\
          -d '[
            {"question": "What is Python?"},
            {"question": "What is JavaScript?"}
          ]'
        ```
    """
    try:
        # Validate batch size
        if not questions:
            raise ValidationError("Batch cannot be empty")
        if len(questions) > 10:
            raise ValidationError("Batch size cannot exceed 10 questions")

        # Check Ollama availability
        is_healthy = await qa_engine.health_check()
        if not is_healthy:
            raise OllamaUnavailableError(
                f"Ollama service is not available at {qa_engine.ollama_url}"
            )

        # Process each question (could be parallelized with asyncio.gather)
        responses: list[AskResponse] = []
        for question_req in questions:
            try:
                # Use the single question endpoint logic
                answer_chunks: list[str] = []
                sources: list[SourceDocument] = []
                metadata: dict[str, Any] = {
                    "model": question_req.model or qa_engine.model_name,
                    "search_mode": question_req.search_mode,
                }

                async for chunk in qa_engine.ask(
                    question=question_req.question,
                    top_k=question_req.max_context_docs,
                    max_context_tokens=2000,
                ):
                    if isinstance(chunk, dict):
                        if "sources" in chunk:
                            for idx, source in enumerate(chunk["sources"], 1):
                                sources.append(
                                    SourceDocument(
                                        id=idx,
                                        file_path=source.get("file_path", ""),
                                        filename=source.get("filename", ""),
                                        score=source.get("score", 0.0),
                                        snippet=source.get("snippet"),
                                        modified_at=source.get("modified_at"),
                                    )
                                )
                    else:
                        answer_chunks.append(chunk)

                answer = "".join(answer_chunks)
                responses.append(AskResponse(answer=answer, sources=sources, metadata=metadata))

            except Exception as e:
                logger.error(
                    "batch_question_failed",
                    question=question_req.question[:100],
                    error=str(e),
                    exc_info=True,
                )
                # Add error response
                responses.append(
                    AskResponse(
                        answer=f"Error: {e!s}",
                        sources=[],
                        metadata={"error": True, "error_message": str(e)},
                    )
                )

        logger.info("qa_batch_completed", batch_size=len(questions), responses_count=len(responses))

        return responses

    except (OllamaUnavailableError, ValidationError):
        raise
    except Exception as e:
        logger.exception("unexpected_batch_error", batch_size=len(questions))
        raise ValidationError("Batch Q&A operation failed") from e


__all__ = ["router"]
