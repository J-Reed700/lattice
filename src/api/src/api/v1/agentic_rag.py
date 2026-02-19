"""
API routes for Agentic RAG endpoints.
"""

from __future__ import annotations

from collections.abc import AsyncIterator
import json
import logging
import time

from fastapi import APIRouter, Depends, HTTPException, status
from fastapi.responses import StreamingResponse
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.errors import ValidationError
from src.config import Settings, get_settings
from src.db import get_session
from src.modules.embedding_generator import EmbeddingService
from src.schemas.agentic_rag import (
    AgenticComparisonRequest,
    AgenticComparisonResponse,
    AgenticQueryRequest,
    AgenticQueryResponse,
    ComparisonResult,
    ReasoningStepResponse,
    SourceDocumentResponse,
)
from src.modules.rag_engine import AgenticRAGService
from src.modules.search_engine import SearchService
from src.services.llm.ollama_service import OllamaService
from src.services.llm.service import LLMService

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/agentic-rag", tags=["agentic-rag"])


async def get_agentic_rag_service(
    session: AsyncSession = Depends(get_session),
    settings: Settings = Depends(get_settings),
) -> AsyncIterator[AgenticRAGService]:
    """Get initialized agentic RAG service with proper cleanup."""
    embedding_service = EmbeddingService()
    search_service = SearchService(session, embedding_service)

    ollama_service = OllamaService(
        base_url=settings.ollama_base_url,
        default_model=settings.ollama_default_model,
        timeout=settings.ollama_timeout,
        stream_timeout=settings.ollama_stream_timeout,
    )
    await ollama_service.initialize()

    try:
        yield AgenticRAGService(
            search_service=search_service,
            ollama_service=ollama_service,
            model=settings.ollama_default_model,
            max_iterations=5,
            timeout=60,
            enable_reflection=True,
            enable_correction=True,
        )
    finally:
        await ollama_service.cleanup()


async def get_simple_llm_service(
    session: AsyncSession = Depends(get_session),
    settings: Settings = Depends(get_settings),
) -> AsyncIterator[LLMService]:
    """Get initialized simple LLM service for comparison with proper cleanup."""
    embedding_service = EmbeddingService()
    search_service = SearchService(session, embedding_service)

    llm_service = LLMService(
        search_service=search_service,
        ollama_url=settings.ollama_base_url,
        model=settings.ollama_default_model,
        timeout=settings.ollama_timeout,
        stream_timeout=settings.ollama_stream_timeout,
    )
    await llm_service.initialize()

    try:
        yield llm_service
    finally:
        await llm_service.cleanup()


@router.post("/ask", response_model=AgenticQueryResponse)
async def ask_agentic(
    request: AgenticQueryRequest,
    service: AgenticRAGService = Depends(get_agentic_rag_service),
):
    """
    Answer a question using Agentic RAG with reasoning and self-correction.

    This endpoint uses an advanced RAG system where the LLM acts as an autonomous agent:
    - Plans what information it needs
    - Executes multiple search rounds to gather comprehensive context
    - Reasons about the gathered information
    - Reflects on its own answer quality
    - Corrects and improves the answer if needed

    **When to use Agentic RAG:**
    - Complex questions requiring multi-step reasoning
    - Questions that need information from multiple documents
    - Comparative analysis (e.g., "Compare X and Y")
    - When you need high-quality, well-reasoned answers
    - When transparency of reasoning is important

    **When to use Simple RAG (POST /llm/ask):**
    - Simple lookup questions
    - When speed is more important than depth
    - When you want lower latency (2-3x faster)
    - Straightforward factual queries

    **Performance:**
    - Latency: 2-3x slower than simple RAG (5-15s vs 2-5s)
    - Accuracy: Significantly higher for complex questions
    - Cost: 3-5x more LLM calls
    - Context: Uses more comprehensive document retrieval

    Args:
        request: Query request with question and configuration

    Returns:
        Complete response with answer, reasoning trace, and sources

    Raises:
        HTTPException 400: Invalid query
        HTTPException 503: LLM service unavailable
        HTTPException 500: Processing error
    """
    if not request.query or len(request.query.strip()) == 0:
        raise ValidationError(message="Query cannot be empty", details={"field": "query"})

    logger.info(f"Agentic RAG request: '{request.query[:100]}...'")

    try:
        if request.streaming:
            return await _handle_streaming_agentic(request, service)

        response = await service.ask_question(
            question=request.query,
            search_mode=request.search_mode,
            temperature=request.temperature,
            streaming=False,
        )

        reasoning_steps = [
            ReasoningStepResponse(
                phase=step.thought.phase.value,
                thought=step.thought.content,
                action=step.action.tool.value if step.action else None,
                action_input=step.action.tool_input if step.action else None,
                observation=(
                    f"Found {len(step.observation.result)} documents"
                    if step.observation and step.observation.success
                    else step.observation.error
                    if step.observation
                    else None
                ),
                iteration=step.iteration,
                timestamp=step.thought.timestamp,
            )
            for step in response.reasoning_steps
        ]

        sources = [
            SourceDocumentResponse(
                file_path=source.file_path,
                filename=source.filename,
                score=source.score,
                snippet=source.snippet,
            )
            for source in response.sources
        ]

        return AgenticQueryResponse(
            answer=response.answer,
            reasoning_steps=reasoning_steps,
            sources=sources,
            reflection=response.reflection,
            corrected=response.corrected,
            total_iterations=response.total_iterations,
            total_time_ms=response.total_time_ms,
            metadata=response.metadata,
        )

    except Exception as e:
        logger.error(f"Agentic RAG error: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Agentic RAG failed: {e!s}",
        )


async def _handle_streaming_agentic(
    request: AgenticQueryRequest,
    service: AgenticRAGService,
) -> StreamingResponse:
    """Handle streaming agentic RAG response."""

    async def generate_stream() -> AsyncIterator[str]:
        try:
            async for event in service.ask_question_stream(
                question=request.query,
                search_mode=request.search_mode,
                temperature=request.temperature,
            ):
                event_json = json.dumps(event)
                yield f"data: {event_json}\n\n"

        except Exception as e:
            logger.error(f"Streaming error: {e}", exc_info=True)
            error_event = {"type": "error", "content": str(e)}
            yield f"data: {json.dumps(error_event)}\n\n"

    return StreamingResponse(
        generate_stream(),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache",
            "Connection": "keep-alive",
            "X-Accel-Buffering": "no",
        },
    )


@router.post("/compare", response_model=AgenticComparisonResponse)
async def compare_rag_approaches(
    request: AgenticComparisonRequest,
    agentic_service: AgenticRAGService = Depends(get_agentic_rag_service),
    simple_service: LLMService = Depends(get_simple_llm_service),
):
    """
    Compare Simple RAG vs Agentic RAG on the same question.

    This endpoint runs both approaches on the same question and provides
    a detailed comparison of:
    - Answer quality and comprehensiveness
    - Processing time and latency
    - Number of sources used
    - Reasoning transparency

    Useful for:
    - Understanding when to use each approach
    - Benchmarking performance
    - Evaluating answer quality differences
    - Cost-benefit analysis

    **Example Use Cases:**

    Simple question (Simple RAG wins):
    - "What is the filename of the budget document?"
    - Simple RAG: 2s, correct answer
    - Agentic RAG: 7s, correct answer (overkill)

    Complex question (Agentic RAG wins):
    - "Compare Q3 and Q4 revenue growth and identify key trends"
    - Simple RAG: 3s, superficial answer from 2 sources
    - Agentic RAG: 9s, comprehensive analysis from 6 sources

    Args:
        request: Question to test with both approaches

    Returns:
        Comparison results with metrics and analysis

    Raises:
        HTTPException 400: Invalid query
        HTTPException 500: Processing error
    """
    if not request.query or len(request.query.strip()) == 0:
        raise ValidationError(message="Query cannot be empty", details={"field": "query"})

    logger.info(f"Comparing RAG approaches for: '{request.query[:100]}...'")

    try:
        start_simple = time.time()
        simple_result = await simple_service.ask_question_with_sources(
            question=request.query,
            search_mode=request.search_mode,
            max_context_docs=5,
        )
        simple_time_ms = int((time.time() - start_simple) * 1000)

        start_agentic = time.time()
        agentic_result = await agentic_service.ask_question(
            question=request.query,
            search_mode=request.search_mode,
        )
        agentic_time_ms = int((time.time() - start_agentic) * 1000)

        time_multiplier = round(agentic_time_ms / simple_time_ms, 2)
        additional_sources = len(agentic_result.sources) - len(simple_result["sources"])

        comparison = {
            "time_multiplier": time_multiplier,
            "agentic_slower_by_ms": agentic_time_ms - simple_time_ms,
            "simple_sources_count": len(simple_result["sources"]),
            "agentic_sources_count": len(agentic_result.sources),
            "additional_sources": additional_sources,
            "agentic_iterations": agentic_result.total_iterations,
            "agentic_corrected": agentic_result.corrected,
            "recommendation": _generate_recommendation(
                time_multiplier=time_multiplier,
                additional_sources=additional_sources,
                corrected=agentic_result.corrected,
            ),
        }

        return AgenticComparisonResponse(
            query=request.query,
            simple_rag=ComparisonResult(
                answer=simple_result["answer"],
                time_ms=simple_time_ms,
                sources_count=len(simple_result["sources"]),
                metadata=simple_result.get("metadata", {}),
            ),
            agentic_rag=ComparisonResult(
                answer=agentic_result.answer,
                time_ms=agentic_time_ms,
                sources_count=len(agentic_result.sources),
                metadata={
                    "iterations": agentic_result.total_iterations,
                    "corrected": agentic_result.corrected,
                    "reflection": agentic_result.reflection,
                },
            ),
            comparison=comparison,
        )

    except Exception as e:
        logger.error(f"Comparison error: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR, detail=f"Comparison failed: {e!s}"
        )


def _generate_recommendation(
    time_multiplier: float,
    additional_sources: int,
    corrected: bool,
) -> str:
    """Generate recommendation on which approach to use."""
    if time_multiplier < 2.0 and additional_sources <= 1:
        return (
            "Question is simple enough for regular RAG. "
            "Agentic RAG provides minimal benefit at 2x cost."
        )
    if time_multiplier > 4.0 and additional_sources < 2 and not corrected:
        return (
            "Agentic RAG is significantly slower without meaningful benefit. "
            "Use simple RAG for this type of question."
        )
    if additional_sources >= 3 or corrected:
        return (
            "Agentic RAG provides significantly more comprehensive answer. "
            "The extra time is justified for this complex question."
        )
    return (
        "Both approaches work reasonably well. "
        "Use agentic RAG if answer quality is more important than speed."
    )


@router.get("/health")
async def agentic_health_check(
    service: AgenticRAGService = Depends(get_agentic_rag_service),
):
    """
    Check if agentic RAG service is available.

    Returns:
        Health status of the service
    """
    try:
        is_healthy = await service.ollama_service.health_check()

        return {
            "status": "healthy" if is_healthy else "unhealthy",
            "ollama_available": is_healthy,
            "max_iterations": service.max_iterations,
            "timeout_seconds": service.timeout,
            "reflection_enabled": service.enable_reflection,
            "correction_enabled": service.enable_correction,
        }

    except Exception as e:
        logger.error(f"Health check error: {e}", exc_info=True)
        return {"status": "unhealthy", "error": str(e)}
