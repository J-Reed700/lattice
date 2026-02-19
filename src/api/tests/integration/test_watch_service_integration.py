import asyncio
from datetime import datetime
from unittest.mock import AsyncMock, patch
from uuid import uuid4

import pytest
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker, create_async_engine

from src.config import get_settings
from src.models import Base, File, WatchFolder
from src.services.indexing import IndexingService
from src.services.watch import WatchService


@pytest.fixture()
async def db_session():
    """Create a test database session."""
    engine = create_async_engine("sqlite+aiosqlite:///:memory:", echo=False)

    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)

    session_factory = async_sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)

    async with session_factory() as session:
        yield session

    await engine.dispose()


@pytest.fixture()
def watch_service():
    """Create WatchService instance."""
    return WatchService()


@pytest.fixture()
def mock_indexing_service():
    """Create mocked IndexingService."""
    service = AsyncMock(spec=IndexingService)
    service.index_file = AsyncMock(return_value=True)
    service.reindex_file = AsyncMock(return_value=True)
    service.delete_file_index = AsyncMock(return_value=True)
    return service


class TestWatchServiceIntegration:
    """Integration tests for WatchService."""

    @pytest.mark.asyncio()
    async def test_start_watching_creates_watcher(self, watch_service, db_session, tmp_path):
        """Test that starting watch creates a FileWatcher."""
        test_dir = tmp_path / "documents"
        test_dir.mkdir()

        watch_folder = WatchFolder(id=uuid4(), path=str(test_dir), recursive=True, active=True)
        db_session.add(watch_folder)
        await db_session.commit()

        session_factory = lambda: db_session
        watch_service.set_session_factory(session_factory)

        await watch_service.start_watching(
            watch_folder_id=watch_folder.id,
            path=str(test_dir),
            recursive=True,
            db_session=db_session,
        )

        assert watch_folder.id in watch_service.watchers
        assert watch_service.watchers[watch_folder.id].running is True

        await watch_service.stop_watching(watch_folder.id)

    @pytest.mark.asyncio()
    async def test_stop_watching_removes_watcher(self, watch_service, db_session, tmp_path):
        """Test that stopping watch removes the watcher."""
        test_dir = tmp_path / "documents"
        test_dir.mkdir()

        watch_folder_id = uuid4()
        session_factory = lambda: db_session
        watch_service.set_session_factory(session_factory)

        await watch_service.start_watching(
            watch_folder_id=watch_folder_id, path=str(test_dir), recursive=True
        )

        await watch_service.stop_watching(watch_folder_id)

        assert watch_folder_id not in watch_service.watchers

    @pytest.mark.asyncio()
    async def test_stop_all_stops_all_watchers(self, watch_service, db_session, tmp_path):
        """Test that stop_all stops all active watchers."""
        test_dir1 = tmp_path / "documents1"
        test_dir1.mkdir()
        test_dir2 = tmp_path / "documents2"
        test_dir2.mkdir()

        session_factory = lambda: db_session
        watch_service.set_session_factory(session_factory)

        watch_id1 = uuid4()
        watch_id2 = uuid4()

        await watch_service.start_watching(watch_id1, str(test_dir1))
        await watch_service.start_watching(watch_id2, str(test_dir2))

        assert len(watch_service.watchers) == 2

        await watch_service.stop_all()

        assert len(watch_service.watchers) == 0

    @pytest.mark.asyncio()
    async def test_handle_file_created_event(
        self, watch_service, db_session, tmp_path, mock_indexing_service
    ):
        """Test handling file created event."""
        with patch.object(watch_service, "indexing_service", mock_indexing_service):
            test_dir = tmp_path / "documents"
            test_dir.mkdir()
            test_file = test_dir / "document.txt"
            test_file.write_text("Test content")

            watch_folder = WatchFolder(id=uuid4(), path=str(test_dir), recursive=True, active=True)
            db_session.add(watch_folder)
            await db_session.commit()

            from src.modules.file_watcher import EventPriority, FileEvent, FileEventType

            event = FileEvent(
                event_type=FileEventType.CREATED,
                file_path=test_file,
                timestamp=datetime.now(),
                priority=EventPriority.HIGH,
                file_hash="abc123",
            )

            session_factory = lambda: db_session
            watch_service.set_session_factory(session_factory)
            watch_service._session_factory = lambda: db_session

            await watch_service._handle_file_event(event, watch_folder.id)

            result = await db_session.execute(select(File).where(File.path == str(test_file)))
            file_obj = result.scalar_one_or_none()

            assert file_obj is not None
            assert file_obj.filename == "document.txt"
            assert file_obj.watch_folder_id == watch_folder.id
            mock_indexing_service.index_file.assert_called_once()

    @pytest.mark.asyncio()
    async def test_handle_file_modified_event(
        self, watch_service, db_session, tmp_path, mock_indexing_service
    ):
        """Test handling file modified event."""
        with patch.object(watch_service, "indexing_service", mock_indexing_service):
            test_dir = tmp_path / "documents"
            test_dir.mkdir()
            test_file = test_dir / "document.txt"
            test_file.write_text("Original content")

            watch_folder = WatchFolder(id=uuid4(), path=str(test_dir), recursive=True, active=True)
            db_session.add(watch_folder)

            file_obj = File(
                id=uuid4(),
                path=str(test_file),
                filename="document.txt",
                mime_type="text/plain",
                size=100,
                watch_folder_id=watch_folder.id,
                hash="old_hash",
            )
            db_session.add(file_obj)
            await db_session.commit()

            test_file.write_text("Modified content")

            from src.modules.file_watcher import EventPriority, FileEvent, FileEventType

            event = FileEvent(
                event_type=FileEventType.MODIFIED,
                file_path=test_file,
                timestamp=datetime.now(),
                priority=EventPriority.MEDIUM,
                file_hash="new_hash",
            )

            watch_service._session_factory = lambda: db_session

            await watch_service._handle_file_event(event, watch_folder.id)

            await db_session.refresh(file_obj)
            assert file_obj.hash == "new_hash"
            mock_indexing_service.reindex_file.assert_called_once()

    @pytest.mark.asyncio()
    async def test_handle_file_deleted_event(
        self, watch_service, db_session, tmp_path, mock_indexing_service
    ):
        """Test handling file deleted event."""
        with patch.object(watch_service, "indexing_service", mock_indexing_service):
            test_dir = tmp_path / "documents"
            test_dir.mkdir()
            test_file = test_dir / "document.txt"

            watch_folder = WatchFolder(id=uuid4(), path=str(test_dir), recursive=True, active=True)
            db_session.add(watch_folder)

            file_obj = File(
                id=uuid4(),
                path=str(test_file),
                filename="document.txt",
                mime_type="text/plain",
                size=100,
                watch_folder_id=watch_folder.id,
                hash="abc123",
            )
            db_session.add(file_obj)
            await db_session.commit()

            from src.modules.file_watcher import EventPriority, FileEvent, FileEventType

            event = FileEvent(
                event_type=FileEventType.DELETED,
                file_path=test_file,
                timestamp=datetime.now(),
                priority=EventPriority.LOW,
            )

            watch_service._session_factory = lambda: db_session

            await watch_service._handle_file_event(event, watch_folder.id)

            await db_session.refresh(file_obj)
            assert file_obj.is_deleted is True
            mock_indexing_service.delete_file_index.assert_called_once()

    @pytest.mark.asyncio()
    async def test_should_process_file_with_filters(self, watch_service, tmp_path):
        """Test file filtering based on type filters."""
        settings = get_settings()
        original_filters = settings.file_watcher_file_type_filters

        try:
            settings.file_watcher_file_type_filters = ["*.txt", "*.pdf"]

            txt_file = tmp_path / "document.txt"
            txt_file.write_text("content")
            assert watch_service._should_process_file(txt_file) is True

            pdf_file = tmp_path / "document.pdf"
            pdf_file.write_text("content")
            assert watch_service._should_process_file(pdf_file) is True

            jpg_file = tmp_path / "image.jpg"
            jpg_file.write_text("content")
            assert watch_service._should_process_file(jpg_file) is False

        finally:
            settings.file_watcher_file_type_filters = original_filters

    @pytest.mark.asyncio()
    async def test_should_process_file_without_filters(self, watch_service, tmp_path):
        """Test file processing when no filters are set."""
        settings = get_settings()
        original_filters = settings.file_watcher_file_type_filters

        try:
            settings.file_watcher_file_type_filters = []

            any_file = tmp_path / "document.xyz"
            any_file.write_text("content")
            assert watch_service._should_process_file(any_file) is True

        finally:
            settings.file_watcher_file_type_filters = original_filters

    @pytest.mark.asyncio()
    async def test_compute_file_hash(self, watch_service, tmp_path):
        """Test file hash computation."""
        test_file = tmp_path / "document.txt"
        test_file.write_text("Test content for hashing")

        hash1 = watch_service._compute_file_hash(test_file)
        assert hash1
        assert len(hash1) == 64

        hash2 = watch_service._compute_file_hash(test_file)
        assert hash1 == hash2

        test_file.write_text("Different content")
        hash3 = watch_service._compute_file_hash(test_file)
        assert hash3 != hash1

    @pytest.mark.asyncio()
    async def test_handle_large_file_skipped(self, watch_service, db_session, tmp_path):
        """Test that files exceeding size limit are skipped."""
        settings = get_settings()
        original_size = settings.file_watcher_max_file_size

        try:
            settings.file_watcher_max_file_size = 100

            test_dir = tmp_path / "documents"
            test_dir.mkdir()
            test_file = test_dir / "large.txt"
            test_file.write_text("x" * 1000)

            watch_folder = WatchFolder(id=uuid4(), path=str(test_dir), recursive=True, active=True)
            db_session.add(watch_folder)
            await db_session.commit()

            from src.modules.file_watcher import EventPriority, FileEvent, FileEventType

            event = FileEvent(
                event_type=FileEventType.CREATED,
                file_path=test_file,
                timestamp=datetime.now(),
                priority=EventPriority.HIGH,
            )

            watch_service._session_factory = lambda: db_session

            await watch_service._handle_file_event(event, watch_folder.id)

            result = await db_session.execute(select(File).where(File.path == str(test_file)))
            file_obj = result.scalar_one_or_none()

            assert file_obj is None

        finally:
            settings.file_watcher_max_file_size = original_size

    @pytest.mark.asyncio()
    async def test_already_watching_same_directory(self, watch_service, db_session, tmp_path):
        """Test that watching the same directory twice doesn't create duplicates."""
        test_dir = tmp_path / "documents"
        test_dir.mkdir()

        watch_folder_id = uuid4()
        session_factory = lambda: db_session
        watch_service.set_session_factory(session_factory)

        await watch_service.start_watching(watch_folder_id, str(test_dir))
        await watch_service.start_watching(watch_folder_id, str(test_dir))

        assert len(watch_service.watchers) == 1

        await watch_service.stop_watching(watch_folder_id)


@pytest.mark.asyncio()
async def test_end_to_end_file_watching_workflow(tmp_path):
    """End-to-end test of file watching workflow."""
    engine = create_async_engine("sqlite+aiosqlite:///:memory:", echo=False)

    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)

    session_factory = async_sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)

    test_dir = tmp_path / "documents"
    test_dir.mkdir()

    watch_service = WatchService()
    watch_service.set_session_factory(session_factory)

    async with session_factory() as session:
        watch_folder = WatchFolder(id=uuid4(), path=str(test_dir), recursive=True, active=True)
        session.add(watch_folder)
        await session.commit()

        with patch.object(
            watch_service.indexing_service, "index_file", new=AsyncMock(return_value=True)
        ):
            await watch_service.start_watching(
                watch_folder_id=watch_folder.id,
                path=str(test_dir),
                recursive=True,
                db_session=session,
            )

            test_file = test_dir / "test_document.txt"
            test_file.write_text("Test content")

            await asyncio.sleep(1.0)

            await watch_service.stop_watching(watch_folder.id)

    await engine.dispose()
