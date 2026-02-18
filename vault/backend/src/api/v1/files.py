from __future__ import annotations

import logging
import os
from pathlib import Path
import tempfile
from uuid import UUID

from fastapi import (
    APIRouter,
    BackgroundTasks,
    Depends,
    File,
    HTTPException,
    Query,
    Request,
    UploadFile,
)
from fastapi.responses import FileResponse
from sqlalchemy import delete, func, select
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.dependencies import get_db, get_indexing_service
from src.api.errors import IndexingError, NotFoundError, StorageError, ValidationError
from src.api.validators import validate_upload_file
from src.auth.dependencies import get_current_active_user
from src.auth.models import User
from src.middleware.csrf import csrf_protect
from src.middleware.rate_limit import limiter
from src.models.file import File as FileModel
from src.models.tag import FileTag, Tag
from src.models.text_content import TextContent
from src.schemas.file import (
    FileListResponse,
    FileMetadata,
    FileMetadataUpdate,
    FileUploadResponse,
)
from src.services.indexing import IndexingService
from src.utils.security import SecurityError, validate_safe_path

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/files", tags=["files"])


@router.post("/", response_model=FileUploadResponse, status_code=201)
@limiter.limit("10/minute")
async def upload_file(
    request: Request,
    file: UploadFile = File(...),
    tags: list[str] | None = Query(None),
    description: str | None = Query(None),
    background_tasks: BackgroundTasks = BackgroundTasks(),
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
    indexing_service: IndexingService = Depends(get_indexing_service),
) -> FileUploadResponse:
    """
    Upload a file and trigger background indexing.
    """
    if not file.filename:
        raise ValidationError("Filename is required")

    await validate_upload_file(file)

    temp_file_path = None

    try:
        with tempfile.NamedTemporaryFile(
            delete=False, suffix=Path(file.filename).suffix
        ) as temp_file:
            content = await file.read()
            temp_file.write(content)
            temp_file_path = temp_file.name

        logger.info(f"Uploaded file saved to temporary location: {temp_file_path}")

        success, error_msg = await indexing_service.index_file(temp_file_path)

        if not success:
            raise IndexingError(f"Failed to index file: {error_msg}")

        stmt = select(FileModel).where(FileModel.path == temp_file_path)
        result = await db.execute(stmt)
        file_record = result.scalar_one_or_none()

        if not file_record:
            raise IndexingError("File was indexed but not found in database")

        if tags:
            for tag_name in tags:
                tag_stmt = select(Tag).where(Tag.name == tag_name)
                tag_result = await db.execute(tag_stmt)
                tag = tag_result.scalar_one_or_none()

                if not tag:
                    tag = Tag(name=tag_name)
                    db.add(tag)
                    await db.flush()

                file_tag = FileTag(file_id=file_record.id, tag_id=tag.id)
                db.add(file_tag)

            await db.commit()

        return FileUploadResponse(
            id=file_record.id,
            filename=file_record.filename,
            size_bytes=file_record.size_bytes,
            mime_type=file_record.mime_type,
            upload_status="completed",
            created_at=file_record.created_at,
        )

    except IndexingError:
        raise
    except ValidationError:
        raise
    except Exception as e:
        logger.error(f"Failed to upload and index file: {e}", exc_info=True)
        raise StorageError(f"Failed to process file upload: {e!s}")
    finally:
        if temp_file_path and os.path.exists(temp_file_path):
            try:
                os.unlink(temp_file_path)
            except Exception as e:
                logger.warning(f"Failed to cleanup temp file {temp_file_path}: {e}")


@router.get("/{file_id}", response_model=FileMetadata)
async def get_file_metadata(
    file_id: UUID,
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
) -> FileMetadata:
    """Get file metadata by ID."""
    stmt = select(FileModel).where(FileModel.id == file_id)
    result = await db.execute(stmt)
    file_record = result.scalar_one_or_none()

    if not file_record:
        raise NotFoundError(f"File {file_id} not found")

    tags_stmt = (
        select(Tag.name).join(FileTag, Tag.id == FileTag.tag_id).where(FileTag.file_id == file_id)
    )
    tags_result = await db.execute(tags_stmt)
    tags = [row[0] for row in tags_result.fetchall()]

    return FileMetadata(
        id=file_record.id,
        filename=file_record.filename,
        original_path=file_record.path,
        size_bytes=file_record.size_bytes,
        mime_type=file_record.mime_type,
        tags=tags,
        description=None,
        custom_metadata={},
        upload_status="completed",
        indexed_at=file_record.indexed_at,
        created_at=file_record.created_at,
        updated_at=file_record.updated_at,
    )


@router.patch("/{file_id}", response_model=FileMetadata)
async def update_file_metadata(
    file_id: UUID,
    update: FileMetadataUpdate,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
) -> FileMetadata:
    """Update file metadata (tags, description, custom fields)."""
    stmt = select(FileModel).where(FileModel.id == file_id)
    result = await db.execute(stmt)
    file_record = result.scalar_one_or_none()

    if not file_record:
        raise NotFoundError(f"File {file_id} not found")

    if update.tags is not None:
        delete_stmt = delete(FileTag).where(FileTag.file_id == file_id)
        await db.execute(delete_stmt)

        for tag_name in update.tags:
            tag_stmt = select(Tag).where(Tag.name == tag_name)
            tag_result = await db.execute(tag_stmt)
            tag = tag_result.scalar_one_or_none()

            if not tag:
                tag = Tag(name=tag_name)
                db.add(tag)
                await db.flush()

            file_tag = FileTag(file_id=file_id, tag_id=tag.id)
            db.add(file_tag)

    await db.commit()
    await db.refresh(file_record)

    tags_stmt = (
        select(Tag.name).join(FileTag, Tag.id == FileTag.tag_id).where(FileTag.file_id == file_id)
    )
    tags_result = await db.execute(tags_stmt)
    tags = [row[0] for row in tags_result.fetchall()]

    return FileMetadata(
        id=file_record.id,
        filename=file_record.filename,
        original_path=file_record.path,
        size_bytes=file_record.size_bytes,
        mime_type=file_record.mime_type,
        tags=tags,
        description=update.description,
        custom_metadata=update.custom_metadata or {},
        upload_status="completed",
        indexed_at=file_record.indexed_at,
        created_at=file_record.created_at,
        updated_at=file_record.updated_at,
    )


@router.delete("/{file_id}", status_code=204)
@limiter.limit("20/minute")
async def delete_file(
    request: Request,
    file_id: UUID,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
) -> None:
    """Delete file and cleanup vectors."""
    stmt = select(FileModel).where(FileModel.id == file_id)
    result = await db.execute(stmt)
    file_record = result.scalar_one_or_none()

    if not file_record:
        raise NotFoundError(f"File {file_id} not found")

    try:
        if file_record.path and os.path.exists(file_record.path):
            os.unlink(file_record.path)
            logger.info(f"Deleted physical file: {file_record.path}")
    except Exception as e:
        logger.warning(f"Failed to delete physical file {file_record.path}: {e}")

    await db.delete(file_record)
    await db.commit()

    logger.info(f"Deleted file record: {file_id}")


@router.get("/", response_model=FileListResponse)
@limiter.limit("30/minute")
async def list_files(
    request: Request,
    page: int = Query(1, ge=1),
    page_size: int = Query(20, ge=1, le=100),
    tags: list[str] | None = Query(None),
    mime_type: str | None = Query(None),
    search: str | None = Query(None),
    sort_by: str = Query("created_at", pattern="^(created_at|updated_at|filename|size_bytes)$"),
    sort_order: str = Query("desc", pattern="^(asc|desc)$"),
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
) -> FileListResponse:
    """List files with pagination and filters."""
    stmt = select(FileModel)

    if mime_type:
        stmt = stmt.where(FileModel.mime_type == mime_type)

    if search:
        stmt = stmt.where(FileModel.filename.ilike(f"%{search}%"))

    if tags:
        stmt = (
            stmt.join(FileTag, FileModel.id == FileTag.file_id)
            .join(Tag, FileTag.tag_id == Tag.id)
            .where(Tag.name.in_(tags))
        )

    count_stmt = select(func.count()).select_from(stmt.subquery())
    count_result = await db.execute(count_stmt)
    total = count_result.scalar_one()

    order_column = getattr(FileModel, sort_by)
    if sort_order == "desc":
        stmt = stmt.order_by(order_column.desc())
    else:
        stmt = stmt.order_by(order_column.asc())

    stmt = stmt.offset((page - 1) * page_size).limit(page_size)

    result = await db.execute(stmt)
    files = result.scalars().all()

    items = []
    for file_record in files:
        tags_stmt = (
            select(Tag.name)
            .join(FileTag, Tag.id == FileTag.tag_id)
            .where(FileTag.file_id == file_record.id)
        )
        tags_result = await db.execute(tags_stmt)
        file_tags = [row[0] for row in tags_result.fetchall()]

        items.append(
            FileMetadata(
                id=file_record.id,
                filename=file_record.filename,
                original_path=file_record.path,
                size_bytes=file_record.size_bytes,
                mime_type=file_record.mime_type,
                tags=file_tags,
                description=None,
                custom_metadata={},
                upload_status="completed",
                indexed_at=file_record.indexed_at,
                created_at=file_record.created_at,
                updated_at=file_record.updated_at,
            )
        )

    total_pages = (total + page_size - 1) // page_size

    return FileListResponse(
        items=items, total=total, page=page, page_size=page_size, total_pages=total_pages
    )


@router.get("/{file_id}/download")
@limiter.limit("100/minute")
async def download_file(
    request: Request,
    file_id: UUID,
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
) -> FileResponse:
    """Download file content."""
    stmt = select(FileModel).where(FileModel.id == file_id)
    result = await db.execute(stmt)
    file_record = result.scalar_one_or_none()

    if not file_record:
        raise NotFoundError(f"File {file_id} not found")

    if not file_record.path or not os.path.exists(file_record.path):
        raise NotFoundError(f"Physical file not found for file {file_id}")

    # Validate path to prevent traversal attacks
    try:
        from src.config.settings import get_settings

        settings = get_settings()
        safe_path = validate_safe_path(
            file_record.path, settings.storage_base_path, allow_symlinks=False, must_exist=True
        )
    except SecurityError as e:
        raise HTTPException(status_code=403, detail=str(e))

    return FileResponse(
        path=str(safe_path), filename=file_record.filename, media_type=file_record.mime_type
    )


@router.get("/{file_id}/content")
async def get_file_content(
    file_id: UUID,
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
) -> dict:
    """Get extracted text content (not raw file)."""
    stmt = select(FileModel).where(FileModel.id == file_id)
    result = await db.execute(stmt)
    file_record = result.scalar_one_or_none()

    if not file_record:
        raise NotFoundError(f"File {file_id} not found")

    content_stmt = select(TextContent).where(TextContent.file_id == file_id)
    content_result = await db.execute(content_stmt)
    text_content = content_result.scalar_one_or_none()

    if not text_content:
        return {
            "file_id": str(file_id),
            "filename": file_record.filename,
            "content": None,
            "word_count": 0,
            "char_count": 0,
            "language": None,
            "message": "No text content extracted from this file",
        }

    return {
        "file_id": str(file_id),
        "filename": file_record.filename,
        "content": text_content.content,
        "word_count": text_content.word_count,
        "char_count": text_content.char_count,
        "language": text_content.language,
    }
