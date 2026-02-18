"""Celery tasks for document auto-tagging and clustering.

This module provides background tasks for:
- Batch document tagging
- Automatic tagging on indexing
- Tag maintenance and cleanup
"""

import asyncio
import logging
from typing import Any

from celery import shared_task
from sqlalchemy.orm import Session

from src.db.session import SessionLocal
from src.models import AutoTag, EntityType, File, FileAutoTag, TagSource
from src.modules.tagger import AutoTagger

logger = logging.getLogger(__name__)


@shared_task(bind=True, max_retries=3)
def batch_tag_documents_task(
    self, file_ids: list[str], min_confidence: float = 0.6, max_tags: int = 20
) -> dict[str, Any]:
    """Background task for batch document tagging.

    Args:
        self: Celery task instance
        file_ids: List of file IDs to tag
        min_confidence: Minimum confidence threshold
        max_tags: Maximum tags per document

    Returns:
        Dictionary with task results
    """
    db = SessionLocal()

    try:
        tagger = AutoTagger(min_confidence=min_confidence, max_tags_per_source=max_tags // 4)

        results = []
        success_count = 0
        error_count = 0

        for file_id in file_ids:
            try:
                file = db.query(File).filter(File.id == file_id).first()

                if not file or not file.text_content:
                    results.append(
                        {"file_id": file_id, "status": "skipped", "reason": "No text content"}
                    )
                    continue

                metadata = {
                    "filename": file.filename,
                    "extension": file.extension,
                    "size_bytes": file.size_bytes,
                }

                tags = asyncio.run(
                    tagger.tag_document(
                        text=file.text_content.content, metadata=metadata, file_id=file_id
                    )
                )

                for tag_data in tags[:max_tags]:
                    auto_tag = _get_or_create_tag_sync(db, tag_data)

                    _create_or_update_file_tag_sync(
                        db,
                        file_id=file_id,
                        tag_id=str(auto_tag.id),
                        confidence=tag_data["confidence"],
                        context=tag_data.get("context"),
                        metadata=tag_data.get("metadata"),
                    )

                db.commit()

                results.append({"file_id": file_id, "status": "success", "tag_count": len(tags)})
                success_count += 1

            except Exception as e:
                logger.error(f"Error tagging file {file_id}: {e}")
                db.rollback()

                results.append({"file_id": file_id, "status": "error", "error": str(e)})
                error_count += 1

        return {
            "status": "completed",
            "total_files": len(file_ids),
            "success_count": success_count,
            "error_count": error_count,
            "results": results,
        }

    except Exception as e:
        logger.error(f"Batch tagging task failed: {e}")

        try:
            self.retry(countdown=60, exc=e)
        except self.MaxRetriesExceededError:
            return {"status": "failed", "error": str(e)}

    finally:
        db.close()


@shared_task
def auto_tag_on_index(file_id: str) -> dict[str, Any]:
    """Auto-tag a document immediately after indexing.

    Args:
        file_id: File ID to tag

    Returns:
        Dictionary with tagging results
    """
    db = SessionLocal()

    try:
        file = db.query(File).filter(File.id == file_id).first()

        if not file or not file.text_content:
            return {"status": "skipped", "reason": "No text content"}

        tagger = AutoTagger()

        metadata = {
            "filename": file.filename,
            "extension": file.extension,
            "size_bytes": file.size_bytes,
        }

        tags = asyncio.run(
            tagger.tag_document(text=file.text_content.content, metadata=metadata, file_id=file_id)
        )

        for tag_data in tags:
            auto_tag = _get_or_create_tag_sync(db, tag_data)

            _create_or_update_file_tag_sync(
                db,
                file_id=file_id,
                tag_id=str(auto_tag.id),
                confidence=tag_data["confidence"],
                context=tag_data.get("context"),
                metadata=tag_data.get("metadata"),
            )

        db.commit()

        return {"status": "success", "file_id": file_id, "tag_count": len(tags)}

    except Exception as e:
        logger.error(f"Error auto-tagging file {file_id}: {e}")
        db.rollback()

        return {"status": "error", "file_id": file_id, "error": str(e)}

    finally:
        db.close()


@shared_task
def cleanup_unused_tags() -> dict[str, Any]:
    """Remove tags that are not associated with any files.

    Returns:
        Dictionary with cleanup results
    """
    db = SessionLocal()

    try:
        unused_tags = db.query(AutoTag).outerjoin(FileAutoTag).filter(FileAutoTag.id == None).all()

        count = len(unused_tags)

        for tag in unused_tags:
            db.delete(tag)

        db.commit()

        return {"status": "success", "deleted_count": count}

    except Exception as e:
        logger.error(f"Error cleaning up tags: {e}")
        db.rollback()

        return {"status": "error", "error": str(e)}

    finally:
        db.close()


def _get_or_create_tag_sync(db: Session, tag_data: dict[str, Any]) -> AutoTag:
    """Get existing tag or create new one (synchronous)."""
    tag = (
        db.query(AutoTag)
        .filter(AutoTag.name == tag_data["name"], AutoTag.source == TagSource(tag_data["source"]))
        .first()
    )

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
        db.flush()

    return tag


def _create_or_update_file_tag_sync(
    db: Session,
    file_id: str,
    tag_id: str,
    confidence: float,
    context: str,
    metadata: dict[str, Any],
) -> FileAutoTag:
    """Create or update file-tag association (synchronous)."""
    file_tag = (
        db.query(FileAutoTag)
        .filter(FileAutoTag.file_id == file_id, FileAutoTag.auto_tag_id == tag_id)
        .first()
    )

    if not file_tag:
        file_tag = FileAutoTag(
            file_id=file_id,
            auto_tag_id=tag_id,
            confidence=confidence,
            context=context,
            metadata=metadata,
        )
        db.add(file_tag)
    else:
        file_tag.confidence = confidence
        file_tag.context = context
        file_tag.metadata = metadata

    db.flush()
    return file_tag
