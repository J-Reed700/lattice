from __future__ import annotations

import asyncio
import logging
import os
from pathlib import Path
import tempfile
from uuid import UUID

from fastapi import APIRouter, Depends, File, HTTPException, UploadFile
from fastapi.responses import FileResponse, StreamingResponse
from pydantic import BaseModel
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy.orm import selectinload

from src.api.dependencies import get_db, get_indexing_service
from src.api.errors import NotFoundError, StorageError, ValidationError
from src.auth.dependencies import get_current_active_user
from src.auth.models import User
from src.config import get_settings
from src.middleware.csrf import csrf_protect
from src.models.file import File as FileModel
from src.schemas.file import FileUploadResponse
from src.services.indexing import IndexingService
from src.services.upload_service import UploadService
from src.utils.security import SecurityError, validate_safe_path

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/documents", tags=["documents"])

settings = get_settings()
upload_service = UploadService(settings.upload_dir)

_background_tasks: set[asyncio.Task] = set()


def _log_task_exception(task: asyncio.Task) -> None:
    if not task.cancelled() and (exc := task.exception()):
        logger.error(f"Background task failed: {exc}", exc_info=exc)


class UploadTaskResponse(BaseModel):
    task_id: str
    total_files: int


class DocumentContent(BaseModel):
    id: str
    filename: str
    path: str
    content: str
    content_type: str
    size: int
    modified_at: int


ALLOWED_MIME_TYPES = {
    "application/pdf",
    "text/plain",
    "text/markdown",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
}

ALLOWED_EXTENSIONS = {".pdf", ".txt", ".md", ".docx"}

MAX_FILE_SIZE = 10 * 1024 * 1024


@router.post("/", response_model=FileUploadResponse, status_code=201)
async def upload_document(
    file: UploadFile = File(...),
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
    indexing_service: IndexingService = Depends(get_indexing_service),
) -> FileUploadResponse:
    if not file.filename:
        raise ValidationError("Filename is required")

    file_ext = Path(file.filename).suffix.lower()
    if file_ext not in ALLOWED_EXTENSIONS:
        raise ValidationError(
            f"Unsupported file type. Allowed: {', '.join(ALLOWED_EXTENSIONS)}",
            details={"allowed_extensions": list(ALLOWED_EXTENSIONS)},
        )

    content = await file.read()
    file_size = len(content)

    if file_size > MAX_FILE_SIZE:
        raise ValidationError(
            f"File size exceeds maximum of {MAX_FILE_SIZE / 1024 / 1024}MB",
            details={"max_size_bytes": MAX_FILE_SIZE, "file_size_bytes": file_size},
        )

    if file_size == 0:
        raise ValidationError("File is empty")

    if file.content_type and file.content_type not in ALLOWED_MIME_TYPES:
        logger.warning(
            f"File {file.filename} has content type {file.content_type} "
            f"but will be processed based on extension {file_ext}"
        )

    temp_file_path = None

    try:
        with tempfile.NamedTemporaryFile(delete=False, suffix=file_ext) as temp_file:
            temp_file.write(content)
            temp_file_path = temp_file.name

        logger.info(f"Document saved to temporary location: {temp_file_path}")

        success, error_msg = await indexing_service.index_file(temp_file_path)

        if not success:
            raise StorageError(f"Failed to index document: {error_msg}")

        stmt = select(FileModel).where(FileModel.path == temp_file_path)
        result = await db.execute(stmt)
        file_record = result.scalar_one_or_none()

        if not file_record:
            raise StorageError("Document was indexed but not found in database")

        return FileUploadResponse(
            id=file_record.id,
            filename=file_record.filename,
            size_bytes=file_record.size_bytes,
            mime_type=file_record.mime_type,
            upload_status="completed",
            created_at=file_record.created_at,
        )

    except (ValidationError, StorageError):
        raise
    except Exception as e:
        logger.exception("Failed to upload and index document", extra={"filename": file.filename})
        raise StorageError(f"Failed to process document upload: {e!s}") from e
    finally:
        if temp_file_path and os.path.exists(temp_file_path):
            try:
                os.unlink(temp_file_path)
            except Exception:
                logger.warning(
                    "Failed to cleanup temp file",
                    exc_info=True,
                    extra={"temp_file": temp_file_path},
                )


@router.get("/{document_id}/download")
async def download_document(
    document_id: UUID,
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
) -> FileResponse:
    stmt = select(FileModel).where(FileModel.id == document_id)
    result = await db.execute(stmt)
    file_record = result.scalar_one_or_none()

    if not file_record:
        raise NotFoundError(f"Document {document_id} not found")

    if not file_record.path or not os.path.exists(file_record.path):
        raise NotFoundError(f"Physical file not found for document {document_id}")

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
        path=str(safe_path),
        filename=file_record.filename,
        media_type=file_record.mime_type,
        headers={"Content-Disposition": f'attachment; filename="{file_record.filename}"'},
    )


@router.get("/{document_id}/content", response_model=DocumentContent)
async def get_document_content(
    document_id: UUID,
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
) -> DocumentContent:
    stmt = (
        select(FileModel)
        .options(selectinload(FileModel.text_content))
        .where(FileModel.id == document_id)
    )
    result = await db.execute(stmt)
    file_record = result.scalar_one_or_none()

    if not file_record:
        raise NotFoundError(f"Document {document_id} not found")

    if not file_record.path or not os.path.exists(file_record.path):
        raise NotFoundError(f"Physical file not found for document {document_id}")

    content = ""
    extension = file_record.extension.lower()

    text_file_extensions = {
        ".txt",
        ".md",
        ".py",
        ".js",
        ".ts",
        ".tsx",
        ".jsx",
        ".json",
        ".yaml",
        ".yml",
        ".xml",
        ".html",
        ".css",
        ".sql",
        ".sh",
        ".bash",
        ".java",
        ".c",
        ".cpp",
        ".h",
        ".rs",
        ".go",
        ".rb",
    }

    if extension in text_file_extensions or file_record.mime_type.startswith("text/"):
        try:
            with open(file_record.path, encoding="utf-8") as f:
                content = f.read()
        except UnicodeDecodeError:
            try:
                with open(file_record.path, encoding="latin-1") as f:
                    content = f.read()
            except Exception as e:
                logger.error(
                    "Failed to read file with latin-1 encoding",
                    exc_info=True,
                    extra={"file_path": file_record.path},
                )
                raise StorageError(f"Failed to read file content: {e!s}") from e
        except Exception as e:
            logger.error(
                "Failed to read file", exc_info=True, extra={"file_path": file_record.path}
            )
            raise StorageError(f"Failed to read file content: {e!s}") from e
    elif file_record.text_content:
        content = file_record.text_content.content
    else:
        content = "[Binary file - content not available for preview]"

    content_type = "text"
    if extension == ".md":
        content_type = "markdown"
    elif extension == ".pdf":
        content_type = "pdf"
    elif extension in {
        ".py",
        ".js",
        ".ts",
        ".tsx",
        ".jsx",
        ".json",
        ".yaml",
        ".yml",
        ".xml",
        ".html",
        ".css",
        ".sql",
        ".sh",
        ".bash",
        ".java",
        ".c",
        ".cpp",
        ".h",
        ".rs",
        ".go",
        ".rb",
    }:
        content_type = "code"

    return DocumentContent(
        id=str(file_record.id),
        filename=file_record.filename,
        path=file_record.path,
        content=content,
        content_type=content_type,
        size=file_record.size_bytes,
        modified_at=int(file_record.modified_at.timestamp()),
    )


@router.post("/upload", response_model=UploadTaskResponse, status_code=202)
async def upload_documents(
    files: list[UploadFile] = File(...),
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
    indexing_service: IndexingService = Depends(get_indexing_service),
) -> UploadTaskResponse:
    if not files:
        raise ValidationError("No files provided")

    if len(files) > 20:
        raise ValidationError("Maximum 20 files allowed per upload")

    file_paths = []
    saved_files = []

    try:
        for file in files:
            if not file.filename:
                raise ValidationError("All files must have filenames")

            content = await file.read()
            file_size = len(content)

            upload_service.validate_file(file.filename, file_size)

            file_path = await upload_service.save_upload(file.filename, content)
            file_paths.append(file_path)
            saved_files.append(file.filename)

        task_id = upload_service.create_task(len(file_paths))

        task = asyncio.create_task(
            upload_service.process_uploads(task_id, file_paths, indexing_service)
        )
        _background_tasks.add(task)
        task.add_done_callback(_background_tasks.discard)
        task.add_done_callback(_log_task_exception)

        logger.info(f"Created upload task {task_id} for {len(file_paths)} files")

        return UploadTaskResponse(task_id=task_id, total_files=len(file_paths))

    except ValidationError:
        for file_path in file_paths:
            if file_path.exists():
                file_path.unlink()
        raise
    except Exception as e:
        for file_path in file_paths:
            if file_path.exists():
                file_path.unlink()
        logger.error(f"Failed to process upload: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail=f"Failed to process upload: {e!s}")


@router.get("/upload/{task_id}/status")
async def get_upload_status_sse(
    task_id: str, current_user: User = Depends(get_current_active_user)
):
    async def event_generator():
        try:
            last_progress = None
            start_time = asyncio.get_event_loop().time()
            last_keepalive = start_time

            while True:
                current_time = asyncio.get_event_loop().time()

                try:
                    progress = upload_service.get_progress(task_id)

                    if progress != last_progress:
                        last_progress = progress

                        stage = progress.get("stage", "pending")
                        current_file = progress.get("current_file")

                        if stage == "indexing" and current_file:
                            yield "event: progress\n"
                            yield "data: {\n"
                            yield '  "stage": "indexing",\n'
                            yield f'  "current": {progress.get("indexed_files", 0) + 1},\n'
                            yield f'  "total": {progress.get("total_files", 0)},\n'
                            yield f'  "filename": "{current_file}"\n'
                            yield "}\n\n"

                        if progress.get("errors"):
                            for error in progress["errors"]:
                                yield "event: error\n"
                                yield f'data: {{"filename": "{error.get("filename", "")}", "error": "{error.get("error", "")}"}}\n\n'

                        if stage == "complete":
                            yield "event: complete\n"
                            yield "data: {\n"
                            yield f'  "total_files": {progress.get("total_files", 0)},\n'
                            yield f'  "successful": {progress.get("indexed_files", 0)},\n'
                            yield f'  "failed": {progress.get("failed_files", 0)}\n'
                            yield "}\n\n"
                            break

                    if current_time - last_keepalive > 15:
                        yield ": keepalive\n\n"
                        last_keepalive = current_time

                except ValueError:
                    yield "event: error\n"
                    yield 'data: {"error": "Task not found"}\n\n'
                    break

                await asyncio.sleep(0.5)

                if current_time - start_time > 300:
                    logger.warning(f"Upload status SSE timeout for task {task_id}")
                    break

        except asyncio.CancelledError:
            logger.info(f"Upload status SSE cancelled for task {task_id}")
        except Exception as e:
            logger.error(f"Error in upload status SSE: {e}", exc_info=True)
            yield "event: error\n"
            yield 'data: {"error": "Internal server error"}\n\n'

    return StreamingResponse(
        event_generator(),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache",
            "Connection": "keep-alive",
            "X-Accel-Buffering": "no",
        },
    )


class DocumentListItem(BaseModel):
    id: str
    filename: str
    path: str
    created_at: int


@router.get("/list", response_model=list[DocumentListItem])
async def list_documents(
    search: str | None = None,
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
) -> list[DocumentListItem]:
    try:
        stmt = select(FileModel).order_by(FileModel.created_at.desc())

        if search and search.strip():
            search_term = f"%{search.strip()}%"
            stmt = stmt.where(FileModel.filename.ilike(search_term))

        result = await db.execute(stmt)
        files = result.scalars().all()

        return [
            DocumentListItem(
                id=str(file.id),
                filename=file.filename,
                path=file.path,
                created_at=int(file.created_at.timestamp()),
            )
            for file in files
        ]

    except Exception as e:
        logger.error(f"Failed to list documents: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail=f"Failed to retrieve documents: {e!s}")
