"""Watch service - orchestrates file system monitoring and auto-indexing.

This service coordinates:
- File system monitoring (via file_watcher module)
- Event routing (CREATED, MODIFIED, DELETED, MOVED)
- File indexing (via IndexingService)
- Database persistence (File, WatchFolder)

The service uses an EVENT-DRIVEN pattern:
- FileWatcher emits events → WatchService routes to handlers → IndexingService indexes

The service is a THIN ORCHESTRATOR - it routes events and manages state,
but delegates indexing logic to IndexingService and domain modules.
"""

from __future__ import annotations

from datetime import UTC, datetime
import hashlib
import logging
from pathlib import Path
from typing import TYPE_CHECKING
from uuid import UUID

from sqlalchemy import select, update
from sqlalchemy.ext.asyncio import AsyncSession

from src.config import get_settings
from src.events.domain.watch_events import (
    DirectoryWatchStarted,
    DirectoryWatchStopped,
    FileChangeDetected,
    WatchErrorOccurred,
)
from src.models import File, WatchFolder
from src.modules.file_watcher import FileEvent, FileEventType, FileWatcher
from src.services.indexing import IndexingService

if TYPE_CHECKING:
    from src.events.bus import EventBus

logger = logging.getLogger(__name__)


class WatchService:
    def __init__(self, event_bus: EventBus | None = None):
        self.watchers: dict[UUID, FileWatcher] = {}
        self.watcher_metadata: dict[UUID, dict[str, str | int]] = {}  # Store path and user_id
        self.indexing_service = IndexingService()
        self.settings = get_settings()
        self._session_factory = None
        self.event_bus = event_bus

    def set_session_factory(self, session_factory):
        self._session_factory = session_factory

    async def start_watching(
        self,
        watch_folder_id: UUID,
        path: str,
        recursive: bool = True,
        db_session: AsyncSession | None = None,
    ) -> None:
        if watch_folder_id in self.watchers:
            logger.warning(f"Already watching: {path}")
            return

        logger.info(f"Starting watch for: {path}")

        # Get watch folder to retrieve user_id for event
        user_id = 0
        if db_session:
            result = await db_session.execute(
                select(WatchFolder).where(WatchFolder.id == watch_folder_id)
            )
            watch_folder = result.scalar_one_or_none()
            if watch_folder:
                # Parse user_id from electric_user_id string
                try:
                    user_id = int(watch_folder.electric_user_id)
                except (ValueError, AttributeError):
                    user_id = 0

        try:
            watcher = FileWatcher(
                process_callback=lambda event: self._handle_file_event(event, watch_folder_id),
                ignore_patterns=self.settings.file_watcher_ignore_patterns,
                num_workers=self.settings.file_watcher_num_workers,
                debounce_seconds=self.settings.file_watcher_debounce_seconds,
            )

            await watcher.start([path])

            self.watchers[watch_folder_id] = watcher
            self.watcher_metadata[watch_folder_id] = {"path": path, "user_id": user_id}

            if recursive:
                await watcher.scan_directory(Path(path))

            if db_session:
                await db_session.execute(
                    update(WatchFolder)
                    .where(WatchFolder.id == watch_folder_id)
                    .values(last_scan_at=datetime.now(UTC))
                )
                await db_session.commit()

            # Emit DirectoryWatchStarted event
            if self.event_bus:
                event = DirectoryWatchStarted(
                    directory_path=path,
                    user_id=user_id,
                    recursive=recursive,
                )
                await self.event_bus.publish(event)

            logger.info(f"Successfully started watching: {path}")

        except Exception as e:
            logger.error(f"Failed to start watching {path}: {e}", exc_info=True)
            # Emit WatchErrorOccurred event
            if self.event_bus:
                error_event = WatchErrorOccurred(
                    directory_path=path,
                    error_message=str(e),
                    error_type=type(e).__name__,
                    user_id=user_id,
                )
                await self.event_bus.publish(error_event)
            raise

    async def stop_watching(self, watch_folder_id: UUID, reason: str = "user_request") -> None:
        if watch_folder_id in self.watchers:
            watcher = self.watchers[watch_folder_id]
            metadata = self.watcher_metadata.get(watch_folder_id, {})
            path = metadata.get("path", "unknown")
            user_id = metadata.get("user_id", 0)

            await watcher.stop()
            del self.watchers[watch_folder_id]
            if watch_folder_id in self.watcher_metadata:
                del self.watcher_metadata[watch_folder_id]

            # Emit DirectoryWatchStopped event
            if self.event_bus:
                event = DirectoryWatchStopped(
                    directory_path=str(path),
                    user_id=int(user_id),
                    reason=reason,
                )
                await self.event_bus.publish(event)

            logger.info(f"Stopped watch: {watch_folder_id}")

    async def stop_all(self) -> None:
        logger.info(f"Stopping {len(self.watchers)} watchers...")
        for watch_folder_id in list(self.watchers.keys()):
            await self.stop_watching(watch_folder_id, reason="shutdown")
        logger.info("All watchers stopped")

    async def _handle_file_event(self, event: FileEvent, watch_folder_id: UUID) -> None:
        logger.info(f"File event: {event.event_type} - {event.file_path}")

        if not self._should_process_file(event.file_path):
            logger.debug(f"Skipping file: {event.file_path}")
            return

        if not self._session_factory:
            logger.error("Session factory not set. Cannot process file event.")
            return

        # Get metadata for event emission
        metadata = self.watcher_metadata.get(watch_folder_id, {})
        directory_path = metadata.get("path", "unknown")
        user_id = metadata.get("user_id", 0)

        async with self._session_factory() as session:
            try:
                if event.event_type == FileEventType.CREATED:
                    await self._handle_created(event, watch_folder_id, session)
                elif event.event_type == FileEventType.MODIFIED:
                    await self._handle_modified(event, watch_folder_id, session)
                elif event.event_type == FileEventType.DELETED:
                    await self._handle_deleted(event, session)
                elif event.event_type == FileEventType.MOVED:
                    await self._handle_moved(event, watch_folder_id, session)

                await session.commit()

                # Emit FileChangeDetected event after successful processing
                if self.event_bus:
                    change_type_map = {
                        FileEventType.CREATED: "created",
                        FileEventType.MODIFIED: "modified",
                        FileEventType.DELETED: "deleted",
                        FileEventType.MOVED: "deleted",  # MOVED is treated as delete
                    }
                    change_type = change_type_map.get(event.event_type, "modified")

                    change_event = FileChangeDetected(
                        file_path=str(event.file_path),
                        change_type=change_type,  # type: ignore
                        directory_path=str(directory_path),
                        user_id=int(user_id),
                    )
                    await self.event_bus.publish(change_event)

            except Exception as e:
                logger.error(f"Failed to handle file event {event.file_path}: {e}", exc_info=True)
                await session.rollback()
                # Emit WatchErrorOccurred event
                if self.event_bus:
                    error_event = WatchErrorOccurred(
                        directory_path=str(directory_path),
                        error_message=str(e),
                        error_type=type(e).__name__,
                        user_id=int(user_id),
                    )
                    await self.event_bus.publish(error_event)
                raise

    async def _handle_created(
        self, event: FileEvent, watch_folder_id: UUID, session: AsyncSession
    ) -> None:
        file_path = event.file_path

        existing = await session.execute(select(File).where(File.path == str(file_path)))
        if existing.scalar_one_or_none():
            logger.debug(f"File already exists in database: {file_path}")
            return

        file_stats = file_path.stat()
        file_size = file_stats.st_size

        if file_size > self.settings.file_watcher_max_file_size:
            logger.warning(f"File too large to index: {file_path} ({file_size} bytes)")
            return

        import magic

        mime_type = magic.from_file(str(file_path), mime=True)

        file_obj = File(
            path=str(file_path),
            filename=file_path.name,
            mime_type=mime_type,
            size=file_size,
            watch_folder_id=watch_folder_id,
            processing_status="pending",
            hash=event.file_hash or self._compute_file_hash(file_path),
        )

        session.add(file_obj)
        await session.flush()

        logger.info(f"Indexing new file: {file_path}")
        await self.indexing_service.index_file(file_obj.id, session)

    async def _handle_modified(
        self, event: FileEvent, watch_folder_id: UUID, session: AsyncSession
    ) -> None:
        file_path = event.file_path

        result = await session.execute(select(File).where(File.path == str(file_path)))
        file_obj = result.scalar_one_or_none()

        if not file_obj:
            logger.info(f"Modified file not in database, treating as new: {file_path}")
            await self._handle_created(event, watch_folder_id, session)
            return

        current_hash = event.file_hash or self._compute_file_hash(file_path)
        if current_hash == file_obj.hash:
            logger.debug(f"File content unchanged: {file_path}")
            return

        file_stats = file_path.stat()
        file_obj.size = file_stats.st_size
        file_obj.hash = current_hash
        file_obj.processing_status = "pending"

        logger.info(f"Re-indexing modified file: {file_path}")
        await self.indexing_service.reindex_file(file_obj.id, session)

    async def _handle_deleted(self, event: FileEvent, session: AsyncSession) -> None:
        file_path = event.file_path

        result = await session.execute(select(File).where(File.path == str(file_path)))
        file_obj = result.scalar_one_or_none()

        if not file_obj:
            logger.debug(f"Deleted file not in database: {file_path}")
            return

        logger.info(f"Removing deleted file from index: {file_path}")
        await self.indexing_service.delete_file_index(file_obj.id, session)

        file_obj.is_deleted = True

    async def _handle_moved(
        self, event: FileEvent, watch_folder_id: UUID, session: AsyncSession
    ) -> None:
        await self._handle_deleted(event, session)

    def _should_process_file(self, file_path: Path) -> bool:
        if not file_path.exists() or not file_path.is_file():
            return False

        if not self.settings.file_watcher_file_type_filters:
            return True

        import fnmatch

        for pattern in self.settings.file_watcher_file_type_filters:
            if fnmatch.fnmatch(file_path.name, pattern):
                return True

        return False

    def _compute_file_hash(self, file_path: Path) -> str:
        hasher = hashlib.sha256()
        try:
            with open(file_path, "rb") as f:
                while chunk := f.read(8192):
                    hasher.update(chunk)
            return hasher.hexdigest()
        except Exception as e:
            logger.error(f"Failed to compute hash for {file_path}: {e}")
            return ""
