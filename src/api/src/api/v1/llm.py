"""LLM Q&A API endpoints."""

from __future__ import annotations

from collections.abc import AsyncIterator
import logging

from fastapi import APIRouter, Depends, HTTPException, status
from fastapi.responses import StreamingResponse
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.dependencies import get_db
from src.auth.dependencies import get_current_active_user
from src.auth.models import User
from src.middleware.csrf import csrf_protect
from src.schemas.llm import (
    AskRequest,
    AskResponse,
    ErrorResponse,
    FunctionCallRequest,
    FunctionCallResponse,
    HealthCheckResponse,
    ListModelsResponse,
)
from src.services.llm import OllamaAPIError, OllamaConnectionError, OllamaTimeoutError
from src.services.llm.service import LLMService

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/llm", tags=["llm"])


async def get_llm_service(session: AsyncSession = Depends(get_db)) -> AsyncIterator[LLMService]:
    """
    Get LLM service instance with proper resource cleanup.

    Args:
        session: Database session for search service

    Yields:
        Initialized LLM service
    """
    # Import here to avoid circular imports
    from src.config import get_settings
    from src.modules.embedding_generator import EmbeddingService
    from src.modules.search_engine import SearchService

    settings = get_settings()

    # Create service with current session
    embedding_service = EmbeddingService()
    search_service = SearchService(session, embedding_service)
    llm_service = LLMService(
        search_service=search_service,
        ollama_url=getattr(settings, "ollama_url", "http://localhost:11434"),
        model=getattr(settings, "ollama_default_model", "llama2"),
    )
    await llm_service.initialize()
    logger.info("LLM service initialized")

    try:
        yield llm_service
    finally:
        await llm_service.cleanup()
        logger.info("LLM service cleaned up")


@router.post(
    "/ask",
    response_class=StreamingResponse,
    summary="Ask a question with streaming response",
    description="Ask a question about your documents and get a streaming LLM response using RAG",
)
async def ask_question_stream(
    request: AskRequest,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    llm_service: LLMService = Depends(get_llm_service),
):
    """
    Ask a question and get streaming LLM response with RAG.

    This endpoint:
    1. Searches your documents for relevant context
    2. Builds a prompt with the context
    3. Streams the LLM response in real-time

    The response is streamed as plain text chunks.
    """
    logger.info(f"Received question: '{request.question[:100]}...'")

    async def generate():
        """Generator for streaming response."""
        try:
            async for chunk in llm_service.ask_question(
                question=request.question,
                max_context_docs=request.max_context_docs,
                search_mode=request.search_mode,
                model=request.model,
                temperature=request.temperature,
                max_tokens=request.max_tokens,
            ):
                yield chunk
        except OllamaConnectionError as e:
            logger.error(f"Ollama connection error: {e}")
            yield "\n\nError: Cannot connect to Ollama server. Please ensure Ollama is running.\n"
        except OllamaTimeoutError as e:
            logger.error(f"Ollama timeout: {e}")
            yield "\n\nError: Request timed out. Please try again.\n"
        except OllamaAPIError as e:
            logger.error(f"Ollama API error: {e}")
            yield f"\n\nError: LLM generation failed. {e!s}\n"
        except Exception as e:
            logger.exception(f"Unexpected error in streaming: {e}")
            yield f"\n\nError: {e!s}\n"

    return StreamingResponse(
        generate(),
        media_type="text/plain",
        headers={
            "Cache-Control": "no-cache",
            "X-Accel-Buffering": "no",
        },
    )


@router.post(
    "/ask-with-sources",
    response_model=AskResponse,
    summary="Ask a question with sources",
    description="Ask a question and get a complete response with source documents",
    responses={
        200: {"description": "Successful response with answer and sources"},
        503: {"description": "Ollama service unavailable", "model": ErrorResponse},
        500: {"description": "Internal server error", "model": ErrorResponse},
    },
)
async def ask_question_with_sources(
    request: AskRequest,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    llm_service: LLMService = Depends(get_llm_service),
) -> AskResponse:
    """
    Ask a question and get response with source documents.

    This endpoint returns a complete response including:
    - The generated answer
    - List of source documents used
    - Metadata about the query

    Use this for non-streaming requests where you need structured output.
    """
    logger.info(f"Received question with sources: '{request.question[:100]}...'")

    try:
        result = await llm_service.ask_question_with_sources(
            question=request.question,
            max_context_docs=request.max_context_docs,
            search_mode=request.search_mode,
            model=request.model,
            temperature=request.temperature,
            max_tokens=request.max_tokens,
        )

        return AskResponse(**result)

    except OllamaConnectionError as e:
        logger.error(f"Ollama connection error: {e}")
        raise HTTPException(
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
            detail="Cannot connect to Ollama server. Please ensure Ollama is running.",
        )
    except OllamaTimeoutError as e:
        logger.error(f"Ollama timeout: {e}")
        raise HTTPException(
            status_code=status.HTTP_504_GATEWAY_TIMEOUT,
            detail="Request timed out. Please try again.",
        )
    except OllamaAPIError as e:
        logger.error(f"Ollama API error: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"LLM generation failed: {e!s}",
        )
    except Exception as e:
        logger.exception(f"Unexpected error: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR, detail=f"Unexpected error: {e!s}"
        )


@router.get(
    "/health",
    response_model=HealthCheckResponse,
    summary="Check LLM service health",
    description="Check if Ollama service is available and responding",
)
async def health_check(
    current_user: User = Depends(get_current_active_user),
    llm_service: LLMService = Depends(get_llm_service),
) -> HealthCheckResponse:
    """
    Check if Ollama is available and healthy.

    Returns:
        Health status including Ollama availability and current model
    """
    try:
        is_healthy = await llm_service.health_check()

        return HealthCheckResponse(
            status="healthy" if is_healthy else "unhealthy",
            ollama_available=is_healthy,
            model=llm_service.model,
        )
    except Exception as e:
        logger.error(f"Health check error: {e}")
        return HealthCheckResponse(
            status="unhealthy",
            ollama_available=False,
            model=llm_service.model,
        )


@router.get(
    "/models",
    response_model=ListModelsResponse,
    summary="List available models",
    description="Get list of models available in Ollama",
    responses={
        200: {"description": "List of available models"},
        503: {"description": "Ollama service unavailable", "model": ErrorResponse},
    },
)
async def list_models(
    current_user: User = Depends(get_current_active_user),
    llm_service: LLMService = Depends(get_llm_service),
) -> ListModelsResponse:
    """
    List all models available in Ollama.

    Returns:
        List of model names and the default model being used
    """
    try:
        models = await llm_service.list_available_models()

        return ListModelsResponse(
            models=models,
            default_model=llm_service.model,
        )
    except OllamaConnectionError as e:
        logger.error(f"Ollama connection error: {e}")
        raise HTTPException(
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
            detail="Cannot connect to Ollama server. Please ensure Ollama is running.",
        )
    except Exception as e:
        logger.exception(f"Error listing models: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to list models: {e!s}",
        )


@router.get(
    "/cache/metrics",
    summary="Get prompt cache metrics",
    description="Get cache performance statistics including hits, misses, and time saved",
)
async def get_cache_metrics(
    current_user: User = Depends(get_current_active_user),
    llm_service: LLMService = Depends(get_llm_service),
):
    """
    Get prompt cache performance metrics.

    Returns cache statistics including:
    - Number of cache hits and misses
    - Hit rate percentage
    - Estimated time saved from caching (ms)
    - Cache enabled status
    """
    try:
        metrics = llm_service.get_cache_metrics()
        return metrics
    except Exception as e:
        logger.exception(f"Error getting cache metrics: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to get cache metrics: {e!s}",
        )


@router.post(
    "/cache/invalidate",
    summary="Invalidate prompt cache",
    description="Clear the prompt cache (e.g., when document corpus changes)",
)
async def invalidate_cache(
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    llm_service: LLMService = Depends(get_llm_service),
):
    """
    Invalidate the prompt cache.

    Call this endpoint when:
    - Documents are added, updated, or deleted
    - Context needs to be refreshed for any reason

    This ensures subsequent queries use fresh context.
    """
    try:
        llm_service.invalidate_cache()
        return {"status": "success", "message": "Cache invalidated"}
    except Exception as e:
        logger.exception(f"Error invalidating cache: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to invalidate cache: {e!s}",
        )


# =============================================================================
# Function Calling Endpoints
# =============================================================================


@router.post(
    "/function-call",
    response_model=FunctionCallResponse,
    summary="Execute a function call",
    description="Execute a single function call (for LLM tool use)",
    responses={
        200: {"description": "Function executed successfully"},
        400: {"description": "Invalid function or arguments", "model": ErrorResponse},
        429: {"description": "Rate limit exceeded", "model": ErrorResponse},
        500: {"description": "Function execution failed", "model": ErrorResponse},
    },
)
async def execute_function_call(
    request: FunctionCallRequest,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    session: AsyncSession = Depends(get_db),
) -> FunctionCallResponse:
    """
    Execute a function call with security controls.

    This endpoint:
    1. Validates the function exists
    2. Checks rate limits
    3. Validates input arguments
    4. Executes the function
    5. Returns the result

    Supports all Phase 1 and Phase 2 functions:
    - semantic_search: Search vault documents
    - get_document: Get full document content
    - list_documents: Browse documents
    - web_search: Search the web (DuckDuckGo)
    - fetch_url_content: Fetch URL content
    """
    from datetime import datetime

    from src.services.llm.function_calling import FunctionExecutor, get_default_registry
    from src.services.llm.function_calling.executor import FunctionCallError, RateLimitError
    from src.services.llm.function_calling.tools import create_all_tools
    from src.services.llm.function_calling.web_service import WebService

    start_time = datetime.now()

    try:
        # Initialize web service
        web_service = WebService(cache_enabled=True)
        await web_service.initialize()

        try:
            # Create registry with all tools
            registry = get_default_registry()

            # Register tools with current session and web service
            registry = type(registry)()  # Create fresh registry
            for tool in create_all_tools(db_session=session, web_service=web_service):
                registry.register(tool)

            # Create executor
            executor = FunctionExecutor(registry)

            # Execute function
            result = await executor.execute(
                function_name=request.function_name,
                arguments=request.arguments,
                user_id=request.user_id or str(current_user.id),
            )

            execution_time = (datetime.now() - start_time).total_seconds() * 1000

            return FunctionCallResponse(
                success=True,
                result=result,
                error=None,
                execution_time_ms=execution_time,
            )

        finally:
            await web_service.cleanup()

    except RateLimitError as e:
        logger.warning(f"Rate limit exceeded: {e}")
        raise HTTPException(
            status_code=status.HTTP_429_TOO_MANY_REQUESTS,
            detail=str(e),
        )

    except FunctionCallError as e:
        logger.error(f"Function call error: {e}")
        return FunctionCallResponse(
            success=False,
            result=None,
            error={
                "error_code": e.error_code,
                "message": e.message,
                "details": str(e.details) if e.details else None,
            },
            execution_time_ms=(datetime.now() - start_time).total_seconds() * 1000,
        )

    except Exception as e:
        logger.exception(f"Unexpected error executing function: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Function execution failed: {e!s}",
        )


@router.get(
    "/functions",
    summary="List available functions",
    description="Get list of all available function calling tools",
)
async def list_available_functions(
    current_user: User = Depends(get_current_active_user),
):
    """
    List all available function calling tools.

    Returns:
        List of function definitions with names, descriptions, and schemas
    """
    from src.services.llm.function_calling import get_default_registry

    try:
        registry = get_default_registry()

        # Convert to dict format
        functions = []
        for tool in registry.list_tools():
            functions.append(
                {
                    "name": tool.name,
                    "description": tool.description,
                    "rate_limit": tool.rate_limit,
                    "input_schema": tool.input_schema.model_json_schema(),
                    "output_schema": tool.output_schema.model_json_schema(),
                    "metadata": tool.metadata,
                }
            )

        return {
            "functions": functions,
            "total": len(functions),
        }

    except Exception as e:
        logger.exception(f"Error listing functions: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to list functions: {e!s}",
        )
