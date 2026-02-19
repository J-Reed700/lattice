"""Tags API endpoints for auto-tagging and tag management.

This module provides REST API endpoints for:
- Auto-tagging documents
- Batch tagging operations
- Tag suggestions and autocomplete
- Tag statistics and management
- Hierarchical tag relationships
"""

from __future__ import annotations

import logging
from typing import Any
from uuid import UUID

from fastapi import APIRouter, BackgroundTasks, Depends, HTTPException, Query
from pydantic import BaseModel, Field
from sqlalchemy import and_, func, select
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.dependencies import get_db
from src.auth.dependencies import get_current_active_user
from src.auth.models import User
from src.middleware.csrf import csrf_protect
from src.models import AutoTag, EntityType, File, FileAutoTag, TagSource
from src.modules.tagger import AutoTagger

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/tags", tags=["tags"])


class AutoTagRequest(BaseModel):
    """Request model for auto-tagging."""

    file_id: UUID
    min_confidence: float = Field(default=0.6, ge=0.0, le=1.0)
    max_tags: int = Field(default=20, gt=0, le=100)
    enable_keywords: bool = True
    enable_entities: bool = True
    enable_topics: bool = True
    enable_rules: bool = True


class BatchAutoTagRequest(BaseModel):
    """Request model for batch auto-tagging."""

    file_ids: list[UUID]
    min_confidence: float = Field(default=0.6, ge=0.0, le=1.0)
    max_tags: int = Field(default=20, gt=0, le=100)


class TagSuggestionRequest(BaseModel):
    """Request model for tag suggestions."""

    query: str = Field(..., min_length=1, max_length=100)
    limit: int = Field(default=10, gt=0, le=50)
    source: TagSource | None = None


class TagResponse(BaseModel):
    """Response model for a single tag."""

    id: UUID
    name: str
    source: TagSource
    entity_type: EntityType | None
    confidence: float
    metadata: dict[str, Any] | None
    usage_count: int


class FileTagResponse(BaseModel):
    """Response model for file-tag association."""

    file_id: UUID
    tag_id: UUID
    tag_name: str
    source: TagSource
    confidence: float
    context: str | None


class TagStatsResponse(BaseModel):
    """Response model for tag statistics."""

    total_tags: int
    by_source: dict[str, int]
    by_entity_type: dict[str, int]
    top_tags: list[dict[str, Any]]
    recent_tags: list[dict[str, Any]]


@router.post("/auto-tag/{file_id}", response_model=list[FileTagResponse])
async def auto_tag_document(
    file_id: UUID,
    request: AutoTagRequest,
    background_tasks: BackgroundTasks,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
):
    """Auto-tag a single document.

    Extracts tags automatically using keyword extraction, NER,
    topic modeling, and rule-based methods.

    Args:
        file_id: Document ID to tag
        request: Auto-tagging configuration
        background_tasks: FastAPI background tasks
        db: Database session

    Returns:
        List of extracted tags with confidence scores

    Raises:
        HTTPException: If file not found or processing error
    """
    try:
        result = await db.execute(select(File).where(File.id == file_id))
        file = result.scalar_one_or_none()

        if not file:
            raise HTTPException(status_code=404, detail="File not found")

        text_content = None
        if file.text_content:
            text_content = file.text_content.content

        if not text_content:
            raise HTTPException(status_code=400, detail="File has no text content to analyze")

        tagger = AutoTagger(
            min_confidence=request.min_confidence,
            max_tags_per_source=request.max_tags // 4,
            enable_keywords=request.enable_keywords,
            enable_entities=request.enable_entities,
            enable_topics=request.enable_topics,
            enable_rules=request.enable_rules,
        )

        metadata = {
            "filename": file.filename,
            "extension": file.extension,
            "size_bytes": file.size_bytes,
        }

        tags = await tagger.tag_document(text=text_content, metadata=metadata, file_id=str(file_id))

        responses = []

        for tag_data in tags[: request.max_tags]:
            auto_tag = await _get_or_create_tag(db, tag_data)

            file_tag = await _get_or_create_file_tag(
                db,
                file_id=file_id,
                tag_id=auto_tag.id,
                confidence=tag_data["confidence"],
                context=tag_data.get("context"),
                metadata=tag_data.get("metadata"),
            )

            responses.append(
                FileTagResponse(
                    file_id=file_id,
                    tag_id=auto_tag.id,
                    tag_name=auto_tag.name,
                    source=auto_tag.source,
                    confidence=file_tag.confidence,
                    context=file_tag.context,
                )
            )

        await db.commit()

        return responses

    except HTTPException:
        raise
    except Exception as e:
        logger.error(f"Error auto-tagging document {file_id}: {e}")
        await db.rollback()
        raise HTTPException(status_code=500, detail=str(e))


@router.post("/batch-tag", response_model=dict[str, Any])
async def batch_auto_tag(
    request: BatchAutoTagRequest,
    background_tasks: BackgroundTasks,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
):
    """Auto-tag multiple documents in batch.

    For large batches, creates a background task.

    Args:
        request: Batch tagging configuration
        background_tasks: FastAPI background tasks
        db: Database session

    Returns:
        Task status or immediate results for small batches
    """
    try:
        if len(request.file_ids) > 100:
            from src.tasks.tagging_tasks import batch_tag_documents_task

            task = batch_tag_documents_task.delay(
                file_ids=[str(fid) for fid in request.file_ids],
                min_confidence=request.min_confidence,
                max_tags=request.max_tags,
            )

            return {
                "status": "queued",
                "task_id": task.id,
                "file_count": len(request.file_ids),
                "message": "Batch tagging queued as background task",
            }

        results = []
        for file_id in request.file_ids:
            try:
                tags = await auto_tag_document(
                    file_id=file_id,
                    request=AutoTagRequest(
                        file_id=file_id,
                        min_confidence=request.min_confidence,
                        max_tags=request.max_tags,
                    ),
                    background_tasks=background_tasks,
                    db=db,
                )
                results.append(
                    {"file_id": str(file_id), "status": "success", "tag_count": len(tags)}
                )
            except Exception as e:
                results.append({"file_id": str(file_id), "status": "error", "error": str(e)})

        return {
            "status": "completed",
            "results": results,
            "success_count": sum(1 for r in results if r["status"] == "success"),
            "error_count": sum(1 for r in results if r["status"] == "error"),
        }

    except Exception as e:
        logger.error(f"Error in batch auto-tagging: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/suggestions", response_model=list[TagResponse])
async def get_tag_suggestions(
    query: str = Query(..., min_length=1, max_length=100),
    limit: int = Query(default=10, gt=0, le=50),
    source: TagSource | None = Query(default=None),
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
):
    """Get tag suggestions for autocomplete.

    Args:
        query: Search query
        limit: Maximum number of suggestions
        source: Optional filter by tag source
        db: Database session

    Returns:
        List of matching tags with usage counts
    """
    try:
        stmt = (
            select(AutoTag, func.count(FileAutoTag.id).label("usage_count"))
            .outerjoin(FileAutoTag)
            .where(AutoTag.name.ilike(f"%{query}%"))
        )

        if source:
            stmt = stmt.where(AutoTag.source == source)

        stmt = stmt.group_by(AutoTag.id).order_by(func.count(FileAutoTag.id).desc()).limit(limit)

        result = await db.execute(stmt)
        rows = result.all()

        responses = []
        for tag, usage_count in rows:
            responses.append(
                TagResponse(
                    id=tag.id,
                    name=tag.name,
                    source=tag.source,
                    entity_type=tag.entity_type,
                    confidence=tag.confidence,
                    metadata=tag.metadata,
                    usage_count=usage_count,
                )
            )

        return responses

    except Exception as e:
        logger.error(f"Error getting tag suggestions: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/stats", response_model=TagStatsResponse)
async def get_tag_statistics(
    current_user: User = Depends(get_current_active_user), db: AsyncSession = Depends(get_db)
):
    """Get comprehensive tag statistics.

    Returns:
        Tag statistics including counts by source and entity type
    """
    try:
        total_result = await db.execute(select(func.count(AutoTag.id)))
        total_tags = total_result.scalar()

        source_result = await db.execute(
            select(AutoTag.source, func.count(AutoTag.id)).group_by(AutoTag.source)
        )
        by_source = {row[0].value: row[1] for row in source_result.all()}

        entity_result = await db.execute(
            select(AutoTag.entity_type, func.count(AutoTag.id))
            .where(AutoTag.entity_type.isnot(None))
            .group_by(AutoTag.entity_type)
        )
        by_entity_type = {row[0].value: row[1] for row in entity_result.all() if row[0] is not None}

        top_tags_result = await db.execute(
            select(AutoTag, func.count(FileAutoTag.id).label("usage_count"))
            .join(FileAutoTag)
            .group_by(AutoTag.id)
            .order_by(func.count(FileAutoTag.id).desc())
            .limit(20)
        )

        top_tags = [
            {
                "id": str(tag.id),
                "name": tag.name,
                "source": tag.source.value,
                "usage_count": usage_count,
            }
            for tag, usage_count in top_tags_result.all()
        ]

        recent_tags_result = await db.execute(
            select(AutoTag).order_by(AutoTag.created_at.desc()).limit(20)
        )

        recent_tags = [
            {
                "id": str(tag.id),
                "name": tag.name,
                "source": tag.source.value,
                "created_at": tag.created_at.isoformat(),
            }
            for tag in recent_tags_result.scalars().all()
        ]

        return TagStatsResponse(
            total_tags=total_tags,
            by_source=by_source,
            by_entity_type=by_entity_type,
            top_tags=top_tags,
            recent_tags=recent_tags,
        )

    except Exception as e:
        logger.error(f"Error getting tag statistics: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/file/{file_id}", response_model=list[FileTagResponse])
async def get_file_tags(
    file_id: UUID,
    min_confidence: float = Query(default=0.0, ge=0.0, le=1.0),
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
):
    """Get all tags for a specific file.

    Args:
        file_id: File ID
        min_confidence: Minimum confidence filter
        db: Database session

    Returns:
        List of tags associated with the file
    """
    try:
        stmt = (
            select(FileAutoTag, AutoTag)
            .join(AutoTag)
            .where(and_(FileAutoTag.file_id == file_id, FileAutoTag.confidence >= min_confidence))
            .order_by(FileAutoTag.confidence.desc())
        )

        result = await db.execute(stmt)
        rows = result.all()

        responses = []
        for file_tag, auto_tag in rows:
            responses.append(
                FileTagResponse(
                    file_id=file_id,
                    tag_id=auto_tag.id,
                    tag_name=auto_tag.name,
                    source=auto_tag.source,
                    confidence=file_tag.confidence,
                    context=file_tag.context,
                )
            )

        return responses

    except Exception as e:
        logger.error(f"Error getting file tags: {e}")
        raise HTTPException(status_code=500, detail=str(e))


async def _get_or_create_tag(db: AsyncSession, tag_data: dict[str, Any]) -> AutoTag:
    """Get existing tag or create new one."""
    result = await db.execute(
        select(AutoTag).where(
            and_(AutoTag.name == tag_data["name"], AutoTag.source == TagSource(tag_data["source"]))
        )
    )
    tag = result.scalar_one_or_none()

    if not tag:
        entity_type = None
        if tag_data.get("entity_type"):
            entity_type = EntityType(tag_data["entity_type"])

        tag = AutoTag(
            name=tag_data["name"],
            source=TagSource(tag_data["source"]),
            entity_type=entity_type,
            confidence=tag_data["confidence"],
            metadata=tag_data.get("metadata"),
        )
        db.add(tag)
        await db.flush()

    return tag


async def _get_or_create_file_tag(
    db: AsyncSession,
    file_id: UUID,
    tag_id: UUID,
    confidence: float,
    context: str | None,
    metadata: dict[str, Any] | None,
) -> FileAutoTag:
    """Get existing file-tag association or create new one."""
    result = await db.execute(
        select(FileAutoTag).where(
            and_(FileAutoTag.file_id == file_id, FileAutoTag.auto_tag_id == tag_id)
        )
    )
    file_tag = result.scalar_one_or_none()

    if not file_tag:
        file_tag = FileAutoTag(
            file_id=file_id,
            auto_tag_id=tag_id,
            confidence=confidence,
            context=context,
            metadata=metadata,
        )
        db.add(file_tag)
        await db.flush()
    else:
        file_tag.confidence = confidence
        file_tag.context = context
        file_tag.metadata = metadata

    return file_tag
