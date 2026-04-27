"""Export service implementation.

Main service class that coordinates export operations, manages export jobs,
and interfaces with format handlers.
"""

import asyncio
from datetime import UTC, datetime, timedelta
import logging
from pathlib import Path
import tempfile
from uuid import UUID, uuid4

logger = logging.getLogger(__name__)


def _log_task_exception(task: asyncio.Task) -> None:
    if not task.cancelled() and (exc := task.exception()):
        logger.error(f"Background task failed: {exc}", exc_info=exc)


from sqlalchemy import and_, select
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy.orm import selectinload

from src.models import File, FileTag

from .handlers import CSVExportHandler, JSONExportHandler, MarkdownExportHandler, ZIPExportHandler
from .types import (
    ExportFilters,
    ExportFormat,
    ExportJob,
    ExportMetadata,
    ExportRequest,
    ExportResult,
    ExportScope,
    ExportStatus,
    FileExportData,
)


class ExportService:
    """Service for managing data exports.

    Coordinates export operations including:
    - Querying database for files to export
    - Generating exports in various formats
    - Tracking export job progress
    - Managing temporary files and cleanup

    Example:
        >>> async with get_session() as session:
        ...     service = ExportService(session, export_dir="/tmp/exports")
        ...     request = ExportRequest(
        ...         format=ExportFormat.JSON,
        ...         scope=ExportScope.FULL
        ...     )
        ...     result = await service.create_export(request)
    """

    def __init__(self, session: AsyncSession, export_dir: str | None = None):
        """Initialize export service.

        Args:
            session: Database session
            export_dir: Directory for export files (default: temp directory)
        """
        self.session = session
        self.export_dir = (
            Path(export_dir) if export_dir else Path(tempfile.gettempdir()) / "vault_exports"
        )
        self.export_dir.mkdir(parents=True, exist_ok=True)

        # In-memory job tracking (should use Redis/database in production)
        self._jobs: dict[UUID, ExportJob] = {}
        self._background_tasks: set[asyncio.Task] = set()

    async def create_export(self, request: ExportRequest) -> ExportResult:
        """Create a new export job.

        Args:
            request: Export request with format and scope

        Returns:
            Export result with job ID and initial status

        Raises:
            ValueError: If request is invalid

        Example:
            >>> request = ExportRequest(
            ...     format=ExportFormat.JSON,
            ...     scope=ExportScope.FULL
            ... )
            >>> result = await service.create_export(request)
            >>> print(f"Export job created: {result.export_id}")
        """
        # Validate request
        self._validate_request(request)

        # Create export job
        export_id = uuid4()
        result = ExportResult(
            export_id=export_id,
            status=ExportStatus.PENDING,
            format=request.format,
            scope=request.scope,
            created_at=datetime.now(UTC),
        )

        job = ExportJob(
            id=export_id,
            request=request,
            result=result,
            cleanup_at=datetime.now(UTC) + timedelta(hours=24),
        )

        self._jobs[export_id] = job

        # Start export in background
        task = asyncio.create_task(self._process_export(job))
        self._background_tasks.add(task)
        task.add_done_callback(self._background_tasks.discard)
        task.add_done_callback(_log_task_exception)

        return result

    async def get_export_status(self, export_id: UUID) -> ExportResult | None:
        """Get status of an export job.

        Args:
            export_id: Export job ID

        Returns:
            Export result or None if not found

        Example:
            >>> result = await service.get_export_status(export_id)
            >>> print(f"Status: {result.status}, Progress: {result.progress_percent}%")
        """
        job = self._jobs.get(export_id)
        return job.result if job else None

    async def cancel_export(self, export_id: UUID) -> bool:
        """Cancel a pending or in-progress export.

        Args:
            export_id: Export job ID

        Returns:
            True if cancelled, False if not found or already completed

        Example:
            >>> cancelled = await service.cancel_export(export_id)
        """
        job = self._jobs.get(export_id)
        if not job:
            return False

        if job.result.status in [ExportStatus.PENDING, ExportStatus.IN_PROGRESS]:
            job.result.status = ExportStatus.CANCELLED
            job.result.completed_at = datetime.now(UTC)
            return True

        return False

    async def cleanup_old_exports(self) -> int:
        """Clean up old export files past their cleanup time.

        Returns:
            Number of exports cleaned up

        Example:
            >>> cleaned = await service.cleanup_old_exports()
            >>> print(f"Cleaned up {cleaned} old exports")
        """
        now = datetime.now(UTC)
        cleaned_count = 0

        for export_id, job in list(self._jobs.items()):
            if now >= job.cleanup_at:
                # Delete export file
                if job.result.output_path:
                    try:
                        path = Path(job.result.output_path)
                        if path.exists():
                            if path.is_file():
                                path.unlink()
                            else:
                                import shutil

                                shutil.rmtree(path)
                        cleaned_count += 1
                    except Exception as e:
                        logger.warning(
                            f"Failed to delete export file {job.result.output_path}: {e}"
                        )

                # Remove job from tracking
                del self._jobs[export_id]

        return cleaned_count

    async def _process_export(self, job: ExportJob):
        """Process an export job (background task).

        Args:
            job: Export job to process
        """
        try:
            job.result.status = ExportStatus.IN_PROGRESS
            job.result.progress_percent = 0.0

            # Fetch files based on scope and filters
            files = await self._fetch_files(job.request)

            job.result.file_count = len(files)
            job.result.progress_percent = 20.0

            # Convert to export data format
            export_data = await self._prepare_export_data(files, job.request.include_embeddings)

            job.result.progress_percent = 50.0

            # Calculate total size
            total_size = sum(f.size_bytes for f in export_data)
            job.result.total_size_bytes = total_size

            # Create metadata
            metadata = ExportMetadata(
                exported_at=datetime.now(UTC),
                total_files=len(export_data),
                total_size_bytes=total_size,
                export_format=job.request.format,
                filters_applied=self._get_filters_description(job.request),
            )

            job.result.progress_percent = 70.0

            # Generate export using appropriate handler
            output_path = await self._generate_export(job.request, export_data, metadata)

            job.result.output_path = output_path
            job.result.progress_percent = 100.0
            job.result.status = ExportStatus.COMPLETED
            job.result.completed_at = datetime.now(UTC)

        except Exception as e:
            job.result.status = ExportStatus.FAILED
            job.result.error_message = str(e)
            job.result.completed_at = datetime.now(UTC)

    async def _fetch_files(self, request: ExportRequest) -> list[File]:
        """Fetch files from database based on request scope and filters.

        Args:
            request: Export request

        Returns:
            List of File models
        """
        query = select(File).options(
            selectinload(File.text_content), selectinload(File.file_tags).selectinload(FileTag.tag)
        )

        # Apply scope filters
        if request.scope == ExportScope.SELECTED:
            if not request.file_ids:
                return []
            query = query.where(File.id.in_(request.file_ids))

        elif request.scope == ExportScope.FILTERED:
            query = self._apply_filters(query, request.filters)

        # Execute query
        result = await self.session.execute(query)
        return list(result.scalars().all())

    def _apply_filters(self, query, filters: ExportFilters | None):
        """Apply filters to query.

        Args:
            query: SQLAlchemy query
            filters: Export filters

        Returns:
            Modified query
        """
        if not filters:
            return query

        conditions = []

        if filters.date_from:
            conditions.append(File.modified_at >= filters.date_from)

        if filters.date_to:
            conditions.append(File.modified_at <= filters.date_to)

        if filters.file_types:
            conditions.append(File.extension.in_(filters.file_types))

        if filters.folder_ids:
            conditions.append(File.watch_folder_id.in_(filters.folder_ids))

        if conditions:
            query = query.where(and_(*conditions))

        # Note: Tag filtering and search query filtering would require joins
        # Simplified here for brevity

        return query

    async def _prepare_export_data(
        self, files: list[File], include_embeddings: bool
    ) -> list[FileExportData]:
        """Convert File models to FileExportData.

        Args:
            files: List of File models
            include_embeddings: Whether to include embeddings

        Returns:
            List of FileExportData
        """
        export_data = []

        for file in files:
            # Extract tags
            tags = [ft.tag.name for ft in file.file_tags] if file.file_tags else []

            # Get text content
            text_content = None
            word_count = None
            char_count = None
            language = None

            if file.text_content:
                text_content = file.text_content.content
                word_count = file.text_content.word_count
                char_count = file.text_content.char_count
                language = file.text_content.language

            # TODO: Fetch embeddings if requested
            embeddings = None

            file_data = FileExportData(
                id=file.id,
                path=file.path,
                filename=file.filename,
                extension=file.extension,
                mime_type=file.mime_type,
                size_bytes=file.size_bytes,
                hash_sha256=file.hash_sha256,
                modified_at=file.modified_at,
                indexed_at=file.indexed_at,
                text_content=text_content,
                word_count=word_count,
                char_count=char_count,
                language=language,
                tags=tags,
                embeddings=embeddings,
            )

            export_data.append(file_data)

        return export_data

    async def _generate_export(
        self, request: ExportRequest, files: list[FileExportData], metadata: ExportMetadata
    ) -> str:
        """Generate export file using appropriate handler.

        Args:
            request: Export request
            files: File data to export
            metadata: Export metadata

        Returns:
            Path to generated export file
        """
        export_id = uuid4()
        base_path = self.export_dir / f"export_{export_id}"

        if request.format == ExportFormat.JSON:
            handler = JSONExportHandler(str(base_path), compress=request.compress)
            return await handler.generate(files, metadata)

        if request.format == ExportFormat.CSV:
            handler = CSVExportHandler(str(base_path), compress=request.compress)
            return await handler.generate(files, metadata)

        if request.format == ExportFormat.MARKDOWN:
            handler = MarkdownExportHandler(str(base_path), compress=request.compress)
            return await handler.generate(files, metadata)

        if request.format == ExportFormat.ZIP:
            handler = ZIPExportHandler(str(base_path), compress=False)
            return await handler.generate(
                files, metadata, include_original_files=request.include_original_files
            )

        raise ValueError(f"Unsupported export format: {request.format}")

    def _validate_request(self, request: ExportRequest):
        """Validate export request.

        Args:
            request: Export request to validate

        Raises:
            ValueError: If request is invalid
        """
        if request.scope == ExportScope.SELECTED:
            if not request.file_ids or len(request.file_ids) == 0:
                raise ValueError("file_ids required for selected scope")

        if request.scope == ExportScope.FILTERED:
            if not request.filters:
                raise ValueError("filters required for filtered scope")

    def _get_filters_description(self, request: ExportRequest) -> dict | None:
        """Get human-readable description of filters applied.

        Args:
            request: Export request

        Returns:
            Dictionary describing filters or None
        """
        if request.scope == ExportScope.FULL:
            return {"scope": "full"}

        if request.scope == ExportScope.SELECTED:
            return {
                "scope": "selected",
                "file_count": len(request.file_ids) if request.file_ids else 0,
            }

        if request.scope == ExportScope.FILTERED and request.filters:
            desc = {"scope": "filtered"}

            if request.filters.date_from:
                desc["date_from"] = request.filters.date_from.isoformat()
            if request.filters.date_to:
                desc["date_to"] = request.filters.date_to.isoformat()
            if request.filters.file_types:
                desc["file_types"] = request.filters.file_types
            if request.filters.tags:
                desc["tags"] = request.filters.tags

            return desc

        return None
