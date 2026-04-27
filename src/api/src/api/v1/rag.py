"""
Advanced RAG API endpoints for agentic query processing.

This module provides REST API endpoints for advanced RAG patterns including:
- Self-RAG: Self-reflective RAG with answer verification
- CRAG: Corrective RAG with fallback sources
- Multi-Step: Complex question decomposition
- Agentic: Unified interface with automatic mode selection
"""

from __future__ import annotations

import json
import logging
import time
from datetime import UTC, datetime
from typing import TYPE_CHECKING, Any

from fastapi import APIRouter, Body, Depends, Request
from fastapi.responses import StreamingResponse
from slowapi import Limiter
from slowapi.util import get_remote_address

from src.api.dependencies import get_search_service
from src.api.errors import ValidationError
from src.auth.dependencies import get_current_active_user
from src.config.settings import get_settings
from src.middleware.csrf import csrf_protect
from src.modules.rag_engine import AgenticRAG
from src.modules.rag_engine.types import RAGMode
from src.modules.rag_engine import OllamaUnavailableError
from src.schemas.agentic_rag import (
    AgenticQueryRequest,
    AgenticQueryResponse,
    ReasoningStepResponse,
    SourceDocumentResponse,
)

if TYPE_CHECKING:
    from src.auth.models import User
    from src.modules.search_engine import SearchService

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/rag", tags=["rag"])
limiter = Limiter(key_func=get_remote_address)


async def get_agentic_rag(
    search_service: SearchService = Depends(get_search_service),
) -> AgenticRAG:
    """
    Agentic RAG dependency.

    Returns an initialized AgenticRAG instance with search service.

    Args:
        search_service: SearchService instance for document retrieval

    Returns:
        AgenticRAG: Initialized agentic RAG engine

    Usage:
        @router.post("/query")
        async def query(rag: AgenticRAG = Depends(get_agentic_rag)):
            ...
    """
    settings = get_settings()
    return AgenticRAG(
        search_engine=search_service,
        ollama_url=settings.ollama_base_url,
        model_name=settings.llm_model,
        enable_web_search=True,  # Enable DuckDuckGo fallback for CRAG
        max_parallel_llm_calls=settings.ollama_max_parallel_requests,
        max_parallel_search_calls=settings.deep_research_max_parallel_search,
    )


@router.post("/query", response_model=AgenticQueryResponse)
@limiter.limit("5/minute")
async def agentic_query(
    http_request: Request,
    request: AgenticQueryRequest = Body(...),
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    rag_engine: AgenticRAG = Depends(get_agentic_rag),
) -> AgenticQueryResponse:
    """
    Execute an agentic RAG query with advanced reasoning.

    This endpoint provides cutting-edge RAG capabilities:
    - **Adaptive Mode**: Automatically selects the best RAG strategy
    - **Self-RAG**: Self-reflective with answer verification
    - **CRAG**: Corrective retrieval with web search fallback
    - **Multi-Step**: Decomposes complex questions into sub-questions

    The engine uses ReAct-style reasoning with:
    1. Planning: Analyze question and choose strategy
    2. Retrieval: Search for relevant documents
    3. Generation: Generate answer with citations
    4. Reflection: Evaluate answer quality (optional)
    5. Correction: Refine answer if needed (optional)

    Args:
        request: Query parameters including question and mode
        current_user: Authenticated user (from JWT)
        rag_engine: Agentic RAG engine instance (injected)

    Returns:
        AgenticQueryResponse: Answer with reasoning steps and sources

    Raises:
        ValidationError: If query is invalid or mode is unsupported
        HTTPException 503: If Ollama service is unavailable
        HTTPException 500: If query processing fails

    Example:
        ```bash
        # Adaptive mode (automatic selection)
        curl -X POST http://localhost:8000/api/v1/rag/query \\
          -H "Authorization: Bearer $TOKEN" \\
          -H "Content-Type: application/json" \\
          -d '{
            "query": "Compare Q3 and Q4 financial results",
            "search_mode": "hybrid",
            "max_iterations": 5,
            "enable_reflection": true,
            "enable_correction": true
          }'
        ```
    """
    start_time = time.time()

    try:
        # Validate query
        if not request.query or not request.query.strip():
            raise ValidationError("Query cannot be empty")

        rag_mode = RAGMode(request.mode)

        logger.info(
            "agentic_rag_query_started",
            query=request.query[:100],
            mode=rag_mode.value,
            search_mode=request.search_mode,
        )

        # Execute agentic query
        result = await rag_engine.ask(
            question=request.query,
            mode=rag_mode,
            top_k=5,
            search_mode=request.search_mode,
            max_iterations=request.max_iterations,
            max_depth=request.max_depth,
            branch_factor=request.branch_factor,
            target_confidence=request.target_confidence,
            min_marginal_gain=request.min_marginal_gain,
            time_budget_seconds=request.time_budget_seconds,
            enable_reflection=request.enable_reflection,
            enable_correction=request.enable_correction,
        )

        # Build reasoning steps from result
        reasoning_steps: list[ReasoningStepResponse] = []
        if hasattr(result, "reasoning_steps") and result.reasoning_steps:
            for index, step in enumerate(result.reasoning_steps):
                if isinstance(step, dict):
                    reasoning_steps.append(
                        ReasoningStepResponse(
                            phase=step.get("phase", "unknown"),
                            thought=step.get("thought"),
                            action=step.get("action"),
                            action_input=step.get("action_input"),
                            observation=step.get("observation"),
                            iteration=step.get("iteration", index),
                            timestamp=step.get("timestamp", datetime.now(UTC)),
                        )
                    )
                    continue

                reasoning_steps.append(
                    ReasoningStepResponse(
                        phase="research_step",
                        thought=getattr(step, "question", None),
                        action="analyze_evidence",
                        action_input={"sources": len(getattr(step, "sources", []))},
                        observation=getattr(step, "answer", None),
                        iteration=index,
                        timestamp=datetime.now(UTC),
                    )
                )

        # Build source documents
        sources: list[SourceDocumentResponse] = []
        if hasattr(result, "sources") and result.sources:
            for source in result.sources:
                if isinstance(source, dict):
                    sources.append(
                        SourceDocumentResponse(
                            file_path=str(source.get("file_path", source.get("location", ""))),
                            filename=str(source.get("filename", source.get("title", ""))),
                            score=float(source.get("score", 0.0)),
                            snippet=(
                                str(source["snippet"]) if source.get("snippet") is not None else None
                            ),
                        )
                    )
                    continue

                sources.append(
                    SourceDocumentResponse(
                        file_path=source.file_path,
                        filename=source.filename if hasattr(source, "filename") else "",
                        score=source.score,
                        snippet=source.snippet if hasattr(source, "snippet") else None,
                    )
                )
        elif hasattr(result, "citations") and result.citations:
            # Alternative: use citations if sources not available
            for idx, citation in enumerate(result.citations, 1):
                sources.append(
                    SourceDocumentResponse(
                        file_path=citation.get("file_path", f"source_{idx}"),
                        filename=citation.get("filename", f"Document {idx}"),
                        score=citation.get("score", 0.0),
                        snippet=citation.get("snippet"),
                    )
                )

        # Calculate metrics
        total_time_ms = int((time.time() - start_time) * 1000)
        total_iterations = len(reasoning_steps)

        # Build metadata
        metadata: dict[str, Any] = {
            "model": request.model or getattr(rag_engine, "model_name", "llama3.1:8b"),
            "search_mode": request.search_mode,
            "rag_mode": rag_mode.value,
            "temperature": request.temperature,
            "max_depth": request.max_depth,
            "branch_factor": request.branch_factor,
            "target_confidence": request.target_confidence,
            "min_marginal_gain": request.min_marginal_gain,
            "time_budget_seconds": request.time_budget_seconds,
        }

        if hasattr(result, "confidence"):
            metadata["confidence"] = result.confidence
        if hasattr(result, "mode"):
            metadata["actual_mode"] = result.mode.value

        # Build response
        response = AgenticQueryResponse(
            answer=result.answer,
            reasoning_steps=reasoning_steps,
            sources=sources,
            reflection=result.reflection if hasattr(result, "reflection") else None,
            corrected=result.corrected if hasattr(result, "corrected") else False,
            total_iterations=total_iterations,
            total_time_ms=total_time_ms,
            metadata=metadata,
        )

        logger.info(
            "agentic_rag_query_completed",
            query=request.query[:100],
            answer_length=len(result.answer),
            sources_count=len(sources),
            iterations=total_iterations,
            time_ms=total_time_ms,
        )

        return response

    except OllamaUnavailableError as e:
        logger.error("ollama_unavailable", error=str(e), exc_info=True)
        raise
    except ValidationError:
        raise
    except Exception as e:
        logger.exception("agentic_rag_error", query=request.query[:100])
        raise ValidationError(f"Agentic RAG query failed: {e!s}") from e


@router.post("/modes", response_model=dict[str, Any])
@limiter.limit("20/minute")
async def list_rag_modes(
    request: Request,
    _: User = Depends(get_current_active_user),
) -> dict[str, Any]:
    """
    List available RAG modes and their characteristics.

    Returns information about each supported RAG mode including:
    - Description of the mode
    - Use cases
    - Performance characteristics
    - Recommended scenarios

    Args:
        _: Authenticated user (from JWT)

    Returns:
        dict: RAG mode information

    Example:
        ```bash
        curl -X POST http://localhost:8000/api/v1/rag/modes \\
          -H "Authorization: Bearer $TOKEN"
        ```
    """
    return {
        "modes": [
            {
                "name": "adaptive",
                "value": RAGMode.ADAPTIVE.value,
                "description": "Automatically selects the best RAG strategy based on question",
                "use_cases": [
                    "General-purpose queries",
                    "When you're unsure which mode to use",
                    "Mixed question types",
                ],
                "performance": "Variable (1-5s depending on selected mode)",
                "recommended": True,
            },
            {
                "name": "deep_research",
                "value": RAGMode.DEEP_RESEARCH.value,
                "description": "Recursive planner/critic loop with multi-hop web+local retrieval",
                "use_cases": [
                    "Open-ended research briefs",
                    "Latest trends and literature scans",
                    "Questions that require coverage, not just a quick answer",
                ],
                "performance": "~10-90s depending on depth, branch factor, and budget",
                "recommended": False,
            },
            {
                "name": "self_rag",
                "value": RAGMode.SELF_RAG.value,
                "description": "Self-reflective RAG with answer verification",
                "use_cases": [
                    "Questions requiring high accuracy",
                    "Fact-checking and verification",
                    "When citations are important",
                ],
                "performance": "~2-4s per query",
                "recommended": False,
            },
            {
                "name": "crag",
                "value": RAGMode.CRAG.value,
                "description": "Corrective RAG with web search fallback",
                "use_cases": [
                    "Questions that may require external knowledge",
                    "When local knowledge may be incomplete",
                    "Recent events or updates",
                ],
                "performance": "~1-3s (local), ~3-5s (with web fallback)",
                "recommended": False,
            },
            {
                "name": "multi_step",
                "value": RAGMode.MULTI_STEP.value,
                "description": "Multi-step reasoning for complex questions",
                "use_cases": [
                    "Complex questions with multiple parts",
                    "Comparative analysis",
                    "Questions requiring synthesis",
                ],
                "performance": "~3-8s per query",
                "recommended": False,
            },
            {
                "name": "standard",
                "value": RAGMode.STANDARD.value,
                "description": "Traditional RAG (retrieve + generate)",
                "use_cases": [
                    "Simple factual questions",
                    "When speed is critical",
                    "Baseline comparisons",
                ],
                "performance": "~1-2s per query",
                "recommended": False,
            },
        ],
        "default_mode": RAGMode.ADAPTIVE.value,
        "reflection_available": True,
        "correction_available": True,
    }


@router.get("/health", response_model=dict[str, Any])
async def rag_health_check(
    rag_engine: AgenticRAG = Depends(get_agentic_rag),
) -> dict[str, Any]:
    """
    Check health of RAG engine and dependencies.

    Verifies that:
    - Ollama service is available
    - Required models are loaded
    - Search engine is operational

    Args:
        rag_engine: Agentic RAG engine instance (injected)

    Returns:
        dict: Health status information

    Example:
        ```bash
        curl -X GET http://localhost:8000/api/v1/rag/health
        ```
    """
    try:
        ollama_healthy = await rag_engine.health_check()

        return {
            "status": "healthy" if ollama_healthy else "degraded",
            "ollama_available": ollama_healthy,
            "ollama_url": "http://localhost:11434",
            "model": "llama3.1:8b",
            "modes_available": [mode.value for mode in RAGMode],
            "web_search_enabled": True,
        }

    except Exception as e:
        logger.exception("rag_health_check_error")
        return {
            "status": "unhealthy",
            "error": str(e),
            "ollama_available": False,
        }


@router.post("/deep-research/stream")
@limiter.limit("3/minute")
async def stream_deep_research(
    http_request: Request,
    request: AgenticQueryRequest = Body(...),
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    rag_engine: AgenticRAG = Depends(get_agentic_rag),
) -> StreamingResponse:
    """
    Stream deep research execution as Server-Sent Events.

    Emits event frames in this shape:
    - status/thought/action/observation/answer/complete/error
    """
    try:
        if not request.query or not request.query.strip():
            raise ValidationError("Query cannot be empty")

        async def stream_generator():
            try:
                async for event in rag_engine.deep_research.ask_deep_research_stream(
                    question=request.query,
                    search_mode=request.search_mode,
                    max_depth=request.max_depth,
                    max_iterations=request.max_iterations,
                    branch_factor=request.branch_factor,
                    top_k=5,
                    target_confidence=request.target_confidence,
                    min_marginal_gain=request.min_marginal_gain,
                    time_budget_seconds=request.time_budget_seconds,
                ):
                    event_type = str(event.get("type", "status"))
                    payload = json.dumps(event, default=str)
                    yield f"event: {event_type}\ndata: {payload}\n\n"

            except Exception as e:
                logger.exception("deep_research_stream_error", query=request.query[:100])
                payload = json.dumps({"type": "error", "content": str(e)}, default=str)
                yield f"event: error\ndata: {payload}\n\n"

        logger.info("deep_research_stream_started", query=request.query[:100])
        return StreamingResponse(
            stream_generator(),
            media_type="text/event-stream",
            headers={
                "Cache-Control": "no-cache",
                "Connection": "keep-alive",
                "X-Accel-Buffering": "no",
            },
        )
    except ValidationError:
        raise
    except Exception as e:
        logger.exception("deep_research_stream_setup_error", query=request.query[:100])
        raise ValidationError(f"Deep research stream setup failed: {e!s}") from e


__all__ = ["router"]
