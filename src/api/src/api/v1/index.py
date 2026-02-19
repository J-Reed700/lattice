from __future__ import annotations

from datetime import UTC, datetime
import logging
from uuid import UUID

from fastapi import APIRouter, BackgroundTasks, Depends, HTTPException
from opentelemetry import trace
from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from src.db import get_session
from src.models.indexing_job import IndexingJob, JobStatus
from src.observability.metrics import record_error
from src.observability.tracing import get_tracer
from src.schemas.common import SuccessResponse
from src.services.indexing import IndexingService

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/index", tags=["indexing"])
tracer = get_tracer(__name__)


class IndexRequest(BaseModel):
    watch_folder_id: UUID = Field(..., description="Watch folder to index")
    force: bool = Field(default=False, description="Force re-indexing of all files")
    recursive: bool = Field(default=True, description="Index subdirectories recursively")

    class Config:
        json_schema_extra = {
            "example": {
                "watch_folder_id": "660e8400-e29b-41d4-a716-446655440001",
                "force": False,
                "recursive": True,
            }
        }


class IndexStatus(BaseModel):
    job_id: int = Field(..., description="Indexing job ID")
    status: str = Field(..., description="Current indexing status")
    total_files: int = Field(..., description="Total files to index")
    processed_files: int = Field(..., description="Files processed so far")
    failed_files: int = Field(..., description="Files that failed to index")
    start_time: datetime | None = Field(None, description="When indexing started")
    end_time: datetime | None = Field(None, description="When indexing completed")
    current_file: str | None = Field(None, description="Currently processing file")
    error_message: str | None = Field(None, description="Error message if failed")

    class Config:
        json_schema_extra = {
            "example": {
                "job_id": 1,
                "status": "running",
                "total_files": 150,
                "processed_files": 45,
                "failed_files": 2,
                "start_time": "2024-01-15T10:00:00Z",
                "end_time": None,
                "current_file": "/Users/john/Documents/report.pdf",
                "error_message": None,
            }
        }


async def run_indexing(job_id: int, watch_folder_id: UUID, force: bool, recursive: bool):
    """Background task to run indexing."""
    from src.db import get_session

    async for session in get_session():
        try:
            # Get the job
            result = await session.execute(select(IndexingJob).where(IndexingJob.id == job_id))
            job = result.scalar_one_or_none()
            if not job:
                logger.error(f"Indexing job {job_id} not found")
                return

            # Update job status to running
            job.status = JobStatus.RUNNING
            job.start_time = datetime.now(UTC)
            job.end_time = None
            await session.commit()

            indexing_service = IndexingService(session)

            await indexing_service.index_watch_folder(
                watch_folder_id=watch_folder_id, force=force, recursive=recursive
            )

            # Update job status to completed
            result = await session.execute(select(IndexingJob).where(IndexingJob.id == job_id))
            job = result.scalar_one_or_none()
            if job:
                job.status = JobStatus.COMPLETED
                job.end_time = datetime.now(UTC)
                await session.commit()

        except Exception as e:
            logger.error(f"Indexing job {job_id} failed: {e}", exc_info=True)
            # Update job status to failed
            try:
                result = await session.execute(select(IndexingJob).where(IndexingJob.id == job_id))
                job = result.scalar_one_or_none()
                if job:
                    job.status = JobStatus.FAILED
                    job.end_time = datetime.now(UTC)
                    job.error_message = str(e)
                    await session.commit()
            except Exception as commit_error:
                logger.error(f"Failed to update job status: {commit_error}", exc_info=True)
        finally:
            break  # Exit the async generator loop


@router.post("", response_model=SuccessResponse)
async def trigger_indexing(
    request: IndexRequest,
    background_tasks: BackgroundTasks,
    session: AsyncSession = Depends(get_session),
):
    """
    Trigger indexing of a watch folder.

    This operation runs in the background and indexes all files
    in the specified watch folder.

    Use the /index/status/{job_id} endpoint to check progress.
    """
    with tracer.start_as_current_span("api_trigger_indexing") as span:
        span.set_attribute("watch_folder_id", str(request.watch_folder_id))
        span.set_attribute("force", request.force)
        span.set_attribute("recursive", request.recursive)

        try:
            # Check if there's already a running job for this watch folder
            result = await session.execute(
                select(IndexingJob).where(
                    IndexingJob.watch_folder_id == str(request.watch_folder_id),
                    IndexingJob.status.in_([JobStatus.RUNNING, JobStatus.STARTING]),
                )
            )
            running_job = result.scalar_one_or_none()

            if running_job:
                raise HTTPException(
                    status_code=409,
                    detail=f"Indexing is already in progress for this watch folder (job_id: {running_job.id})",
                )

            # Create new indexing job
            job = IndexingJob(
                watch_folder_id=str(request.watch_folder_id),
                status=JobStatus.STARTING,
                total_files=0,
                processed_files=0,
                failed_files=0,
            )
            session.add(job)
            await session.commit()
            await session.refresh(job)

            # Start background task
            background_tasks.add_task(
                run_indexing, job.id, request.watch_folder_id, request.force, request.recursive
            )

            span.set_attribute("success", True)
            span.set_attribute("job_id", job.id)

            return SuccessResponse(
                success=True,
                message="Indexing started",
                data={
                    "job_id": job.id,
                    "watch_folder_id": str(request.watch_folder_id),
                    "force": request.force,
                    "recursive": request.recursive,
                },
            )

        except HTTPException:
            raise
        except Exception as e:
            span.record_exception(e)
            span.set_status(trace.Status(trace.StatusCode.ERROR, str(e)))
            record_error(type(e).__name__, "/index")
            raise


@router.get("/status/{job_id}", response_model=IndexStatus)
async def get_indexing_status(job_id: int, session: AsyncSession = Depends(get_session)):
    """
    Get indexing status for a specific job.

    Returns information about the specified indexing job.
    """
    result = await session.execute(select(IndexingJob).where(IndexingJob.id == job_id))
    job = result.scalar_one_or_none()

    if not job:
        raise HTTPException(status_code=404, detail=f"Indexing job {job_id} not found")

    return IndexStatus(
        job_id=job.id,
        status=job.status.value,
        total_files=job.total_files,
        processed_files=job.processed_files,
        failed_files=job.failed_files,
        start_time=job.start_time,
        end_time=job.end_time,
        current_file=job.current_file,
        error_message=job.error_message,
    )


@router.get("/status", response_model=IndexStatus)
async def get_latest_indexing_status(session: AsyncSession = Depends(get_session)):
    """
    Get the latest indexing status.

    Returns information about the most recent indexing job.
    """
    result = await session.execute(
        select(IndexingJob).order_by(IndexingJob.created_at.desc()).limit(1)
    )
    job = result.scalar_one_or_none()

    if not job:
        raise HTTPException(status_code=404, detail="No indexing jobs found")

    return IndexStatus(
        job_id=job.id,
        status=job.status.value,
        total_files=job.total_files,
        processed_files=job.processed_files,
        failed_files=job.failed_files,
        start_time=job.start_time,
        end_time=job.end_time,
        current_file=job.current_file,
        error_message=job.error_message,
    )


@router.post("/cancel/{job_id}", response_model=SuccessResponse)
async def cancel_indexing(job_id: int, session: AsyncSession = Depends(get_session)):
    """
    Cancel a specific indexing operation.

    Note: This is a graceful cancellation - the current file
    will finish processing before stopping.
    """
    result = await session.execute(select(IndexingJob).where(IndexingJob.id == job_id))
    job = result.scalar_one_or_none()

    if not job:
        raise HTTPException(status_code=404, detail=f"Indexing job {job_id} not found")

    if job.status not in [JobStatus.RUNNING, JobStatus.STARTING]:
        raise HTTPException(
            status_code=400, detail=f"Job {job_id} is not running (status: {job.status.value})"
        )

    job.status = JobStatus.CANCELLING
    await session.commit()

    return SuccessResponse(
        success=True,
        message="Indexing cancellation requested",
        data={"job_id": job_id, "status": "cancelling"},
    )
