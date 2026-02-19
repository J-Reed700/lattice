from __future__ import annotations

import logging
from pathlib import Path
from uuid import UUID

from fastapi import APIRouter, BackgroundTasks, Body, Depends, Request
from sqlalchemy import func, select, update
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.dependencies import get_db
from src.api.errors import NotFoundError, ValidationError
from src.auth.dependencies import get_current_active_user
from src.auth.models import User
from src.middleware.csrf import csrf_protect
from src.models import File, WatchFolder
from src.schemas.watch import (
    ReindexRequest,
    WatchDirectory,
    WatchDirectoryCreate,
    WatchDirectoryList,
    WatchStatus,
)
from src.services.watch import WatchService

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/watch", tags=["watch"])


@router.post("/", response_model=WatchDirectory, status_code=201)
async def add_watch_directory(
    request: Request,
    config: WatchDirectoryCreate = Body(...),
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
) -> WatchDirectory:
    """Add directory to watch list and start monitoring."""
    existing = await db.execute(select(WatchFolder).where(WatchFolder.path == config.path))
    if existing.scalar_one_or_none():
        raise ValidationError(f"Directory already being watched: {config.path}")

    watch_folder = WatchFolder(
        path=config.path, recursive=config.recursive, active=config.auto_index
    )
    db.add(watch_folder)
    await db.flush()

    if config.auto_index and hasattr(request.app.state, "watch_service"):
        watch_service: WatchService = request.app.state.watch_service
        await watch_service.start_watching(
            watch_folder_id=watch_folder.id,
            path=watch_folder.path,
            recursive=watch_folder.recursive,
            db_session=db,
        )
        logger.info(f"Started watching directory: {watch_folder.path}")

    await db.commit()
    await db.refresh(watch_folder)

    files_count = await db.execute(
        select(func.count(File.id)).where(File.watch_folder_id == watch_folder.id)
    )
    files_watched = files_count.scalar() or 0

    return WatchDirectory(
        id=watch_folder.id,
        path=watch_folder.path,
        recursive=watch_folder.recursive,
        file_patterns=config.file_patterns,
        ignore_patterns=config.ignore_patterns,
        auto_index=watch_folder.active,
        status="active" if watch_folder.active else "inactive",
        files_watched=files_watched,
        last_scan=watch_folder.last_scan_at,
        created_at=watch_folder.created_at,
    )


@router.get("/", response_model=WatchDirectoryList)
async def list_watch_directories(
    current_user: User = Depends(get_current_active_user), db: AsyncSession = Depends(get_db)
) -> WatchDirectoryList:
    """List all watch directories."""
    result = await db.execute(select(WatchFolder))
    folders = result.scalars().all()

    items = []
    for folder in folders:
        files_count = await db.execute(
            select(func.count(File.id)).where(File.watch_folder_id == folder.id)
        )
        files_watched = files_count.scalar() or 0

        items.append(
            WatchDirectory(
                id=folder.id,
                path=folder.path,
                recursive=folder.recursive,
                file_patterns=["*"],
                ignore_patterns=[],
                auto_index=folder.active,
                status="active" if folder.active else "inactive",
                files_watched=files_watched,
                last_scan=folder.last_scan_at,
                created_at=folder.created_at,
            )
        )

    return WatchDirectoryList(items=items, total=len(items))


@router.get("/{watch_id}", response_model=WatchDirectory)
async def get_watch_directory(
    watch_id: UUID,
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
) -> WatchDirectory:
    """Get watch directory details."""
    result = await db.execute(select(WatchFolder).where(WatchFolder.id == watch_id))
    folder = result.scalar_one_or_none()
    if not folder:
        raise NotFoundError(f"Watch directory {watch_id} not found")

    files_count = await db.execute(
        select(func.count(File.id)).where(File.watch_folder_id == folder.id)
    )
    files_watched = files_count.scalar() or 0

    return WatchDirectory(
        id=folder.id,
        path=folder.path,
        recursive=folder.recursive,
        file_patterns=["*"],
        ignore_patterns=[],
        auto_index=folder.active,
        status="active" if folder.active else "inactive",
        files_watched=files_watched,
        last_scan=folder.last_scan_at,
        created_at=folder.created_at,
    )


@router.delete("/{watch_id}", status_code=204)
async def remove_watch_directory(
    request: Request,
    watch_id: UUID,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
) -> None:
    """Remove directory from watch list and stop monitoring."""
    result = await db.execute(select(WatchFolder).where(WatchFolder.id == watch_id))
    folder = result.scalar_one_or_none()
    if not folder:
        raise NotFoundError(f"Watch directory {watch_id} not found")

    if hasattr(request.app.state, "watch_service"):
        watch_service: WatchService = request.app.state.watch_service
        await watch_service.stop_watching(watch_id)
        logger.info(f"Stopped watching directory: {folder.path}")

    await db.delete(folder)
    await db.commit()


@router.get("/status", response_model=WatchStatus)
async def get_watch_status(
    request: Request,
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
) -> WatchStatus:
    """Get overall watch system status."""
    total_dirs = await db.execute(select(func.count(WatchFolder.id)))
    total_directories = total_dirs.scalar() or 0

    active_dirs = await db.execute(
        select(func.count(WatchFolder.id)).where(WatchFolder.active == True)
    )
    active_watchers = active_dirs.scalar() or 0

    total_files = await db.execute(select(func.count(File.id)))
    total_files_watched = total_files.scalar() or 0

    last_scan_result = await db.execute(
        select(WatchFolder.last_scan_at)
        .where(WatchFolder.last_scan_at.isnot(None))
        .order_by(WatchFolder.last_scan_at.desc())
        .limit(1)
    )
    last_event = last_scan_result.scalar_one_or_none()

    return WatchStatus(
        total_directories=total_directories,
        active_watchers=active_watchers,
        total_files_watched=total_files_watched,
        pending_index_queue=0,
        last_event=last_event,
    )


@router.post("/reindex", status_code=202)
async def trigger_reindex(
    request: Request,
    reindex_request: ReindexRequest = Body(...),
    background_tasks: BackgroundTasks = BackgroundTasks(),
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
) -> dict:
    """Manually trigger reindex of a watch directory."""
    if reindex_request.watch_id:
        result = await db.execute(
            select(WatchFolder).where(WatchFolder.id == reindex_request.watch_id)
        )
        folder = result.scalar_one_or_none()
        if not folder:
            raise NotFoundError(f"Watch directory {reindex_request.watch_id} not found")

        if hasattr(request.app.state, "watch_service"):
            watch_service: WatchService = request.app.state.watch_service
            if reindex_request.watch_id in watch_service.watchers:
                watcher = watch_service.watchers[reindex_request.watch_id]
                background_tasks.add_task(watcher.scan_directory, Path(folder.path))

        return {
            "message": f"Reindex triggered for {folder.path}",
            "watch_id": str(reindex_request.watch_id),
        }
    return {"message": "Reindex all not implemented", "watch_id": None}


@router.post("/{watch_id}/pause", status_code=204)
async def pause_watch(
    request: Request,
    watch_id: UUID,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
) -> None:
    """Pause watching a directory."""
    result = await db.execute(select(WatchFolder).where(WatchFolder.id == watch_id))
    folder = result.scalar_one_or_none()
    if not folder:
        raise NotFoundError(f"Watch directory {watch_id} not found")

    if hasattr(request.app.state, "watch_service"):
        watch_service: WatchService = request.app.state.watch_service
        await watch_service.stop_watching(watch_id)

    await db.execute(update(WatchFolder).where(WatchFolder.id == watch_id).values(active=False))
    await db.commit()
    logger.info(f"Paused watching directory: {folder.path}")


@router.post("/{watch_id}/resume", status_code=204)
async def resume_watch(
    request: Request,
    watch_id: UUID,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
) -> None:
    """Resume watching a directory."""
    result = await db.execute(select(WatchFolder).where(WatchFolder.id == watch_id))
    folder = result.scalar_one_or_none()
    if not folder:
        raise NotFoundError(f"Watch directory {watch_id} not found")

    await db.execute(update(WatchFolder).where(WatchFolder.id == watch_id).values(active=True))
    await db.commit()

    if hasattr(request.app.state, "watch_service"):
        watch_service: WatchService = request.app.state.watch_service
        await watch_service.start_watching(
            watch_folder_id=watch_id, path=folder.path, recursive=folder.recursive, db_session=db
        )

    logger.info(f"Resumed watching directory: {folder.path}")
