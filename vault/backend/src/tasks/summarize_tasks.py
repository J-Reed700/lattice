"""
Celery tasks for background document summarization.

These tasks handle batch summarization and auto-summarization
of newly indexed documents.
"""

import asyncio
from datetime import UTC, datetime
from uuid import UUID

from celery import shared_task
from sqlalchemy import select

from ..db.session import get_db_context
from ..models.file import File
from ..models.summary import Summary as SummaryModel
from ..modules.summarizer import SummarizerConfig, SummarizerService, SummaryType

# Global service instance
_summarizer_service: SummarizerService | None = None


def get_summarizer_service() -> SummarizerService:
    """Get or create summarizer service for tasks.

    Returns:
        SummarizerService instance
    """
    global _summarizer_service
    if _summarizer_service is None:
        config = SummarizerConfig()
        _summarizer_service = SummarizerService(config)
    return _summarizer_service


@shared_task(name="summarize_document_task", bind=True, max_retries=3, default_retry_delay=60)
def summarize_document_task(
    self,
    file_id: str,
    summary_type: str = "abstractive",
    max_words: int = 150,
    model: str | None = None,
) -> dict:
    """Background task to summarize a document.

    Args:
        self: Celery task instance
        file_id: UUID of the file to summarize
        summary_type: Type of summary
        max_words: Target word count
        model: Model to use

    Returns:
        Summary result dictionary

    Example:
        >>> from tasks.summarize_tasks import summarize_document_task
        >>> task = summarize_document_task.delay(
        ...     file_id="123e4567-e89b-12d3-a456-426614174000",
        ...     summary_type="tldr",
        ...     max_words=50
        ... )
    """
    try:
        # Run async summarization
        result = asyncio.run(
            _async_summarize_document(UUID(file_id), SummaryType(summary_type), max_words, model)
        )
        return result
    except Exception as exc:
        # Retry on failure
        self.retry(exc=exc)


async def _async_summarize_document(
    file_id: UUID, summary_type: SummaryType, max_words: int, model: str | None
) -> dict:
    """Async helper for document summarization.

    Args:
        file_id: File UUID
        summary_type: Summary type
        max_words: Word count
        model: Model name

    Returns:
        Summary result dict
    """
    service = get_summarizer_service()

    # Initialize service if needed
    if not service.list_downloaded_models():
        await service.initialize()

    async with get_db_context() as db:
        # Get file with text content
        result = await db.execute(select(File).where(File.id == file_id))
        file = result.scalar_one_or_none()

        if not file or not file.text_content:
            raise ValueError(f"File {file_id} not found or has no text content")

        text = file.text_content.content

        # Generate summary
        summary = await service.summarize(
            text=text, summary_type=summary_type, max_words=max_words, model=model
        )

        # Save to database
        import hashlib

        text_hash = hashlib.sha256(text.encode()).hexdigest()

        db_summary = SummaryModel(
            file_id=file_id,
            text=summary.text,
            summary_type=summary_type.value,
            source_text_hash=text_hash,
            source_length=summary.source_length,
            summary_length=summary.summary_length,
            compression_ratio=summary.compression_ratio,
            model=summary.model,
            language=summary.language,
            generation_time=summary.generation_time,
            tokens_per_second=summary.tokens_per_second,
            metadata=summary.metadata,
        )

        db.add(db_summary)
        await db.commit()
        await db.refresh(db_summary)

        return db_summary.to_dict()


@shared_task(name="batch_summarize_documents_task", bind=True, max_retries=2)
def batch_summarize_documents_task(
    self,
    file_ids: list[str],
    summary_type: str = "abstractive",
    max_words: int = 150,
    model: str | None = None,
) -> dict:
    """Background task to summarize multiple documents.

    Args:
        self: Celery task instance
        file_ids: List of file UUIDs
        summary_type: Type of summary
        max_words: Target word count
        model: Model to use

    Returns:
        Batch result with stats

    Example:
        >>> from tasks.summarize_tasks import batch_summarize_documents_task
        >>> task = batch_summarize_documents_task.delay(
        ...     file_ids=["uuid1", "uuid2"],
        ...     summary_type="bullet_points"
        ... )
    """
    try:
        result = asyncio.run(
            _async_batch_summarize(
                [UUID(fid) for fid in file_ids], SummaryType(summary_type), max_words, model
            )
        )
        return result
    except Exception as exc:
        self.retry(exc=exc)


async def _async_batch_summarize(
    file_ids: list[UUID], summary_type: SummaryType, max_words: int, model: str | None
) -> dict:
    """Async helper for batch summarization.

    Args:
        file_ids: List of file UUIDs
        summary_type: Summary type
        max_words: Word count
        model: Model name

    Returns:
        Batch result dict
    """
    service = get_summarizer_service()

    if not service.list_downloaded_models():
        await service.initialize()

    summaries = []
    errors = []
    start_time = datetime.now(UTC)

    for file_id in file_ids:
        try:
            summary = await _async_summarize_document(file_id, summary_type, max_words, model)
            summaries.append(summary)
        except Exception as e:
            errors.append({"file_id": str(file_id), "error": str(e)})

    end_time = datetime.now(UTC)
    total_time = (end_time - start_time).total_seconds()

    return {
        "summaries": summaries,
        "total_count": len(file_ids),
        "success_count": len(summaries),
        "failed_count": len(errors),
        "total_time": total_time,
        "errors": errors,
    }


@shared_task(name="auto_summarize_on_index", bind=True)
def auto_summarize_on_index(self, file_id: str, summary_types: list[str] | None = None) -> dict:
    """Auto-generate summaries when a document is indexed.

    This task is called from the indexing pipeline if auto-summarization
    is enabled. It generates multiple summary types for the document.

    Args:
        self: Celery task instance
        file_id: UUID of newly indexed file
        summary_types: Summary types to generate (defaults to tldr and abstractive)

    Returns:
        Result with generated summaries

    Example:
        >>> from tasks.summarize_tasks import auto_summarize_on_index
        >>> task = auto_summarize_on_index.delay(
        ...     file_id="123e4567-e89b-12d3-a456-426614174000"
        ... )
    """
    if summary_types is None:
        summary_types = ["tldr", "abstractive"]

    try:
        result = asyncio.run(
            _async_auto_summarize(UUID(file_id), [SummaryType(st) for st in summary_types])
        )
        return result
    except Exception as exc:
        self.retry(exc=exc)


async def _async_auto_summarize(file_id: UUID, summary_types: list[SummaryType]) -> dict:
    """Async helper for auto-summarization.

    Args:
        file_id: File UUID
        summary_types: Summary types to generate

    Returns:
        Auto-summarization result
    """
    service = get_summarizer_service()

    if not service.list_downloaded_models():
        await service.initialize()

    summaries = []
    errors = []

    for summary_type in summary_types:
        try:
            # Use appropriate word count for each type
            max_words = 50 if summary_type == SummaryType.TLDR else 150

            summary = await _async_summarize_document(
                file_id,
                summary_type,
                max_words,
                None,  # Use default model
            )
            summaries.append(summary)
        except Exception as e:
            errors.append({"summary_type": summary_type.value, "error": str(e)})

    return {
        "file_id": str(file_id),
        "summaries": summaries,
        "summary_types_requested": [st.value for st in summary_types],
        "success_count": len(summaries),
        "failed_count": len(errors),
        "errors": errors,
    }


@shared_task(name="cleanup_expired_summaries", bind=True)
def cleanup_expired_summaries(self) -> dict:
    """Clean up expired cached summaries.

    This task should be run periodically (e.g., daily) to remove
    expired summary cache entries.

    Returns:
        Cleanup stats

    Example:
        >>> from tasks.summarize_tasks import cleanup_expired_summaries
        >>> task = cleanup_expired_summaries.delay()
    """
    try:
        result = asyncio.run(_async_cleanup_expired())
        return result
    except Exception as exc:
        self.retry(exc=exc)


async def _async_cleanup_expired() -> dict:
    """Async helper for cleanup.

    Returns:
        Cleanup statistics
    """
    from sqlalchemy import delete

    async with get_db_context() as db:
        # Delete expired summaries
        result = await db.execute(
            delete(SummaryModel).where(
                SummaryModel.expires_at != None,
                SummaryModel.expires_at <= datetime.now(UTC),
            )
        )

        deleted_count = result.rowcount
        await db.commit()

        return {
            "deleted_count": deleted_count,
            "cleaned_at": datetime.now(UTC).isoformat(),
        }


@shared_task(name="regenerate_summaries_for_file", bind=True)
def regenerate_summaries_for_file(self, file_id: str, force: bool = False) -> dict:
    """Regenerate all summaries for a file.

    Useful when a document is updated or when switching models.

    Args:
        self: Celery task instance
        file_id: UUID of the file
        force: Force regeneration even if summaries exist

    Returns:
        Regeneration result

    Example:
        >>> from tasks.summarize_tasks import regenerate_summaries_for_file
        >>> task = regenerate_summaries_for_file.delay(
        ...     file_id="123e4567-e89b-12d3-a456-426614174000",
        ...     force=True
        ... )
    """
    try:
        result = asyncio.run(_async_regenerate_summaries(UUID(file_id), force))
        return result
    except Exception as exc:
        self.retry(exc=exc)


async def _async_regenerate_summaries(file_id: UUID, force: bool) -> dict:
    """Async helper for regeneration.

    Args:
        file_id: File UUID
        force: Force regeneration

    Returns:
        Regeneration result
    """
    from sqlalchemy import delete

    async with get_db_context() as db:
        # Get existing summaries
        result = await db.execute(select(SummaryModel).where(SummaryModel.file_id == file_id))
        existing = result.scalars().all()

        if force and existing:
            # Delete existing summaries
            await db.execute(delete(SummaryModel).where(SummaryModel.file_id == file_id))
            await db.commit()

        # Regenerate with common types
        summary_types = [SummaryType.TLDR, SummaryType.ABSTRACTIVE, SummaryType.BULLET_POINTS]

        result = await _async_auto_summarize(file_id, summary_types)

        result["regenerated"] = True
        result["deleted_count"] = len(existing) if force else 0

        return result
