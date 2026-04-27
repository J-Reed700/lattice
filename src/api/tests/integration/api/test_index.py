from unittest.mock import patch

from httpx import AsyncClient
import pytest
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from src.models import WatchFolder
from src.models.indexing_job import IndexingJob, JobStatus


class TestIndexTriggerEndpoint:
    @pytest.mark.asyncio()
    async def test_trigger_indexing_success(
        self, client: AsyncClient, watch_folder: WatchFolder, db_session: AsyncSession
    ):
        response = await client.post(
            "/api/v1/index",
            json={"watch_folder_id": str(watch_folder.id), "force": False, "recursive": True},
        )

        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert "job_id" in data["data"]
        assert data["data"]["watch_folder_id"] == str(watch_folder.id)
        assert data["data"]["force"] is False
        assert data["data"]["recursive"] is True

        result = await db_session.execute(
            select(IndexingJob).where(IndexingJob.id == data["data"]["job_id"])
        )
        job = result.scalar_one_or_none()
        assert job is not None
        assert job.status in [JobStatus.STARTING, JobStatus.RUNNING]

    @pytest.mark.asyncio()
    async def test_trigger_indexing_with_force(
        self, client: AsyncClient, watch_folder: WatchFolder
    ):
        response = await client.post(
            "/api/v1/index",
            json={"watch_folder_id": str(watch_folder.id), "force": True, "recursive": False},
        )

        assert response.status_code == 200
        data = response.json()
        assert data["data"]["force"] is True
        assert data["data"]["recursive"] is False

    @pytest.mark.asyncio()
    async def test_trigger_indexing_duplicate_job_conflict(
        self, client: AsyncClient, watch_folder: WatchFolder, db_session: AsyncSession
    ):
        job = IndexingJob(
            watch_folder_id=str(watch_folder.id),
            status=JobStatus.RUNNING,
            total_files=0,
            processed_files=0,
            failed_files=0,
        )
        db_session.add(job)
        await db_session.commit()

        response = await client.post(
            "/api/v1/index",
            json={"watch_folder_id": str(watch_folder.id), "force": False, "recursive": True},
        )

        assert response.status_code == 409
        data = response.json()
        assert "already in progress" in data["detail"]

    @pytest.mark.asyncio()
    async def test_trigger_indexing_validation_missing_watch_folder_id(self, client: AsyncClient):
        response = await client.post("/api/v1/index", json={"force": False, "recursive": True})

        assert response.status_code == 422

    @pytest.mark.asyncio()
    async def test_trigger_indexing_validation_invalid_uuid(self, client: AsyncClient):
        response = await client.post(
            "/api/v1/index",
            json={"watch_folder_id": "invalid-uuid", "force": False, "recursive": True},
        )

        assert response.status_code == 422

    @pytest.mark.asyncio()
    async def test_trigger_indexing_default_values(
        self, client: AsyncClient, watch_folder: WatchFolder
    ):
        response = await client.post(
            "/api/v1/index", json={"watch_folder_id": str(watch_folder.id)}
        )

        assert response.status_code == 200
        data = response.json()
        assert data["data"]["force"] is False
        assert data["data"]["recursive"] is True


class TestIndexStatusByIdEndpoint:
    @pytest.mark.asyncio()
    async def test_get_status_success(
        self, client: AsyncClient, db_session: AsyncSession, watch_folder: WatchFolder
    ):
        job = IndexingJob(
            watch_folder_id=str(watch_folder.id),
            status=JobStatus.RUNNING,
            total_files=100,
            processed_files=45,
            failed_files=2,
        )
        db_session.add(job)
        await db_session.commit()
        await db_session.refresh(job)

        response = await client.get(f"/api/v1/index/status/{job.id}")

        assert response.status_code == 200
        data = response.json()
        assert data["job_id"] == job.id
        assert data["status"] == JobStatus.RUNNING.value
        assert data["total_files"] == 100
        assert data["processed_files"] == 45
        assert data["failed_files"] == 2

    @pytest.mark.asyncio()
    async def test_get_status_not_found(self, client: AsyncClient):
        response = await client.get("/api/v1/index/status/99999")

        assert response.status_code == 404
        data = response.json()
        assert "not found" in data["detail"]

    @pytest.mark.asyncio()
    async def test_get_status_completed_job(
        self, client: AsyncClient, db_session: AsyncSession, watch_folder: WatchFolder
    ):
        from datetime import datetime

        job = IndexingJob(
            watch_folder_id=str(watch_folder.id),
            status=JobStatus.COMPLETED,
            total_files=50,
            processed_files=50,
            failed_files=0,
            start_time=datetime.utcnow(),
            end_time=datetime.utcnow(),
        )
        db_session.add(job)
        await db_session.commit()
        await db_session.refresh(job)

        response = await client.get(f"/api/v1/index/status/{job.id}")

        assert response.status_code == 200
        data = response.json()
        assert data["status"] == JobStatus.COMPLETED.value
        assert data["start_time"] is not None
        assert data["end_time"] is not None

    @pytest.mark.asyncio()
    async def test_get_status_failed_job(
        self, client: AsyncClient, db_session: AsyncSession, watch_folder: WatchFolder
    ):
        job = IndexingJob(
            watch_folder_id=str(watch_folder.id),
            status=JobStatus.FAILED,
            total_files=10,
            processed_files=5,
            failed_files=5,
            error_message="Indexing failed due to disk error",
        )
        db_session.add(job)
        await db_session.commit()
        await db_session.refresh(job)

        response = await client.get(f"/api/v1/index/status/{job.id}")

        assert response.status_code == 200
        data = response.json()
        assert data["status"] == JobStatus.FAILED.value
        assert data["error_message"] is not None
        assert "disk error" in data["error_message"]


class TestIndexLatestStatusEndpoint:
    @pytest.mark.asyncio()
    async def test_get_latest_status_success(
        self, client: AsyncClient, db_session: AsyncSession, watch_folder: WatchFolder
    ):
        for i in range(3):
            job = IndexingJob(
                watch_folder_id=str(watch_folder.id),
                status=JobStatus.COMPLETED,
                total_files=10 * (i + 1),
                processed_files=10 * (i + 1),
                failed_files=0,
            )
            db_session.add(job)

        await db_session.commit()

        response = await client.get("/api/v1/index/status")

        assert response.status_code == 200
        data = response.json()
        assert "job_id" in data
        assert "status" in data
        assert data["total_files"] == 30

    @pytest.mark.asyncio()
    async def test_get_latest_status_no_jobs(self, client: AsyncClient):
        response = await client.get("/api/v1/index/status")

        assert response.status_code == 404
        data = response.json()
        assert "No indexing jobs found" in data["detail"]

    @pytest.mark.asyncio()
    async def test_get_latest_status_returns_most_recent(
        self, client: AsyncClient, db_session: AsyncSession, watch_folder: WatchFolder
    ):
        import asyncio

        job1 = IndexingJob(
            watch_folder_id=str(watch_folder.id),
            status=JobStatus.COMPLETED,
            total_files=10,
            processed_files=10,
            failed_files=0,
        )
        db_session.add(job1)
        await db_session.commit()

        await asyncio.sleep(0.01)

        job2 = IndexingJob(
            watch_folder_id=str(watch_folder.id),
            status=JobStatus.RUNNING,
            total_files=20,
            processed_files=5,
            failed_files=0,
        )
        db_session.add(job2)
        await db_session.commit()
        await db_session.refresh(job2)

        response = await client.get("/api/v1/index/status")

        assert response.status_code == 200
        data = response.json()
        assert data["job_id"] == job2.id
        assert data["total_files"] == 20


class TestIndexCancelEndpoint:
    @pytest.mark.asyncio()
    async def test_cancel_indexing_success(
        self, client: AsyncClient, db_session: AsyncSession, watch_folder: WatchFolder
    ):
        job = IndexingJob(
            watch_folder_id=str(watch_folder.id),
            status=JobStatus.RUNNING,
            total_files=100,
            processed_files=10,
            failed_files=0,
        )
        db_session.add(job)
        await db_session.commit()
        await db_session.refresh(job)

        response = await client.post(f"/api/v1/index/cancel/{job.id}")

        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert data["data"]["job_id"] == job.id
        assert data["data"]["status"] == "cancelling"

        await db_session.refresh(job)
        assert job.status == JobStatus.CANCELLING

    @pytest.mark.asyncio()
    async def test_cancel_indexing_not_found(self, client: AsyncClient):
        response = await client.post("/api/v1/index/cancel/99999")

        assert response.status_code == 404
        data = response.json()
        assert "not found" in data["detail"]

    @pytest.mark.asyncio()
    async def test_cancel_indexing_not_running(
        self, client: AsyncClient, db_session: AsyncSession, watch_folder: WatchFolder
    ):
        job = IndexingJob(
            watch_folder_id=str(watch_folder.id),
            status=JobStatus.COMPLETED,
            total_files=50,
            processed_files=50,
            failed_files=0,
        )
        db_session.add(job)
        await db_session.commit()
        await db_session.refresh(job)

        response = await client.post(f"/api/v1/index/cancel/{job.id}")

        assert response.status_code == 400
        data = response.json()
        assert "not running" in data["detail"]

    @pytest.mark.asyncio()
    async def test_cancel_indexing_starting_status(
        self, client: AsyncClient, db_session: AsyncSession, watch_folder: WatchFolder
    ):
        job = IndexingJob(
            watch_folder_id=str(watch_folder.id),
            status=JobStatus.STARTING,
            total_files=0,
            processed_files=0,
            failed_files=0,
        )
        db_session.add(job)
        await db_session.commit()
        await db_session.refresh(job)

        response = await client.post(f"/api/v1/index/cancel/{job.id}")

        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True

    @pytest.mark.asyncio()
    async def test_cancel_indexing_already_failed(
        self, client: AsyncClient, db_session: AsyncSession, watch_folder: WatchFolder
    ):
        job = IndexingJob(
            watch_folder_id=str(watch_folder.id),
            status=JobStatus.FAILED,
            total_files=10,
            processed_files=5,
            failed_files=5,
            error_message="Failed",
        )
        db_session.add(job)
        await db_session.commit()
        await db_session.refresh(job)

        response = await client.post(f"/api/v1/index/cancel/{job.id}")

        assert response.status_code == 400


class TestIndexObservability:
    @pytest.mark.asyncio()
    async def test_trigger_indexing_creates_span(
        self, client: AsyncClient, watch_folder: WatchFolder
    ):
        with patch("src.api.routes.index.tracer") as mock_tracer:

            response = await client.post(
                "/api/v1/index",
                json={"watch_folder_id": str(watch_folder.id), "force": False, "recursive": True},
            )

            assert response.status_code == 200
            mock_tracer.start_as_current_span.assert_called_with("api_trigger_indexing")

    @pytest.mark.asyncio()
    async def test_trigger_indexing_sets_span_attributes(
        self, client: AsyncClient, watch_folder: WatchFolder
    ):
        with patch("src.api.routes.index.tracer") as mock_tracer:
            mock_span = mock_tracer.start_as_current_span.return_value.__enter__.return_value

            response = await client.post(
                "/api/v1/index",
                json={"watch_folder_id": str(watch_folder.id), "force": True, "recursive": False},
            )

            assert response.status_code == 200
            assert mock_span.set_attribute.called
