"""Sync API endpoints."""

from __future__ import annotations

import logging

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy.ext.asyncio import AsyncSession

from ...auth.dependencies import get_current_user
from ...auth.models import User
from ...db import get_session
from ...modules.sync_manager import SyncService
from ...schemas.sync import (
    ConflictResolution,
    DeviceCreate,
    DeviceResponse,
    PullRequest,
    PullResponse,
    PushRequest,
    PushResponse,
    SyncStatus,
)

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/sync", tags=["sync"])


def get_sync_service(db: AsyncSession = Depends(get_session)) -> SyncService:
    """Dependency to get sync service instance."""
    return SyncService(db)


@router.post("/devices", response_model=DeviceResponse, status_code=status.HTTP_201_CREATED)
async def register_device(
    device_data: DeviceCreate,
    current_user: User = Depends(get_current_user),
    sync_service: SyncService = Depends(get_sync_service),
) -> DeviceResponse:
    """Register a new device for the current user.

    Args:
        device_data: Device registration data
        current_user: Authenticated user
        sync_service: Sync service instance

    Returns:
        Created device information

    Raises:
        HTTPException: If device registration fails
    """
    try:
        device = await sync_service.register_device(
            user_id=current_user.id,
            device_id=device_data.device_id,
            device_name=device_data.device_name,
        )

        logger.info(f"Device {device.device_id} registered for user {current_user.id}")

        return DeviceResponse.model_validate(device)

    except ValueError as e:
        logger.error(f"Device registration failed: {e}")
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(e))
    except Exception as e:
        logger.error(f"Unexpected error in device registration: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR, detail="Failed to register device"
        )


@router.post("/pull", response_model=PullResponse)
async def pull_changes(
    pull_request: PullRequest,
    current_user: User = Depends(get_current_user),
    sync_service: SyncService = Depends(get_sync_service),
) -> PullResponse:
    """Pull changes from server since given timestamp.

    Desktop clients call this endpoint to get changes made by other devices.

    Args:
        pull_request: Pull request with device ID and timestamp
        current_user: Authenticated user
        sync_service: Sync service instance

    Returns:
        Changes and conflicts since the requested timestamp

    Raises:
        HTTPException: If pull fails
    """
    try:
        response = await sync_service.pull_changes(
            user_id=current_user.id,
            device_id=pull_request.device_id,
            since_timestamp=pull_request.since_timestamp,
        )

        logger.info(
            f"Pull completed for device {pull_request.device_id}: "
            f"{response.total_changes} changes, {len(response.conflicts)} conflicts"
        )

        return response

    except ValueError as e:
        logger.error(f"Pull failed: {e}")
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(e))
    except Exception as e:
        logger.error(f"Unexpected error in pull: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR, detail="Failed to pull changes"
        )


@router.post("/push", response_model=PushResponse)
async def push_changes(
    push_request: PushRequest,
    current_user: User = Depends(get_current_user),
    sync_service: SyncService = Depends(get_sync_service),
) -> PushResponse:
    """Push local changes to server.

    Desktop clients call this endpoint to upload their local changes.

    Args:
        push_request: Push request with device ID and changes
        current_user: Authenticated user
        sync_service: Sync service instance

    Returns:
        Results with accepted changes and detected conflicts

    Raises:
        HTTPException: If push fails
    """
    try:
        response = await sync_service.push_changes(
            user_id=current_user.id,
            device_id=push_request.device_id,
            changes=push_request.changes,
        )

        logger.info(
            f"Push completed for device {push_request.device_id}: "
            f"{response.total_accepted} accepted, {response.total_conflicts} conflicts"
        )

        return response

    except ValueError as e:
        logger.error(f"Push failed: {e}")
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(e))
    except Exception as e:
        logger.error(f"Unexpected error in push: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR, detail="Failed to push changes"
        )


@router.post("/conflicts/resolve", status_code=status.HTTP_204_NO_CONTENT)
async def resolve_conflict(
    resolution: ConflictResolution,
    current_user: User = Depends(get_current_user),
    sync_service: SyncService = Depends(get_sync_service),
) -> None:
    """Resolve a sync conflict.

    Args:
        resolution: Conflict resolution data
        current_user: Authenticated user
        sync_service: Sync service instance

    Raises:
        HTTPException: If resolution fails
    """
    try:
        await sync_service.resolve_conflict(
            user_id=current_user.id,
            conflict_id=resolution.conflict_id,
            resolution=resolution.resolution,
            merged_content=resolution.merged_content,
        )

        logger.info(f"Conflict {resolution.conflict_id} resolved by user {current_user.id}")

    except ValueError as e:
        logger.error(f"Conflict resolution failed: {e}")
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(e))
    except Exception as e:
        logger.error(f"Unexpected error in conflict resolution: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR, detail="Failed to resolve conflict"
        )


@router.get("/status", response_model=SyncStatus)
async def get_sync_status(
    device_id: str,
    current_user: User = Depends(get_current_user),
    sync_service: SyncService = Depends(get_sync_service),
) -> SyncStatus:
    """Get sync status for a device.

    Args:
        device_id: Device ID to check status for
        current_user: Authenticated user
        sync_service: Sync service instance

    Returns:
        Sync status information

    Raises:
        HTTPException: If status check fails
    """
    try:
        status_data = await sync_service.get_sync_status(
            user_id=current_user.id,
            device_id=device_id,
        )

        return SyncStatus(**status_data)

    except ValueError as e:
        logger.error(f"Status check failed: {e}")
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(e))
    except Exception as e:
        logger.error(f"Unexpected error in status check: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR, detail="Failed to get sync status"
        )


@router.get("/health")
async def sync_health() -> dict:
    """Health check endpoint for sync service.

    Returns:
        Health status
    """
    return {
        "status": "healthy",
        "service": "sync",
    }
