"""Celery tasks for export operations.

Background tasks for processing large exports and cleanup operations.
Uses Celery for distributed task execution.
"""

import asyncio
import logging

from celery import Celery, Task
from celery.schedules import crontab

from src.config import get_settings
from src.db import get_session_factory
from src.modules.exporter import ExportRequest, ExportService

logger = logging.getLogger(__name__)

settings = get_settings()

# Initialize Celery app
celery_app = Celery(
    "vault_tasks",
    broker=settings.celery_broker_url
    if hasattr(settings, "celery_broker_url")
    else "redis://localhost:6379/0",
    backend=settings.celery_result_backend
    if hasattr(settings, "celery_result_backend")
    else "redis://localhost:6379/0",
)

celery_app.conf.update(
    task_serializer="json",
    accept_content=["json"],
    result_serializer="json",
    timezone="UTC",
    enable_utc=True,
    task_track_started=True,
    task_time_limit=3600,  # 1 hour max
    task_soft_time_limit=3300,  # 55 minutes soft limit
)


class AsyncTask(Task):
    """Base task class that handles async code execution.

    Celery tasks are synchronous by default, but our service code is async.
    This class provides a bridge to run async code in Celery tasks.
    """

    def run_async(self, coro):
        """Run async coroutine in task.

        Args:
            coro: Async coroutine to execute

        Returns:
            Result from coroutine
        """
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        try:
            return loop.run_until_complete(coro)
        finally:
            loop.close()


@celery_app.task(
    bind=True, base=AsyncTask, name="export.process_export", max_retries=3, default_retry_delay=60
)
def process_export_task(self, export_request_dict: dict) -> dict:
    """Process export in background.

    This task is used for large exports that would timeout in a normal
    HTTP request. It processes the export asynchronously and updates
    the job status as it progresses.

    Args:
        export_request_dict: Serialized ExportRequest

    Returns:
        Export result dictionary

    Raises:
        Exception: If export fails after retries

    Example:
        >>> from tasks.export_tasks import process_export_task
        >>> request = ExportRequest(format="json", scope="full")
        >>> task = process_export_task.delay(request.model_dump())
        >>> result = task.get(timeout=3600)
    """
    try:
        logger.info(f"Starting export task: {self.request.id}")

        # Update task state to indicate processing started
        self.update_state(
            state="PROGRESS", meta={"progress": 0, "status": "Initializing export..."}
        )

        async def process():
            """Async function to process export."""
            async with get_session_factory()() as session:
                service = ExportService(session)

                # Reconstruct request from dict
                request = ExportRequest(**export_request_dict)

                # Create and process export
                result = await service.create_export(request)

                # Wait for export to complete (with progress updates)
                while result.status in ["pending", "in_progress"]:
                    await asyncio.sleep(2)
                    result = await service.get_export_status(result.export_id)

                    # Update task progress
                    self.update_state(
                        state="PROGRESS",
                        meta={
                            "progress": result.progress_percent,
                            "status": f"Processing... {result.file_count} files",
                            "file_count": result.file_count,
                            "total_size_bytes": result.total_size_bytes,
                        },
                    )

                return result.model_dump(mode="json")

        result = self.run_async(process())
        logger.info(f"Export task completed: {self.request.id}")
        return result

    except Exception as exc:
        logger.error(f"Export task failed: {exc}", exc_info=True)

        # Retry on failure
        try:
            raise self.retry(exc=exc)
        except self.MaxRetriesExceededError:
            logger.error(f"Max retries exceeded for export task: {self.request.id}")
            return {"status": "failed", "error": str(exc)}


@celery_app.task(bind=True, base=AsyncTask, name="export.cleanup_old_exports")
def cleanup_old_exports_task(self) -> dict:
    """Clean up old export files (scheduled task).

    This task runs on a schedule (configured via Celery beat) to
    automatically clean up export files that have exceeded their
    retention period (default: 24 hours).

    Returns:
        Dictionary with cleanup statistics

    Example:
        This task is typically scheduled rather than called manually:

        celery_app.conf.beat_schedule = {
            'cleanup-exports': {
                'task': 'export.cleanup_old_exports',
                'schedule': crontab(hour=2, minute=0),  # Run daily at 2 AM
            }
        }
    """
    try:
        logger.info("Starting export cleanup task")

        async def cleanup():
            """Async function to clean up exports."""
            async with get_session_factory()() as session:
                service = ExportService(session)
                cleaned_count = await service.cleanup_old_exports()
                return {"cleaned_count": cleaned_count}

        result = self.run_async(cleanup())
        logger.info(f"Export cleanup completed: {result['cleaned_count']} exports cleaned")
        return result

    except Exception as exc:
        logger.error(f"Export cleanup failed: {exc}", exc_info=True)
        return {"status": "failed", "error": str(exc)}


# Celery beat schedule configuration
celery_app.conf.beat_schedule = {
    "cleanup-old-exports": {
        "task": "export.cleanup_old_exports",
        "schedule": crontab(hour=2, minute=0),  # Run daily at 2 AM
    }
}


@celery_app.task(name="export.get_task_status")
def get_task_status(task_id: str) -> dict:
    """Get status of an export task.

    Args:
        task_id: Celery task ID

    Returns:
        Task status information

    Example:
        >>> status = get_task_status.delay("task-uuid-here").get()
        >>> print(status)
        {
            "state": "PROGRESS",
            "progress": 45.0,
            "status": "Processing... 150 files"
        }
    """
    from celery.result import AsyncResult

    task = AsyncResult(task_id, app=celery_app)

    if task.state == "PENDING":
        return {"state": task.state, "status": "Task is waiting to be executed"}
    if task.state == "PROGRESS":
        return {
            "state": task.state,
            "progress": task.info.get("progress", 0),
            "status": task.info.get("status", "Processing..."),
            "file_count": task.info.get("file_count", 0),
            "total_size_bytes": task.info.get("total_size_bytes", 0),
        }
    if task.state == "SUCCESS":
        return {"state": task.state, "result": task.result}
    if task.state == "FAILURE":
        return {
            "state": task.state,
            "status": str(task.info),  # Exception message
        }
    return {"state": task.state, "status": str(task.info)}


if __name__ == "__main__":
    # For testing
    celery_app.worker_main(["worker", "--loglevel=info", "--concurrency=2"])
