"""Export API endpoints.

FastAPI routes for data export functionality including:
- Creating exports in various formats
- Checking export status
- Downloading completed exports
- Cancelling exports
- Cleaning up old exports
"""

from __future__ import annotations

from pathlib import Path
from uuid import UUID

from fastapi import APIRouter, BackgroundTasks, Depends, HTTPException, status
from fastapi.responses import FileResponse
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.errors import NotFoundError, ValidationError
from src.auth.dependencies import get_current_active_user
from src.auth.models import User
from src.db import get_session
from src.middleware.csrf import csrf_protect
from src.modules.exporter import (
    ExportFormat,
    ExportRequest,
    ExportResult,
    ExportScope,
    ExportService,
    ExportStatus,
)
from src.utils.security import SecurityError, validate_safe_path

router = APIRouter(prefix="/export", tags=["export"])


@router.post(
    "/full",
    response_model=ExportResult,
    status_code=status.HTTP_202_ACCEPTED,
    summary="Export full knowledge base",
    description="Create export of entire knowledge base in specified format",
)
async def export_full(
    format: ExportFormat,
    include_embeddings: bool = False,
    include_original_files: bool = True,
    compress: bool = True,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    session: AsyncSession = Depends(get_session),
) -> ExportResult:
    """Export full knowledge base.

    Creates an export job for the entire knowledge base in the specified format.
    The export runs in the background and can be downloaded when complete.

    Args:
        format: Export format (json, csv, markdown, zip)
        include_embeddings: Include vector embeddings in export
        include_original_files: Include original files (ZIP format only)
        compress: Compress output file
        session: Database session

    Returns:
        Export result with job ID and initial status

    Example:
        POST /api/v1/export/full?format=json&compress=true

        Response:
        {
            "export_id": "123e4567-e89b-12d3-a456-426614174000",
            "status": "pending",
            "format": "json",
            "scope": "full",
            "file_count": 0,
            "progress_percent": 0.0,
            "created_at": "2024-01-01T00:00:00Z"
        }
    """
    service = ExportService(session)

    request = ExportRequest(
        format=format,
        scope=ExportScope.FULL,
        include_embeddings=include_embeddings,
        include_original_files=include_original_files,
        compress=compress,
    )

    result = await service.create_export(request)
    return result


@router.post(
    "/filtered",
    response_model=ExportResult,
    status_code=status.HTTP_202_ACCEPTED,
    summary="Export filtered data",
    description="Create export with filters applied (date range, file types, tags, etc.)",
)
async def export_filtered(
    request: ExportRequest,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    session: AsyncSession = Depends(get_session),
) -> ExportResult:
    """Export filtered knowledge base.

    Creates an export job with filters applied. Use this endpoint for
    selective exports based on date range, file types, tags, or folders.

    Args:
        request: Export request with filters
        session: Database session

    Returns:
        Export result with job ID and initial status

    Raises:
        ValidationError: If request validation fails

    Example:
        POST /api/v1/export/filtered

        Body:
        {
            "format": "csv",
            "scope": "filtered",
            "filters": {
                "date_from": "2024-01-01T00:00:00Z",
                "date_to": "2024-12-31T23:59:59Z",
                "file_types": ["pdf", "docx"],
                "tags": ["important", "work"]
            },
            "compress": true
        }
    """
    if request.scope != ExportScope.FILTERED:
        raise ValidationError("Scope must be 'filtered' for this endpoint")

    if not request.filters:
        raise ValidationError("Filters required for filtered export")

    service = ExportService(session)
    result = await service.create_export(request)
    return result


@router.post(
    "/selected",
    response_model=ExportResult,
    status_code=status.HTTP_202_ACCEPTED,
    summary="Export selected files",
    description="Create export of specific user-selected files",
)
async def export_selected(
    file_ids: list[UUID],
    format: ExportFormat,
    include_embeddings: bool = False,
    include_original_files: bool = True,
    compress: bool = True,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    session: AsyncSession = Depends(get_session),
) -> ExportResult:
    """Export selected files.

    Creates an export job for specific user-selected files identified by IDs.

    Args:
        file_ids: List of file UUIDs to export
        format: Export format
        include_embeddings: Include vector embeddings
        include_original_files: Include original files (ZIP only)
        compress: Compress output
        session: Database session

    Returns:
        Export result with job ID and initial status

    Raises:
        ValidationError: If file_ids is empty

    Example:
        POST /api/v1/export/selected

        Body:
        {
            "file_ids": [
                "123e4567-e89b-12d3-a456-426614174000",
                "987e6543-e21b-98d7-b654-321654987000"
            ],
            "format": "zip",
            "include_original_files": true
        }
    """
    if not file_ids or len(file_ids) == 0:
        raise ValidationError("file_ids cannot be empty")

    service = ExportService(session)

    request = ExportRequest(
        format=format,
        scope=ExportScope.SELECTED,
        file_ids=file_ids,
        include_embeddings=include_embeddings,
        include_original_files=include_original_files,
        compress=compress,
    )

    result = await service.create_export(request)
    return result


@router.get(
    "/{export_id}/status",
    response_model=ExportResult,
    summary="Get export status",
    description="Check status and progress of an export job",
)
async def get_export_status(
    export_id: UUID,
    current_user: User = Depends(get_current_active_user),
    session: AsyncSession = Depends(get_session),
) -> ExportResult:
    """Get export status.

    Retrieve current status and progress of an export job.

    Args:
        export_id: Export job UUID
        session: Database session

    Returns:
        Export result with current status and progress

    Raises:
        NotFoundError: If export job not found

    Example:
        GET /api/v1/export/123e4567-e89b-12d3-a456-426614174000/status

        Response:
        {
            "export_id": "123e4567-e89b-12d3-a456-426614174000",
            "status": "in_progress",
            "format": "json",
            "scope": "full",
            "file_count": 150,
            "total_size_bytes": 10485760,
            "progress_percent": 45.0,
            "estimated_time_remaining": 120
        }
    """
    service = ExportService(session)
    result = await service.get_export_status(export_id)

    if not result:
        raise NotFoundError(f"Export {export_id} not found")

    return result


@router.get(
    "/{export_id}/download",
    response_class=FileResponse,
    summary="Download export file",
    description="Download completed export file",
)
async def download_export(
    export_id: UUID,
    current_user: User = Depends(get_current_active_user),
    session: AsyncSession = Depends(get_session),
) -> FileResponse:
    """Download export file.

    Download the completed export file. Export must be in 'completed' status.

    Args:
        export_id: Export job UUID
        session: Database session

    Returns:
        Export file for download

    Raises:
        NotFoundError: If export not found
        ValidationError: If export not completed or file not found

    Example:
        GET /api/v1/export/123e4567-e89b-12d3-a456-426614174000/download

        Returns the export file with appropriate Content-Type and filename
    """
    service = ExportService(session)
    result = await service.get_export_status(export_id)

    if not result:
        raise NotFoundError(f"Export {export_id} not found")

    if result.status != ExportStatus.COMPLETED:
        raise ValidationError(f"Export status is {result.status}, not completed")

    if not result.output_path:
        raise ValidationError("Export file not found")

    file_path = Path(result.output_path)
    if not file_path.exists():
        raise ValidationError("Export file no longer exists")

    # Determine media type and filename
    media_type_map = {
        ExportFormat.JSON: "application/json",
        ExportFormat.CSV: "text/csv",
        ExportFormat.MARKDOWN: "application/zip",
        ExportFormat.ZIP: "application/zip",
    }

    media_type = media_type_map.get(result.format, "application/octet-stream")
    filename = f"vault_export_{export_id}{file_path.suffix}"

    # Validate path to prevent traversal attacks
    try:
        import tempfile

        temp_dir = Path(tempfile.gettempdir())
        safe_path = validate_safe_path(file_path, temp_dir, allow_symlinks=False, must_exist=True)
    except SecurityError as e:
        raise HTTPException(status_code=403, detail=str(e))

    return FileResponse(path=str(safe_path), media_type=media_type, filename=filename)


@router.delete(
    "/{export_id}",
    status_code=status.HTTP_204_NO_CONTENT,
    summary="Cancel export",
    description="Cancel a pending or in-progress export job",
)
async def cancel_export(
    export_id: UUID,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    session: AsyncSession = Depends(get_session),
):
    """Cancel export job.

    Cancel a pending or in-progress export. Completed exports cannot be cancelled.

    Args:
        export_id: Export job UUID
        session: Database session

    Raises:
        NotFoundError: If export not found
        ValidationError: If export already completed

    Example:
        DELETE /api/v1/export/123e4567-e89b-12d3-a456-426614174000
    """
    service = ExportService(session)
    cancelled = await service.cancel_export(export_id)

    if not cancelled:
        result = await service.get_export_status(export_id)
        if not result:
            raise NotFoundError(f"Export {export_id} not found")
        raise ValidationError(f"Cannot cancel export with status: {result.status}")


@router.post(
    "/cleanup",
    status_code=status.HTTP_200_OK,
    summary="Cleanup old exports",
    description="Remove old export files past their retention period (24 hours)",
)
async def cleanup_exports(
    background_tasks: BackgroundTasks,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    session: AsyncSession = Depends(get_session),
) -> dict:
    """Cleanup old exports.

    Remove export files that have exceeded their retention period (24 hours).
    Runs in background to avoid blocking the request.

    Args:
        background_tasks: FastAPI background tasks
        session: Database session

    Returns:
        Message indicating cleanup started

    Example:
        POST /api/v1/export/cleanup

        Response:
        {
            "message": "Cleanup started in background"
        }
    """

    async def cleanup_task():
        service = ExportService(session)
        cleaned = await service.cleanup_old_exports()
        # Log cleanup results
        import logging

        logger = logging.getLogger(__name__)
        logger.info(f"Cleaned up {cleaned} old exports")

    background_tasks.add_task(cleanup_task)

    return {"message": "Cleanup started in background"}
