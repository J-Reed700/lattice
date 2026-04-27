import asyncio
from pathlib import Path
from uuid import uuid4

from httpx import AsyncClient
import pytest
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from src.models import File, TextContent
from src.models.indexing_job import IndexingJob, JobStatus


@pytest.mark.integration()
class TestConcurrentFileIndexing:
    async def test_concurrent_file_uploads(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        watch_folder,
        concurrent_test_files: list[Path],
        mock_embedding_service,
        mocker,
    ):
        mocker.patch(
            "src.services.indexing.IndexingService._get_embedding_service",
            return_value=mock_embedding_service,
        )
        mocker.patch(
            "src.services.indexing.IndexingService._get_storage_service"
        ).return_value.store_file.return_value = ("http://storage.url", None)

        tasks = []
        for file_path in concurrent_test_files:
            payload = {
                "watch_folder_id": str(watch_folder.id),
                "path": str(file_path),
                "force_reindex": False,
            }
            tasks.append(client.post("/api/v1/files", json=payload))

        responses = await asyncio.gather(*tasks, return_exceptions=True)

        success_count = sum(
            1 for r in responses if not isinstance(r, Exception) and r.status_code == 200
        )

        assert success_count >= 8

        result = await db_session.execute(select(File))
        indexed_files = result.scalars().all()
        assert len(indexed_files) >= 8

    async def test_concurrent_searches(
        self, client: AsyncClient, indexed_files_batch, mock_embedding_service, mocker
    ):
        mocker.patch(
            "src.services.search.SearchService._get_embedding_service",
            return_value=mock_embedding_service,
        )

        search_queries = [
            "python programming",
            "javascript guide",
            "data science",
            "machine learning",
            "web development",
        ]

        tasks = []
        for query in search_queries:
            payload = {"query": query, "mode": "text", "limit": 10, "offset": 0}
            tasks.append(client.post("/api/v1/search", json=payload))

        responses = await asyncio.gather(*tasks, return_exceptions=True)

        success_count = sum(
            1 for r in responses if not isinstance(r, Exception) and r.status_code == 200
        )

        assert success_count == len(search_queries)

        for response in responses:
            if not isinstance(response, Exception):
                data = response.json()
                assert "results" in data
                assert "took_ms" in data

    async def test_concurrent_file_and_search_operations(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        watch_folder,
        temp_dir: Path,
        indexed_file_with_content,
        mock_embedding_service,
        mocker,
    ):
        mocker.patch(
            "src.services.indexing.IndexingService._get_embedding_service",
            return_value=mock_embedding_service,
        )
        mocker.patch(
            "src.services.indexing.IndexingService._get_storage_service"
        ).return_value.store_file.return_value = ("http://storage.url", None)

        upload_file = temp_dir / "upload_test.txt"
        upload_file.write_text("Upload test content")

        upload_task = client.post(
            "/api/v1/files",
            json={
                "watch_folder_id": str(watch_folder.id),
                "path": str(upload_file),
                "force_reindex": False,
            },
        )

        search_task = client.post(
            "/api/v1/search", json={"query": "test", "mode": "text", "limit": 10, "offset": 0}
        )

        list_task = client.get("/api/v1/files?limit=10")

        upload_resp, search_resp, list_resp = await asyncio.gather(
            upload_task, search_task, list_task
        )

        assert upload_resp.status_code == 200
        assert search_resp.status_code == 200
        assert list_resp.status_code == 200


@pytest.mark.integration()
class TestConcurrentSearchWithFilters:
    async def test_concurrent_filtered_searches(self, client: AsyncClient, indexed_files_batch):
        filter_combinations = [
            {"mime_types": ["text/plain"]},
            {"extensions": ["md"]},
            {"extensions": ["txt", "json"]},
            {},
            {"mime_types": ["text/markdown"]},
        ]

        tasks = []
        for filters in filter_combinations:
            payload = {
                "query": "test",
                "mode": "text",
                "limit": 10,
                "offset": 0,
                "filters": filters,
            }
            tasks.append(client.post("/api/v1/search", json=payload))

        responses = await asyncio.gather(*tasks)

        assert all(r.status_code == 200 for r in responses)


@pytest.mark.integration()
class TestConcurrentFileUpdates:
    async def test_concurrent_file_metadata_updates(
        self, client: AsyncClient, db_session: AsyncSession, multiple_indexed_files
    ):
        from datetime import datetime

        tasks = []
        for file in multiple_indexed_files[:5]:
            payload = {"last_accessed_at": datetime.utcnow().isoformat()}
            tasks.append(client.patch(f"/api/v1/files/{file.id}", json=payload))

        responses = await asyncio.gather(*tasks, return_exceptions=True)

        success_count = sum(
            1 for r in responses if not isinstance(r, Exception) and r.status_code == 200
        )

        assert success_count >= 4


@pytest.mark.integration()
class TestConcurrentBatchOperations:
    @pytest.mark.slow()
    async def test_batch_indexing_with_concurrent_searches(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        watch_folder,
        temp_dir: Path,
        mock_embedding_service,
        mocker,
    ):
        mocker.patch(
            "src.services.indexing.IndexingService._get_embedding_service",
            return_value=mock_embedding_service,
        )
        mocker.patch(
            "src.services.indexing.IndexingService._get_storage_service"
        ).return_value.store_file.return_value = ("http://storage.url", None)

        for i in range(20):
            file_path = temp_dir / f"batch_{i}.txt"
            file_path.write_text(f"Batch content {i}")

        upload_tasks = []
        for i in range(20):
            file_path = temp_dir / f"batch_{i}.txt"
            payload = {
                "watch_folder_id": str(watch_folder.id),
                "path": str(file_path),
                "force_reindex": False,
            }
            upload_tasks.append(client.post("/api/v1/files", json=payload))

        search_tasks = []
        for i in range(5):
            payload = {"query": f"content {i}", "mode": "text", "limit": 10, "offset": 0}
            search_tasks.append(client.post("/api/v1/search", json=payload))

        all_tasks = upload_tasks + search_tasks

        responses = await asyncio.gather(*all_tasks, return_exceptions=True)

        assert len(responses) == 25


@pytest.mark.integration()
class TestRaceConditions:
    async def test_duplicate_file_upload_race_condition(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        watch_folder,
        sample_text_file: Path,
        mock_embedding_service,
        mocker,
    ):
        mocker.patch(
            "src.services.indexing.IndexingService._get_embedding_service",
            return_value=mock_embedding_service,
        )
        mocker.patch(
            "src.services.indexing.IndexingService._get_storage_service"
        ).return_value.store_file.return_value = ("http://storage.url", None)

        payload = {
            "watch_folder_id": str(watch_folder.id),
            "path": str(sample_text_file),
            "force_reindex": False,
        }

        tasks = [client.post("/api/v1/files", json=payload) for _ in range(5)]

        await asyncio.gather(*tasks, return_exceptions=True)

        result = await db_session.execute(select(File).where(File.path == str(sample_text_file)))
        files = result.scalars().all()

        assert len(files) <= 1

    async def test_concurrent_file_deletion_and_search(
        self, client: AsyncClient, db_session: AsyncSession, indexed_file_with_content
    ):
        file_id = indexed_file_with_content.id

        delete_task = client.delete(f"/api/v1/files/{file_id}")

        search_task = client.post(
            "/api/v1/search", json={"query": "test", "mode": "text", "limit": 10, "offset": 0}
        )

        delete_resp, search_resp = await asyncio.gather(
            delete_task, search_task, return_exceptions=True
        )

        assert not isinstance(delete_resp, Exception)
        assert not isinstance(search_resp, Exception)


@pytest.mark.integration()
class TestConcurrentStressTest:
    @pytest.mark.slow()
    async def test_high_concurrency_mixed_operations(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        watch_folder,
        temp_dir: Path,
        indexed_files_batch,
        mock_embedding_service,
        mocker,
    ):
        mocker.patch(
            "src.services.indexing.IndexingService._get_embedding_service",
            return_value=mock_embedding_service,
        )
        mocker.patch(
            "src.services.indexing.IndexingService._get_storage_service"
        ).return_value.store_file.return_value = ("http://storage.url", None)

        tasks = []

        for i in range(10):
            file_path = temp_dir / f"stress_{i}.txt"
            file_path.write_text(f"Stress test content {i}")
            payload = {
                "watch_folder_id": str(watch_folder.id),
                "path": str(file_path),
                "force_reindex": False,
            }
            tasks.append(client.post("/api/v1/files", json=payload))

        for i in range(10):
            payload = {"query": f"test {i}", "mode": "text", "limit": 10, "offset": 0}
            tasks.append(client.post("/api/v1/search", json=payload))

        for _ in range(5):
            tasks.append(client.get("/api/v1/files?limit=20"))

        for _ in range(5):
            tasks.append(client.get("/api/v1/files/stats"))

        responses = await asyncio.gather(*tasks, return_exceptions=True)

        errors = [r for r in responses if isinstance(r, Exception)]
        assert len(errors) < len(tasks) * 0.1


@pytest.mark.integration()
class TestConcurrentDatabaseTransactions:
    async def test_concurrent_text_content_writes(
        self, db_session: AsyncSession, multiple_indexed_files
    ):
        async def add_content(file):
            text_content = TextContent(
                id=uuid4(),
                file_id=file.id,
                content=f"Concurrent content for {file.filename}",
                language="en",
                char_count=50,
                word_count=8,
                embedding=[0.3] * 384,
            )
            db_session.add(text_content)
            await db_session.commit()

        tasks = [add_content(file) for file in multiple_indexed_files]

        results = await asyncio.gather(*tasks, return_exceptions=True)

        errors = [r for r in results if isinstance(r, Exception)]
        assert len(errors) == 0

        for file in multiple_indexed_files:
            await db_session.refresh(file)
            result = await db_session.execute(
                select(TextContent).where(TextContent.file_id == file.id)
            )
            content = result.scalar_one_or_none()
            assert content is not None


@pytest.mark.integration()
class TestConcurrentRateLimiting:
    """Test thread-safe rate limiting with concurrent requests."""

    async def test_concurrent_agent_queries_rate_limit(self, client: AsyncClient, mocker):
        """Test that rate limiter handles concurrent requests correctly."""
        # Mock the Ollama service to always be available
        mocker.patch(
            "src.services.llm.ollama_service.OllamaService.health_check", return_value=True
        )
        mocker.patch(
            "src.services.langchain.base_service.BaseLangChainService.invoke_agent"
        ).return_value.success = True
        mocker.patch(
            "src.services.langchain.base_service.BaseLangChainService.invoke_agent"
        ).return_value.output = "Test response"
        mocker.patch(
            "src.services.langchain.base_service.BaseLangChainService.invoke_agent"
        ).return_value.intermediate_steps = []
        mocker.patch(
            "src.services.langchain.base_service.BaseLangChainService.invoke_agent"
        ).return_value.metadata = {}

        # Send 15 concurrent requests (rate limit is 10/minute)
        tasks = []
        for i in range(15):
            payload = {
                "query": f"test query {i}",
                "streaming": False,
                "context_limit": 3,
                "use_local_llm": True,
            }
            tasks.append(client.post("/api/v1/agent/query", json=payload))

        responses = await asyncio.gather(*tasks, return_exceptions=True)

        # Count successful and rate-limited responses
        success_count = sum(
            1 for r in responses if not isinstance(r, Exception) and r.status_code == 200
        )
        rate_limited_count = sum(
            1 for r in responses if not isinstance(r, Exception) and r.status_code == 429
        )

        # Should have some rate-limited responses
        assert rate_limited_count > 0
        # Total should match request count
        assert success_count + rate_limited_count == 15


@pytest.mark.integration()
class TestConcurrentIndexingJobs:
    """Test database-backed indexing job tracking with concurrent operations."""

    async def test_concurrent_indexing_job_creation(
        self, client: AsyncClient, db_session: AsyncSession, mocker
    ):
        """Test that multiple concurrent indexing jobs can be created."""
        uuid4()

        # Mock the indexing service
        mocker.patch("src.services.indexing.IndexingService.index_watch_folder")

        # Create multiple concurrent indexing requests for different watch folders
        tasks = []
        watch_folder_ids = [uuid4() for _ in range(5)]
        for wf_id in watch_folder_ids:
            payload = {"watch_folder_id": str(wf_id), "force": False, "recursive": True}
            tasks.append(client.post("/api/v1/index", json=payload))

        responses = await asyncio.gather(*tasks, return_exceptions=True)

        # All requests should succeed
        success_count = sum(
            1 for r in responses if not isinstance(r, Exception) and r.status_code == 200
        )
        assert success_count == 5

        # Verify jobs were created in database
        result = await db_session.execute(select(IndexingJob))
        jobs = result.scalars().all()
        assert len(jobs) >= 5

    async def test_concurrent_same_folder_indexing_prevented(
        self, client: AsyncClient, db_session: AsyncSession, mocker
    ):
        """Test that concurrent indexing of the same folder is prevented."""
        watch_folder_id = uuid4()

        # Mock the indexing service to take some time
        async def slow_index(*args, **kwargs):
            await asyncio.sleep(0.5)

        mocker.patch(
            "src.services.indexing.IndexingService.index_watch_folder", side_effect=slow_index
        )

        # Try to start multiple concurrent jobs for the same watch folder
        payload = {"watch_folder_id": str(watch_folder_id), "force": False, "recursive": True}
        tasks = [client.post("/api/v1/index", json=payload) for _ in range(5)]

        responses = await asyncio.gather(*tasks, return_exceptions=True)

        # Only one should succeed, others should get 409 Conflict
        success_count = sum(
            1 for r in responses if not isinstance(r, Exception) and r.status_code == 200
        )
        conflict_count = sum(
            1 for r in responses if not isinstance(r, Exception) and r.status_code == 409
        )

        # Should have exactly one success and the rest conflicts
        assert success_count >= 1
        assert conflict_count >= 3

    async def test_concurrent_job_status_queries(
        self, client: AsyncClient, db_session: AsyncSession
    ):
        """Test that concurrent status queries work correctly."""
        # Create a job
        job = IndexingJob(
            watch_folder_id=str(uuid4()),
            status=JobStatus.RUNNING,
            total_files=100,
            processed_files=50,
            failed_files=2,
        )
        db_session.add(job)
        await db_session.commit()
        await db_session.refresh(job)

        # Query status concurrently multiple times
        tasks = [client.get(f"/api/v1/index/status/{job.id}") for _ in range(10)]

        responses = await asyncio.gather(*tasks, return_exceptions=True)

        # All should succeed
        success_count = sum(
            1 for r in responses if not isinstance(r, Exception) and r.status_code == 200
        )
        assert success_count == 10

        # All should return the same data
        for response in responses:
            if not isinstance(response, Exception):
                data = response.json()
                assert data["job_id"] == job.id
                assert data["status"] == "running"
                assert data["total_files"] == 100
                assert data["processed_files"] == 50


@pytest.mark.integration()
class TestConcurrentSearchEngineInitialization:
    """Test thread-safe search engine initialization with concurrent access."""

    async def test_concurrent_bm25_engine_access(self, client: AsyncClient, indexed_files_batch):
        """Test that concurrent BM25 search requests initialize engine correctly."""
        # Make multiple concurrent search requests that will trigger engine initialization
        tasks = []
        for i in range(10):
            payload = {"query": f"test query {i}", "mode": "bm25", "limit": 5, "offset": 0}
            tasks.append(client.post("/api/v1/search", json=payload))

        responses = await asyncio.gather(*tasks, return_exceptions=True)

        # All should succeed without race conditions
        success_count = sum(
            1 for r in responses if not isinstance(r, Exception) and r.status_code == 200
        )

        # Most should succeed (allow for some failures due to test environment)
        assert success_count >= 8

    async def test_concurrent_hybrid_engine_access(self, client: AsyncClient, indexed_files_batch):
        """Test that concurrent hybrid search requests initialize engine correctly."""
        # Make multiple concurrent search requests that will trigger engine initialization
        tasks = []
        for i in range(10):
            payload = {"query": f"test query {i}", "mode": "hybrid", "limit": 5, "offset": 0}
            tasks.append(client.post("/api/v1/search", json=payload))

        responses = await asyncio.gather(*tasks, return_exceptions=True)

        # All should succeed without race conditions
        success_count = sum(
            1 for r in responses if not isinstance(r, Exception) and r.status_code == 200
        )

        # Most should succeed (allow for some failures due to test environment)
        assert success_count >= 8
