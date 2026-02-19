from __future__ import annotations

import asyncio
from collections import defaultdict
from collections.abc import Callable
from datetime import UTC, datetime, timedelta
import logging
from pathlib import Path
from typing import TYPE_CHECKING, ClassVar
import uuid

from src.events.domain.upload_events import (
    BatchUploadCompleted,
    FileUploadCompleted,
    FileUploadFailed,
    FileUploadStarted,
)
from src.services.indexing import IndexingService

if TYPE_CHECKING:
    from src.events.bus import EventBus

logger = logging.getLogger(__name__)


class UploadProgress:
    def __init__(self, task_id: str, total_files: int):
        self.task_id = task_id
        self.total_files = total_files
        self.uploaded_files = 0
        self.indexed_files = 0
        self.failed_files = 0
        self.current_file: str | None = None
        self.stage = "pending"
        self.errors: list[dict] = []
        self.document_ids: dict[str, str] = {}
        self.created_at = datetime.now(UTC)
        self.completed_at: datetime | None = None

    def to_dict(self) -> dict:
        return {
            "task_id": self.task_id,
            "total_files": self.total_files,
            "uploaded_files": self.uploaded_files,
            "indexed_files": self.indexed_files,
            "failed_files": self.failed_files,
            "current_file": self.current_file,
            "stage": self.stage,
            "errors": self.errors,
            "document_ids": self.document_ids,
            "created_at": self.created_at.isoformat(),
            "completed_at": self.completed_at.isoformat() if self.completed_at else None,
        }


class UploadService:
    ALLOWED_EXTENSIONS: ClassVar[set[str]] = {".pdf", ".txt", ".md", ".doc", ".docx", ".html", ".csv", ".json", ".xml"}
    MAX_FILE_SIZE: ClassVar[int] = 50 * 1024 * 1024

    def __init__(self, upload_dir: str, event_bus: EventBus | None = None):
        self.upload_dir = Path(upload_dir)
        self.upload_dir.mkdir(parents=True, exist_ok=True)

        self._tasks: dict[str, UploadProgress] = {}
        self._task_locks: dict[str, asyncio.Lock] = defaultdict(asyncio.Lock)
        self.event_bus = event_bus

        logger.info(f"UploadService initialized with upload_dir: {self.upload_dir}")

    def validate_file(self, filename: str, file_size: int) -> None:
        file_ext = Path(filename).suffix.lower()

        if file_ext not in self.ALLOWED_EXTENSIONS:
            raise ValueError(
                f"Unsupported file type '{file_ext}'. Allowed: {', '.join(self.ALLOWED_EXTENSIONS)}"
            )

        if file_size > self.MAX_FILE_SIZE:
            raise ValueError(
                f"File size {file_size} exceeds maximum of {self.MAX_FILE_SIZE / 1024 / 1024}MB"
            )

        if file_size == 0:
            raise ValueError("File is empty")

    async def save_upload(
        self, filename: str, content: bytes, user_id: int = 0, file_id: str | None = None
    ) -> Path:
        timestamp = datetime.now(UTC).strftime("%Y%m%d_%H%M%S")
        unique_id = str(uuid.uuid4())[:8]
        safe_filename = Path(filename).name
        new_filename = f"{timestamp}_{unique_id}_{safe_filename}"

        file_path = self.upload_dir / new_filename

        # Use provided file_id or generate new one
        actual_file_id = file_id or str(uuid.uuid4())

        # Emit FileUploadStarted event
        if self.event_bus:
            event = FileUploadStarted(
                file_id=actual_file_id,
                filename=safe_filename,
                size_bytes=len(content),
                user_id=user_id,
            )
            await self.event_bus.publish(event)

        start_time = datetime.now(UTC)
        try:
            with open(file_path, "wb") as f:
                f.write(content)

            duration = (datetime.now(UTC) - start_time).total_seconds()

            # Emit FileUploadCompleted event
            if self.event_bus:
                event = FileUploadCompleted(
                    file_id=actual_file_id,
                    filename=safe_filename,
                    size_bytes=len(content),
                    storage_path=str(file_path),
                    user_id=user_id,
                    duration_seconds=duration,
                )
                await self.event_bus.publish(event)

            logger.info(f"Saved upload to {file_path}")
            return file_path

        except Exception as e:
            # Emit FileUploadFailed event
            if self.event_bus:
                event = FileUploadFailed(
                    file_id=actual_file_id,
                    filename=safe_filename,
                    error_message=str(e),
                    error_type=type(e).__name__,
                    user_id=user_id,
                )
                await self.event_bus.publish(event)
            raise

    def create_task(self, file_count: int) -> str:
        task_id = str(uuid.uuid4())
        progress = UploadProgress(task_id, file_count)
        self._tasks[task_id] = progress
        logger.info(f"Created upload task {task_id} for {file_count} files")
        return task_id

    async def update_progress(
        self,
        task_id: str,
        stage: str | None = None,
        current_file: str | None = None,
        uploaded: bool = False,
        indexed: bool = False,
        failed: bool = False,
        error: dict | None = None,
        document_id: str | None = None,
    ):
        async with self._task_locks[task_id]:
            if task_id not in self._tasks:
                logger.warning(f"Task {task_id} not found for progress update")
                return

            progress = self._tasks[task_id]

            if stage:
                progress.stage = stage
            if current_file:
                progress.current_file = current_file
            if uploaded:
                progress.uploaded_files += 1
            if indexed:
                progress.indexed_files += 1
            if failed:
                progress.failed_files += 1
            if error:
                progress.errors.append(error)
            if document_id and current_file:
                progress.document_ids[current_file] = document_id

    async def process_uploads(
        self,
        task_id: str,
        file_paths: list[Path],
        indexing_service: IndexingService,
        callback: Callable | None = None,
        user_id: int = 0,
    ):
        progress = self._tasks.get(task_id)
        if not progress:
            logger.error(f"Task {task_id} not found")
            return

        batch_start_time = datetime.now(UTC)
        successful_count = 0
        failed_count = 0

        try:
            await self.update_progress(task_id, stage="uploading")

            for file_path in file_paths:
                filename = file_path.name

                try:
                    await self.update_progress(task_id, stage="indexing", current_file=filename)

                    if callback:
                        await callback(
                            {
                                "event": "progress",
                                "data": {
                                    "stage": "indexing",
                                    "current": progress.indexed_files + 1,
                                    "total": progress.total_files,
                                    "filename": filename,
                                },
                            }
                        )

                    # Progress events removed - batch already has start/complete events
                    # Individual file uploads have their own FileUploadStarted/Completed events

                    success, error_msg = await indexing_service.index_file(str(file_path))

                    if success:
                        successful_count += 1
                        await self.update_progress(task_id, indexed=True)

                        if callback:
                            await callback(
                                {
                                    "event": "indexed",
                                    "data": {
                                        "filename": filename,
                                        "document_id": str(uuid.uuid4()),
                                    },
                                }
                            )
                    else:
                        failed_count += 1
                        await self.update_progress(
                            task_id,
                            failed=True,
                            error={"filename": filename, "error": error_msg or "Unknown error"},
                        )

                        if callback:
                            await callback(
                                {
                                    "event": "error",
                                    "data": {
                                        "filename": filename,
                                        "error": error_msg or "Failed to index file",
                                    },
                                }
                            )

                except Exception as e:
                    logger.error(f"Error processing file {filename}: {e}", exc_info=True)
                    failed_count += 1
                    await self.update_progress(
                        task_id, failed=True, error={"filename": filename, "error": str(e)}
                    )

                    if callback:
                        await callback(
                            {"event": "error", "data": {"filename": filename, "error": str(e)}}
                        )

            await self.update_progress(task_id, stage="complete")
            progress.completed_at = datetime.now(UTC)

            # Emit BatchUploadCompleted event
            if self.event_bus:
                batch_duration = (datetime.now(UTC) - batch_start_time).total_seconds()
                batch_event = BatchUploadCompleted(
                    batch_id=task_id,
                    total_files=progress.total_files,
                    successful_uploads=successful_count,
                    failed_uploads=failed_count,
                    user_id=user_id,
                    duration_seconds=batch_duration,
                )
                await self.event_bus.publish(batch_event)

            if callback:
                await callback(
                    {
                        "event": "complete",
                        "data": {
                            "total_files": progress.total_files,
                            "successful": progress.indexed_files,
                            "failed": progress.failed_files,
                        },
                    }
                )

        except Exception as e:
            logger.error(f"Error in upload task {task_id}: {e}", exc_info=True)
            await self.update_progress(task_id, stage="failed")
            progress.completed_at = datetime.now(UTC)

    def get_progress(self, task_id: str) -> dict:
        if task_id not in self._tasks:
            raise ValueError(f"Unknown task ID: {task_id}")

        return self._tasks[task_id].to_dict()

    def cleanup_old_tasks(self, max_age_hours: int = 1):
        now = datetime.now(UTC)
        to_remove = []

        for task_id, progress in self._tasks.items():
            if progress.completed_at:
                age = now - progress.completed_at
                if age > timedelta(hours=max_age_hours):
                    to_remove.append(task_id)

        for task_id in to_remove:
            logger.info(f"Removing old task: {task_id}")
            del self._tasks[task_id]
            if task_id in self._task_locks:
                del self._task_locks[task_id]

        return len(to_remove)

    def get_all_tasks(self) -> dict[str, dict]:
        return {task_id: progress.to_dict() for task_id, progress in self._tasks.items()}
