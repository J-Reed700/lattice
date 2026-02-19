"""
API endpoints for document summarization.

Privacy-preserving summarization using on-device SLMs.
"""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
import hashlib
from uuid import UUID

from fastapi import APIRouter, BackgroundTasks, Depends, HTTPException, status
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.dependencies import get_db
from src.auth.dependencies import get_current_active_user
from src.auth.models import User
from src.middleware.csrf import csrf_protect
from src.models.file import File
from src.models.summary import Summary as SummaryModel
from src.modules.summarizer import (
    BatchSummaryRequest,
    LanguageNotSupportedError,
    ModelNotFoundError,
    SummarizationError,
    SummarizerConfig,
    SummarizerService,
    SummaryRequest,
    SummaryType,
)

router = APIRouter(prefix="/summarize", tags=["summarization"])

# Global service instance (initialized on startup)
_summarizer_service: SummarizerService | None = None


def get_summarizer_service() -> SummarizerService:
    """Get or create summarizer service instance.

    Returns:
        SummarizerService instance

    Raises:
        HTTPException: If service not initialized
    """
    global _summarizer_service
    if _summarizer_service is None:
        config = SummarizerConfig()
        _summarizer_service = SummarizerService(config)
    return _summarizer_service


async def initialize_summarizer():
    """Initialize summarizer service on app startup."""
    service = get_summarizer_service()
    await service.initialize()


async def cleanup_summarizer():
    """Cleanup summarizer service on app shutdown."""
    global _summarizer_service
    if _summarizer_service:
        await _summarizer_service.cleanup()
        _summarizer_service = None


def _compute_text_hash(text: str) -> str:
    """Compute SHA-256 hash of text for caching.

    Args:
        text: Input text

    Returns:
        Hex digest of hash
    """
    return hashlib.sha256(text.encode()).hexdigest()


async def _get_cached_summary(
    db: AsyncSession, text_hash: str, summary_type: str, model: str
) -> SummaryModel | None:
    """Check if summary exists in cache.

    Args:
        db: Database session
        text_hash: Hash of source text
        summary_type: Type of summary
        model: Model used

    Returns:
        Cached summary or None
    """
    result = await db.execute(
        select(SummaryModel)
        .where(
            SummaryModel.source_text_hash == text_hash,
            SummaryModel.summary_type == summary_type,
            SummaryModel.model == model,
        )
        .where((SummaryModel.expires_at == None) | (SummaryModel.expires_at > datetime.now(UTC)))
    )
    return result.scalar_one_or_none()


async def _save_summary_to_cache(
    db: AsyncSession,
    summary: dict,
    text_hash: str,
    file_id: UUID | None = None,
    ttl_hours: int = 24,
) -> SummaryModel:
    """Save summary to database cache.

    Args:
        db: Database session
        summary: Summary data
        text_hash: Hash of source text
        file_id: Optional file ID
        ttl_hours: Cache TTL in hours

    Returns:
        Created SummaryModel
    """
    expires_at = datetime.now(UTC) + timedelta(hours=ttl_hours) if ttl_hours > 0 else None

    db_summary = SummaryModel(
        file_id=file_id,
        text=summary["text"],
        summary_type=summary["summary_type"],
        source_text_hash=text_hash,
        source_length=summary["source_length"],
        summary_length=summary["summary_length"],
        compression_ratio=summary["compression_ratio"],
        model=summary["model"],
        language=summary["language"],
        generation_time=summary["generation_time"],
        tokens_per_second=summary["tokens_per_second"],
        expires_at=expires_at,
        summary_metadata=summary.get("metadata", {}),
    )

    db.add(db_summary)
    await db.commit()
    await db.refresh(db_summary)

    return db_summary


@router.post("/text", response_model=dict, status_code=status.HTTP_200_OK)
async def summarize_text(
    request: SummaryRequest,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
    service: SummarizerService = Depends(get_summarizer_service),
):
    """Summarize arbitrary text.

    This endpoint generates a summary for any input text without linking
    it to a specific file in the database.

    Args:
        request: Summary request parameters
        db: Database session
        service: Summarizer service

    Returns:
        Summary object with generated text and metadata

    Raises:
        400: Invalid input
        404: Model not found
        500: Summarization failed

    Example:
        POST /api/v1/summarize/text
        {
            "text": "Long document text...",
            "summary_type": "bullet_points",
            "max_words": 100,
            "model": "phi-3.5-mini"
        }
    """
    try:
        # Check cache if enabled
        text_hash = _compute_text_hash(request.text)
        model_name = request.model or service.config.default_model

        if service.config.enable_caching:
            cached = await _get_cached_summary(
                db, text_hash, request.summary_type.value, model_name
            )
            if cached:
                result = cached.to_dict()
                result["cache_hit"] = True
                return result

        # Generate summary
        summary = await service.summarize(
            text=request.text,
            summary_type=request.summary_type,
            max_words=request.max_words,
            model=request.model,
            language=request.language,
            stream=False,
        )

        # Save to cache if enabled
        if service.config.enable_caching:
            await _save_summary_to_cache(
                db, summary.dict(), text_hash, ttl_hours=service.config.cache_ttl // 3600
            )

        return summary.dict()

    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except ModelNotFoundError as e:
        raise HTTPException(status_code=404, detail=str(e))
    except (SummarizationError, LanguageNotSupportedError) as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.post("/document/{file_id}", response_model=dict, status_code=status.HTTP_200_OK)
async def summarize_document(
    file_id: UUID,
    summary_type: SummaryType = SummaryType.ABSTRACTIVE,
    max_words: int = 150,
    model: str | None = None,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
    service: SummarizerService = Depends(get_summarizer_service),
):
    """Summarize an indexed document.

    This endpoint generates a summary for a file that has been indexed
    in the database. It uses the extracted text content.

    Args:
        file_id: UUID of the file to summarize
        summary_type: Type of summary to generate
        max_words: Target word count
        model: Model to use (defaults to config)
        db: Database session
        service: Summarizer service

    Returns:
        Summary object with generated text and metadata

    Raises:
        404: File not found or no text content
        500: Summarization failed

    Example:
        POST /api/v1/summarize/document/123e4567-e89b-12d3-a456-426614174000
        ?summary_type=tldr&max_words=50
    """
    # Get file with text content
    result = await db.execute(select(File).where(File.id == file_id))
    file = result.scalar_one_or_none()

    if not file:
        raise HTTPException(status_code=404, detail="File not found")

    if not file.text_content or not file.text_content.content:
        raise HTTPException(status_code=404, detail="No text content for this file")

    text = file.text_content.content

    try:
        # Check cache
        text_hash = _compute_text_hash(text)
        model_name = model or service.config.default_model

        if service.config.enable_caching:
            cached = await _get_cached_summary(db, text_hash, summary_type.value, model_name)
            if cached and cached.file_id == file_id:
                result = cached.to_dict()
                result["cache_hit"] = True
                return result

        # Generate summary
        summary = await service.summarize(
            text=text, summary_type=summary_type, max_words=max_words, model=model, stream=False
        )

        # Save to cache with file link
        if service.config.enable_caching:
            await _save_summary_to_cache(
                db,
                summary.dict(),
                text_hash,
                file_id=file_id,
                ttl_hours=service.config.cache_ttl // 3600,
            )

        return summary.dict()

    except (SummarizationError, LanguageNotSupportedError) as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.post("/batch", response_model=dict, status_code=status.HTTP_200_OK)
async def summarize_batch(
    request: BatchSummaryRequest,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
    service: SummarizerService = Depends(get_summarizer_service),
):
    """Summarize multiple texts in batch.

    Args:
        request: Batch summary request
        db: Database session
        service: Summarizer service

    Returns:
        Batch response with all summaries and stats

    Example:
        POST /api/v1/summarize/batch
        {
            "texts": ["Doc 1...", "Doc 2..."],
            "summary_type": "tldr",
            "max_words": 50,
            "parallel": true
        }
    """
    try:
        response = await service.batch_summarize(
            texts=request.texts,
            summary_type=request.summary_type,
            max_words=request.max_words,
            model=request.model,
            parallel=request.parallel,
        )

        return response.dict()

    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/models", response_model=list[dict], status_code=status.HTTP_200_OK)
async def list_models(
    current_user: User = Depends(get_current_active_user),
    service: SummarizerService = Depends(get_summarizer_service),
):
    """List all available models.

    Returns:
        List of ModelInfo objects with download status

    Example:
        GET /api/v1/summarize/models
    """
    models = service.list_models()
    return [model.dict() for model in models]


@router.get("/models/downloaded", response_model=list[dict], status_code=status.HTTP_200_OK)
async def list_downloaded_models(
    current_user: User = Depends(get_current_active_user),
    service: SummarizerService = Depends(get_summarizer_service),
):
    """List only downloaded models.

    Returns:
        List of downloaded ModelInfo objects

    Example:
        GET /api/v1/summarize/models/downloaded
    """
    models = service.list_downloaded_models()
    return [model.dict() for model in models]


@router.post("/models/download/{model_name}", response_model=dict, status_code=status.HTTP_200_OK)
async def download_model(
    model_name: str,
    background_tasks: BackgroundTasks,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    service: SummarizerService = Depends(get_summarizer_service),
):
    """Download a model.

    This endpoint initiates model download in the background.
    Use the progress endpoint to check status.

    Args:
        model_name: Model to download
        background_tasks: FastAPI background tasks
        service: Summarizer service

    Returns:
        Download initiation confirmation

    Example:
        POST /api/v1/summarize/models/download/phi-3.5-mini
    """
    try:
        # Check if already downloaded
        if service.model_manager.is_model_downloaded(model_name):
            return {"status": "already_downloaded", "model_name": model_name}

        # Start download in background
        background_tasks.add_task(service.download_model, model_name)

        return {"status": "download_started", "model_name": model_name}

    except ModelNotFoundError as e:
        raise HTTPException(status_code=404, detail=str(e))


@router.get("/{summary_id}", response_model=dict, status_code=status.HTTP_200_OK)
async def get_summary(
    summary_id: UUID,
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
):
    """Get a cached summary by ID.

    Args:
        summary_id: UUID of the summary
        db: Database session

    Returns:
        Summary object

    Raises:
        404: Summary not found

    Example:
        GET /api/v1/summarize/550e8400-e29b-41d4-a716-446655440000
    """
    result = await db.execute(select(SummaryModel).where(SummaryModel.id == summary_id))
    summary = result.scalar_one_or_none()

    if not summary:
        raise HTTPException(status_code=404, detail="Summary not found")

    return summary.to_dict()


@router.delete("/{summary_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_summary(
    summary_id: UUID,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
):
    """Delete a cached summary.

    Args:
        summary_id: UUID of the summary to delete
        db: Database session

    Raises:
        404: Summary not found

    Example:
        DELETE /api/v1/summarize/550e8400-e29b-41d4-a716-446655440000
    """
    result = await db.execute(select(SummaryModel).where(SummaryModel.id == summary_id))
    summary = result.scalar_one_or_none()

    if not summary:
        raise HTTPException(status_code=404, detail="Summary not found")

    await db.delete(summary)
    await db.commit()


@router.get(
    "/document/{file_id}/summaries", response_model=list[dict], status_code=status.HTTP_200_OK
)
async def get_document_summaries(
    file_id: UUID,
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
):
    """Get all cached summaries for a document.

    Args:
        file_id: UUID of the file
        db: Database session

    Returns:
        List of summaries for the file

    Example:
        GET /api/v1/summarize/document/123e4567-e89b-12d3-a456-426614174000/summaries
    """
    result = await db.execute(
        select(SummaryModel)
        .where(SummaryModel.file_id == file_id)
        .order_by(SummaryModel.created_at.desc())
    )
    summaries = result.scalars().all()

    return [summary.to_dict() for summary in summaries]
