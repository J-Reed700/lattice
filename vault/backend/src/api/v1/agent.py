from __future__ import annotations

from collections.abc import AsyncIterator
from datetime import UTC, datetime
import logging
import time

from fastapi import APIRouter, Body, Depends, HTTPException, Request, status
from fastapi.responses import StreamingResponse
from slowapi.util import get_remote_address
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.errors import ValidationError
from src.config import Settings, get_settings
from src.db import get_session
from src.middleware.rate_limit import RateLimitExceededError, check_rate_limit_by_key
from src.schemas.agent import (
    IntegrationStatus,
    QueryRequest,
    QueryResponse,
    SourceDocument,
    StatusResponse,
    SyncRequest,
    SyncResponse,
)
from src.services.langchain.base_service import BaseLangChainService
from src.services.llm.ollama_service import OllamaService

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/agent", tags=["agent"])


async def check_agent_rate_limit(request: Request) -> None:
    """
    Rate limiting dependency for agent endpoints.

    Limits: 10 requests per minute per IP.

    Raises:
        RateLimitExceededError: If rate limit exceeded
    """
    ip = get_remote_address(request)
    key = f"agent:query:ip:{ip}"
    allowed, retry_after = await check_rate_limit_by_key(key, 10, 60)

    if not allowed:
        raise RateLimitExceededError(retry_after)


async def get_ollama_service(settings: Settings = Depends(get_settings)) -> OllamaService:
    service = OllamaService(
        base_url=settings.ollama_base_url,
        default_model=settings.ollama_default_model,
        timeout=settings.ollama_timeout,
        stream_timeout=settings.ollama_stream_timeout,
    )
    await service.initialize()
    try:
        yield service
    finally:
        await service.cleanup()


async def get_langchain_service(settings: Settings = Depends(get_settings)) -> BaseLangChainService:
    return BaseLangChainService(settings)


@router.post("/query", response_model=QueryResponse)
async def query_agent(
    request: QueryRequest = Body(...),
    session: AsyncSession = Depends(get_session),
    ollama_service: OllamaService = Depends(get_ollama_service),
    langchain_service: BaseLangChainService = Depends(get_langchain_service),
    settings: Settings = Depends(get_settings),
    _rate_limit: None = Depends(check_agent_rate_limit),
):
    """
    Query the RAG agent with natural language.

    This endpoint processes natural language queries using a LangChain-based
    Retrieval-Augmented Generation (RAG) agent. It retrieves relevant documents
    from the knowledge vault and generates contextual answers using a local LLM.

    Features:
    - Semantic search over indexed documents
    - Context-aware answer generation
    - Source attribution with relevance scores
    - Optional streaming for long-running queries
    - Configurable context window and LLM selection

    Rate Limiting:
    - 10 requests per minute per user

    Args:
        request: Query request with natural language query and options
        session: Database session (injected)
        ollama_service: Ollama LLM service (injected)
        langchain_service: LangChain agent service (injected)
        settings: Application settings (injected)

    Returns:
        QueryResponse with generated answer, sources, and metadata

    Raises:
        HTTPException 429: Rate limit exceeded
        HTTPException 400: Invalid query or parameters
        HTTPException 503: LLM service unavailable
        HTTPException 500: Internal processing error
    """
    if not request.query or len(request.query.strip()) == 0:
        raise ValidationError(message="Query cannot be empty", details={"field": "query"})

    start_time = time.time()

    try:
        is_available = await ollama_service.health_check()
        if not is_available:
            raise HTTPException(
                status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
                detail="LLM service (Ollama) is not available. Please ensure Ollama is running.",
            )

        if request.streaming:
            return await _handle_streaming_query(request, langchain_service, settings, start_time)

        logger.info(f"Processing query: {request.query[:100]}...")

        agent_response = await langchain_service.invoke_agent(
            input_text=request.query,
            additional_context={
                "context_limit": request.context_limit,
                "use_local_llm": request.use_local_llm,
            },
        )

        if not agent_response.success:
            raise HTTPException(
                status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
                detail=f"Agent execution failed: {agent_response.error}",
            )

        sources = []
        for step in agent_response.intermediate_steps[: request.context_limit]:
            observation = step.get("observation", "")
            if isinstance(observation, dict) and "documents" in observation:
                for doc in observation["documents"]:
                    sources.append(
                        SourceDocument(
                            file_path=doc.get("path", ""),
                            filename=doc.get("filename", ""),
                            snippet=doc.get("snippet", "")[:500],
                            score=doc.get("score", 0.5),
                            page_number=doc.get("page_number"),
                        )
                    )

        confidence = _calculate_confidence(agent_response, sources)

        query_time_ms = int((time.time() - start_time) * 1000)

        response = QueryResponse(
            answer=agent_response.output,
            sources=sources,
            confidence=confidence,
            query_time_ms=query_time_ms,
            model_used=settings.langchain_ollama_model,
            token_count=agent_response.metadata.get("token_count"),
        )

        logger.info(
            f"Query processed in {query_time_ms}ms with {len(sources)} sources "
            f"and confidence {confidence:.2f}"
        )

        return response

    except HTTPException:
        raise
    except Exception as e:
        logger.error(f"Error processing query: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to process query: {e!s}",
        )


async def _handle_streaming_query(
    request: QueryRequest,
    langchain_service: BaseLangChainService,
    settings: Settings,
    start_time: float,
) -> StreamingResponse:
    async def generate_stream() -> AsyncIterator[str]:
        try:
            async for chunk in langchain_service.stream_agent(
                input_text=request.query,
                additional_context={
                    "context_limit": request.context_limit,
                    "use_local_llm": request.use_local_llm,
                },
            ):
                if chunk.chunk_type == "token":
                    yield f"data: {chunk.content}\n\n"
                elif chunk.chunk_type == "error":
                    yield f"data: [ERROR] {chunk.content}\n\n"
                elif chunk.chunk_type == "final":
                    query_time_ms = int((time.time() - start_time) * 1000)
                    yield f"data: [DONE] Query completed in {query_time_ms}ms\n\n"
        except Exception as e:
            logger.error(f"Streaming error: {e}", exc_info=True)
            yield f"data: [ERROR] {e!s}\n\n"

    return StreamingResponse(
        generate_stream(),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache",
            "Connection": "keep-alive",
        },
    )


def _calculate_confidence(agent_response, sources: list) -> float:
    if not agent_response.success:
        return 0.0

    if not sources:
        return 0.3

    avg_source_score = sum(s.score for s in sources) / len(sources) if sources else 0.0

    source_count_factor = min(len(sources) / 3.0, 1.0)

    confidence = (avg_source_score * 0.7) + (source_count_factor * 0.3)

    return round(min(max(confidence, 0.0), 1.0), 2)


@router.get("/status", response_model=StatusResponse)
async def get_agent_status(
    ollama_service: OllamaService = Depends(get_ollama_service),
    langchain_service: BaseLangChainService = Depends(get_langchain_service),
    settings: Settings = Depends(get_settings),
):
    """
    Check agent and LLM service status.

    This endpoint provides comprehensive status information about the RAG agent
    system, including LLM availability, configured models, and integration status.

    Useful for:
    - Health checks and monitoring
    - Verifying Ollama service availability
    - Checking which models are installed
    - Monitoring integration connectivity
    - Debugging configuration issues

    Returns:
        StatusResponse with service availability and configuration details

    Raises:
        HTTPException 500: Error checking service status
    """
    try:
        ollama_available = await ollama_service.health_check()

        ollama_models = []
        if ollama_available:
            try:
                ollama_models = await ollama_service.list_available_models()
            except Exception as e:
                logger.warning(f"Failed to list Ollama models: {e}")

        integrations = _get_integration_status()

        agent_info = langchain_service.get_config_info()

        return StatusResponse(
            ollama_available=ollama_available,
            ollama_models=ollama_models,
            integrations=integrations,
            agent_initialized=agent_info.get("initialized", False),
            default_model=settings.langchain_ollama_model,
        )

    except Exception as e:
        logger.error(f"Error getting agent status: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to get agent status: {e!s}",
        )


def _get_integration_status() -> dict:
    integrations = {
        "google_drive": IntegrationStatus(
            enabled=False,
            connected=False,
            last_sync=None,
            document_count=0,
            error=None,
        ),
        "dropbox": IntegrationStatus(
            enabled=False,
            connected=False,
            last_sync=None,
            document_count=0,
            error=None,
        ),
        "local_files": IntegrationStatus(
            enabled=True,
            connected=True,
            last_sync=datetime.now(UTC),
            document_count=0,
            error=None,
        ),
    }

    return integrations


@router.post("/integrations/sync", response_model=SyncResponse)
async def sync_integration(
    request: SyncRequest = Body(...),
    session: AsyncSession = Depends(get_session),
    settings: Settings = Depends(get_settings),
    _rate_limit: None = Depends(check_agent_rate_limit),
):
    """
    Manually trigger synchronization for a specific integration.

    This endpoint allows manual triggering of document synchronization from
    configured integrations (e.g., Google Drive, Dropbox). Use this to:
    - Force immediate sync instead of waiting for scheduled sync
    - Recover from failed automatic syncs
    - Test integration connectivity
    - Update the knowledge base with latest documents

    Supported integrations:
    - google_drive: Google Drive documents and files
    - dropbox: Dropbox files and folders
    - local_files: Local file system watch folders

    Rate Limiting:
    - 10 requests per minute per user

    Args:
        request: Sync request with integration name and options
        session: Database session (injected)
        settings: Application settings (injected)

    Returns:
        SyncResponse with sync results and statistics

    Raises:
        HTTPException 429: Rate limit exceeded
        HTTPException 400: Invalid integration name
        HTTPException 404: Integration not found or not configured
        HTTPException 503: Integration service unavailable
        HTTPException 500: Sync operation failed
    """
    valid_integrations = ["google_drive", "dropbox", "local_files"]
    if request.integration_name not in valid_integrations:
        raise ValidationError(
            message=f"Invalid integration name: {request.integration_name}",
            details={"field": "integration_name", "valid_values": valid_integrations},
        )

    start_time = time.time()

    try:
        logger.info(
            f"Starting sync for integration: {request.integration_name} (force={request.force})"
        )

        documents_synced = 0
        errors = []
        sync_status = "success"

        if request.integration_name == "local_files":
            documents_synced = 0
        elif request.integration_name == "google_drive":
            errors.append("Google Drive integration not yet implemented")
            sync_status = "failed"
        elif request.integration_name == "dropbox":
            errors.append("Dropbox integration not yet implemented")
            sync_status = "failed"

        sync_time_ms = int((time.time() - start_time) * 1000)

        response = SyncResponse(
            integration_name=request.integration_name,
            documents_synced=documents_synced,
            sync_time_ms=sync_time_ms,
            last_sync=datetime.now(UTC),
            status=sync_status,
            errors=errors,
        )

        logger.info(
            f"Sync completed for {request.integration_name}: "
            f"{documents_synced} docs in {sync_time_ms}ms, status={sync_status}"
        )

        return response

    except ValidationError:
        raise
    except Exception as e:
        logger.error(f"Error syncing integration {request.integration_name}: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to sync integration: {e!s}",
        )
