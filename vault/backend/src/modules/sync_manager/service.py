"""Sync service for multi-device synchronization."""

from datetime import UTC, datetime, timedelta
import logging

from sqlalchemy import and_, or_, select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy.orm import joinedload

from ...models.sync import (
    Conflict,
    ConflictStatus,
    Device,
    Document,
    SyncAction,
    SyncLog,
    SyncState,
)
from ...schemas.sync import (
    ConflictResponse,
    DocumentChange,
    PullResponse,
    PushResponse,
    SyncActionEnum,
)

logger = logging.getLogger(__name__)


class SyncService:
    """Service for handling document synchronization across devices.

    Implements a simple timestamp-based sync protocol with last-write-wins
    conflict resolution and conflict detection.
    """

    # Time threshold for conflict detection (5 minutes)
    CONFLICT_THRESHOLD_SECONDS = 300

    def __init__(self, db: AsyncSession):
        """Initialize sync service.

        Args:
            db: Async database session
        """
        self.db = db

    async def register_device(self, user_id: int, device_id: str, device_name: str) -> Device:
        """Register a new device for a user.

        Args:
            user_id: User ID
            device_id: Unique device identifier from client
            device_name: User-friendly device name

        Returns:
            Created Device model

        Raises:
            ValueError: If device_id already exists for different user
        """
        # Check if device already exists
        result = await self.db.execute(select(Device).where(Device.device_id == device_id))
        existing = result.scalar_one_or_none()

        if existing:
            if existing.user_id != user_id:
                raise ValueError(f"Device {device_id} already registered to different user")

            # Update last seen
            existing.last_seen_at = datetime.now(UTC)
            existing.device_name = device_name
            await self.db.commit()
            await self.db.refresh(existing)
            logger.info(f"Device {device_id} updated for user {user_id}")
            return existing

        # Create new device
        device = Device(
            user_id=user_id,
            device_id=device_id,
            device_name=device_name,
            last_seen_at=datetime.now(UTC),
        )
        self.db.add(device)
        await self.db.commit()
        await self.db.refresh(device)

        logger.info(f"New device {device_id} registered for user {user_id}")
        return device

    async def pull_changes(
        self, user_id: int, device_id: str, since_timestamp: datetime | None = None
    ) -> PullResponse:
        """Pull changes from server since given timestamp.

        Args:
            user_id: User ID requesting sync
            device_id: Device ID requesting sync
            since_timestamp: Get changes since this time (None for first sync)

        Returns:
            PullResponse with changes and conflicts
        """
        # Get device
        device = await self._get_device(device_id, user_id)

        # Update last seen
        device.last_seen_at = datetime.now(UTC)

        # Get documents for sync: both active modifications AND recent deletions
        # This implements the hybrid soft delete + event log pattern
        current_time = datetime.now(UTC)

        query = select(Document).where(
            and_(
                Document.user_id == user_id,
                or_(
                    # Active modified documents (not deleted)
                    and_(
                        Document.deleted_at.is_(None),
                        or_(
                            Document.last_modified_device_id.is_(None),  # Orphaned docs
                            Document.last_modified_device_id
                            != device.id,  # Modified by other devices
                        ),
                    ),
                    # Recently deleted documents (within cleanup window)
                    # Exclude deletions by requesting device (no need to echo back)
                    and_(
                        Document.deleted_at.isnot(None),
                        Document.cleanup_after > current_time,  # Not yet expired
                        Document.deleted_by_device_id != device.id,  # Don't return own deletions
                    ),
                ),
            )
        )

        # Apply timestamp filter if provided
        if since_timestamp:
            query = query.where(
                or_(
                    # For active docs, check modified_at
                    and_(Document.deleted_at.is_(None), Document.modified_at > since_timestamp),
                    # For deleted docs, check deleted_at
                    and_(Document.deleted_at.isnot(None), Document.deleted_at > since_timestamp),
                )
            )

        query = query.order_by(Document.modified_at)

        result = await self.db.execute(query)
        changed_docs = result.scalars().all()

        # Get pending conflicts for this user
        conflicts_query = (
            select(Conflict)
            .join(Document)
            .where(and_(Document.user_id == user_id, Conflict.status == ConflictStatus.PENDING))
            .options(joinedload(Conflict.document))
        )

        conflicts_result = await self.db.execute(conflicts_query)
        conflicts = conflicts_result.scalars().all()

        await self.db.commit()

        # Convert to response format
        changes = [self._document_to_change(doc) for doc in changed_docs]
        conflict_responses = [self._conflict_to_response(c) for c in conflicts]

        logger.info(
            f"Pull for device {device_id}: {len(changes)} changes, {len(conflicts)} conflicts"
        )

        return PullResponse(
            changes=changes,
            conflicts=conflict_responses,
            new_timestamp=datetime.now(UTC),
            total_changes=len(changes),
        )

    async def push_changes(
        self, user_id: int, device_id: str, changes: list[DocumentChange]
    ) -> PushResponse:
        """Push changes from device to server.

        Args:
            user_id: User ID pushing changes
            device_id: Device ID pushing changes
            changes: List of document changes

        Returns:
            PushResponse with accepted changes and conflicts
        """
        # Get device
        device = await self._get_device(device_id, user_id)

        # Update last seen
        device.last_seen_at = datetime.now(UTC)

        accepted_paths: list[str] = []
        detected_conflicts: list[Conflict] = []

        for change in changes:
            # Use savepoint for each change to prevent partial commits
            try:
                async with self.db.begin_nested():
                    # Check if document exists on server
                    existing = await self._get_document_by_path(user_id, change.path)

                    if change.action == SyncActionEnum.CREATE:
                        if existing:
                            # Conflict: trying to create existing document
                            conflict = await self._create_conflict(
                                existing, change.version, change.modified_at, change.content_hash
                            )
                            detected_conflicts.append(conflict)
                        else:
                            # Create new document
                            await self._create_document(device, change)
                            accepted_paths.append(change.path)

                    elif change.action == SyncActionEnum.UPDATE:
                        if not existing:
                            # Document doesn't exist, treat as create
                            await self._create_document(device, change)
                            accepted_paths.append(change.path)
                        elif await self._has_conflict(existing, change):
                            # Conflict detected
                            conflict = await self._create_conflict(
                                existing, change.version, change.modified_at, change.content_hash
                            )
                            detected_conflicts.append(conflict)
                        else:
                            # No conflict, update document
                            await self._update_document(device, existing, change)
                            accepted_paths.append(change.path)

                    elif change.action == SyncActionEnum.DELETE:
                        if existing:
                            # Check if already soft-deleted
                            if existing.deleted_at is not None:
                                # Already deleted, that's fine
                                accepted_paths.append(change.path)
                            else:
                                # Log BEFORE soft delete to maintain audit trail
                                await self._log_sync(
                                    device, existing, SyncAction.DELETE, existing.version
                                )

                                # Soft delete: mark as deleted but keep in database
                                existing.deleted_at = datetime.now(UTC)
                                existing.deleted_by_device_id = device.id
                                existing.cleanup_after = datetime.now(UTC) + timedelta(days=30)
                                existing.version += 1  # Increment version for soft delete
                                existing.last_modified_device_id = device.id

                                await self.db.flush()
                                accepted_paths.append(change.path)
                        else:
                            # Document doesn't exist, treat as already deleted
                            accepted_paths.append(change.path)

            except IntegrityError as e:
                # Handle race condition: concurrent path creation after deletion
                # Two devices try to create same path simultaneously
                if "uq_document_user_path_active" in str(e):
                    logger.warning(
                        f"Integrity error on {change.path} - likely concurrent creation. "
                        f"Creating conflict. Error: {e}"
                    )
                    # Rollback savepoint
                    await self.db.rollback()

                    # Fetch the document that was just created by concurrent request
                    existing = await self._get_document_by_path(user_id, change.path)
                    if existing:
                        # Create conflict for user to resolve
                        conflict = await self._create_conflict(
                            existing, change.version, change.modified_at, change.content_hash
                        )
                        detected_conflicts.append(conflict)
                    else:
                        # Unexpected: index violation but no document found
                        logger.error(
                            f"IntegrityError on {change.path} but no document found. "
                            f"This shouldn't happen. Error: {e}"
                        )
                else:
                    # Other integrity errors should fail loudly
                    raise

            except Exception as e:
                logger.error(f"Error processing change for {change.path}: {e}", exc_info=True)
                # Savepoint rolled back, continue with other changes
                continue

        await self.db.commit()

        conflict_responses = [self._conflict_to_response(c) for c in detected_conflicts]

        logger.info(
            f"Push from device {device_id}: {len(accepted_paths)} accepted, "
            f"{len(detected_conflicts)} conflicts"
        )

        return PushResponse(
            accepted=accepted_paths,
            conflicts=conflict_responses,
            timestamp=datetime.now(UTC),
            total_accepted=len(accepted_paths),
            total_conflicts=len(detected_conflicts),
        )

    async def resolve_conflict(
        self,
        user_id: int,
        conflict_id: int,
        resolution: ConflictStatus,
        merged_content: str | None = None,
    ) -> None:
        """Resolve a sync conflict.

        Args:
            user_id: User ID resolving conflict
            conflict_id: Conflict ID to resolve
            resolution: How to resolve (local, remote, or merge)
            merged_content: Required if resolution is merge

        Raises:
            ValueError: If conflict not found or resolution invalid
        """
        # Get conflict
        result = await self.db.execute(
            select(Conflict)
            .join(Document)
            .where(and_(Conflict.id == conflict_id, Document.user_id == user_id))
            .options(joinedload(Conflict.document))
        )
        conflict = result.scalar_one_or_none()

        if not conflict:
            raise ValueError(f"Conflict {conflict_id} not found")

        if conflict.status != ConflictStatus.PENDING:
            raise ValueError(f"Conflict {conflict_id} already resolved")

        # Update conflict status
        conflict.status = resolution
        conflict.resolved_at = datetime.now(UTC)

        # Apply resolution to document if needed
        if resolution == ConflictStatus.RESOLVED_MERGE:
            if not merged_content:
                raise ValueError("Merged content required for merge resolution")
            conflict.document.content = merged_content
            conflict.document.version += 1
            conflict.document.modified_at = datetime.now(UTC)

        # Mark document as synced after conflict resolution
        conflict.document.sync_state = SyncState.SYNCED

        await self.db.commit()

        logger.info(f"Conflict {conflict_id} resolved as {resolution}")

    async def cleanup_expired_deletions(self, batch_size: int = 100) -> int:
        """Clean up soft-deleted documents that have expired.

        This is a background job that hard-deletes documents where cleanup_after
        has passed. Implements the hybrid soft delete + event log pattern by
        keeping deleted documents for a grace period before permanent removal.

        Args:
            batch_size: Maximum number of documents to delete in one call

        Returns:
            Number of documents permanently deleted

        Note:
            Related records (sync_logs, conflicts) cascade delete automatically.
        """
        current_time = datetime.now(UTC)

        try:
            async with self.db.begin_nested():  # Use savepoint for safety
                # Find expired soft-deleted documents
                query = (
                    select(Document)
                    .where(
                        and_(
                            Document.deleted_at.isnot(None), Document.cleanup_after <= current_time
                        )
                    )
                    .limit(batch_size)
                )

                result = await self.db.execute(query)
                expired_docs = result.scalars().all()

                if not expired_docs:
                    return 0

                # Hard delete each document
                deleted_count = 0
                for doc in expired_docs:
                    logger.info(
                        f"Cleaning up expired deletion: document_id={doc.id}, "
                        f"path={doc.path}, deleted_at={doc.deleted_at}, "
                        f"cleanup_after={doc.cleanup_after}"
                    )
                    await self.db.delete(doc)
                    deleted_count += 1

                await self.db.flush()

                logger.info(f"Cleaned up {deleted_count} expired soft-deleted documents")
                return deleted_count

        except Exception as e:
            logger.error(f"Error during cleanup of old deletions: {e}", exc_info=True)
            # Savepoint rolled back automatically
            raise

    async def get_sync_status(self, user_id: int, device_id: str) -> dict:
        """Get sync status for a device.

        Args:
            user_id: User ID
            device_id: Device ID

        Returns:
            Sync status information
        """
        device = await self._get_device(device_id, user_id)

        # Get last sync times from sync log
        last_pull_result = await self.db.execute(
            select(SyncLog)
            .where(SyncLog.device_id == device.id)
            .order_by(SyncLog.timestamp.desc())
            .limit(1)
        )
        last_sync = last_pull_result.scalar_one_or_none()

        # Count pending conflicts
        conflicts_count_result = await self.db.execute(
            select(Conflict)
            .join(Document)
            .where(and_(Document.user_id == user_id, Conflict.status == ConflictStatus.PENDING))
        )
        pending_conflicts = len(conflicts_count_result.scalars().all())

        # Count synced documents
        docs_count_result = await self.db.execute(
            select(Document).where(Document.user_id == user_id)
        )
        synced_documents = len(docs_count_result.scalars().all())

        return {
            "device_id": device_id,
            "last_pull_timestamp": last_sync.timestamp if last_sync else None,
            "last_push_timestamp": last_sync.timestamp if last_sync else None,
            "pending_conflicts": pending_conflicts,
            "synced_documents": synced_documents,
        }

    # Private helper methods

    async def _get_device(self, device_id: str, user_id: int) -> Device:
        """Get device by ID and verify ownership."""
        result = await self.db.execute(
            select(Device).where(and_(Device.device_id == device_id, Device.user_id == user_id))
        )
        device = result.scalar_one_or_none()

        if not device:
            raise ValueError(f"Device {device_id} not found for user {user_id}")

        return device

    async def _get_document_by_path(self, user_id: int, path: str) -> Document | None:
        """Get ACTIVE (non-deleted) document by user ID and path.

        Only returns documents where deleted_at IS NULL, allowing path reuse
        immediately after soft deletion without waiting for cleanup.
        """
        result = await self.db.execute(
            select(Document).where(
                and_(
                    Document.user_id == user_id,
                    Document.path == path,
                    Document.deleted_at.is_(None),  # Only find active documents
                )
            )
        )
        return result.scalar_one_or_none()

    async def _has_conflict(self, existing: Document, change: DocumentChange) -> bool:
        """Check if change conflicts with existing document.

        Conflict occurs if:
        1. Server modified >5min AFTER client (server is newer, reject client)
        2. Timestamps within 5min AND content hashes differ (concurrent edits)

        No conflict (last-write-wins):
        - Client modified >5min AFTER server (client is newer, accept)
        """
        # Calculate time difference (positive = server newer, negative = client newer)
        time_diff = (existing.modified_at - change.modified_at).total_seconds()

        # Server is significantly newer (>5min) - CONFLICT: reject client update
        if time_diff > self.CONFLICT_THRESHOLD_SECONDS:
            logger.warning(
                f"Conflict detected for {change.path}: server is {time_diff}s newer than client"
            )
            return True

        # Client is significantly newer (>5min) - NO CONFLICT: accept (last-write-wins)
        if time_diff < -self.CONFLICT_THRESHOLD_SECONDS:
            logger.info(
                f"No conflict for {change.path}: "
                f"client is {abs(time_diff)}s newer (last-write-wins)"
            )
            return False

        # Timestamps within 5min - check content hash for concurrent edits
        if existing.content_hash and change.content_hash:
            if existing.content_hash != change.content_hash:
                logger.warning(
                    f"Conflict detected for {change.path}: "
                    f"time_diff={time_diff}s, hashes differ (concurrent edit)"
                )
                return True

        # Within 5min but same content - NO CONFLICT
        return False

    async def _create_conflict(
        self,
        document: Document,
        local_version: int,
        local_modified_at: datetime,
        local_content_hash: str | None,
    ) -> Conflict:
        """Create a conflict record and mark document as conflicted."""
        conflict = Conflict(
            document_id=document.id,
            local_version=local_version,
            remote_version=document.version,
            local_modified_at=local_modified_at,
            remote_modified_at=document.modified_at,
            local_content_hash=local_content_hash,
            remote_content_hash=document.content_hash,
            status=ConflictStatus.PENDING,
        )
        self.db.add(conflict)

        # Mark document as in conflict state
        document.sync_state = SyncState.CONFLICT

        await self.db.flush()
        return conflict

    async def _create_document(self, device: Device, change: DocumentChange) -> Document:
        """Create new document from change."""
        document = Document(
            user_id=device.user_id,
            device_id=device.id,
            last_modified_device_id=device.id,
            path=change.path,
            title=change.title,
            content=change.content,
            content_hash=change.content_hash,
            version=1,  # Server controls version
            modified_at=change.modified_at,
            sync_state=SyncState.SYNCED,  # New document is synced
        )
        self.db.add(document)
        await self.db.flush()
        await self._log_sync(device, document, SyncAction.CREATE, document.version)
        return document

    async def _update_document(
        self, device: Device, document: Document, change: DocumentChange
    ) -> None:
        """Update existing document from change."""
        document.title = change.title
        document.content = change.content
        document.content_hash = change.content_hash
        document.version += 1  # Server controls version
        document.last_modified_device_id = device.id
        document.modified_at = change.modified_at
        document.sync_state = SyncState.SYNCED  # Mark as synced after successful update
        await self.db.flush()
        await self._log_sync(device, document, SyncAction.UPDATE, document.version)

    async def _get_device_by_id(self, device_id: int) -> Device:
        """Get device by internal ID."""
        result = await self.db.execute(select(Device).where(Device.id == device_id))
        return result.scalar_one()

    async def _log_sync(
        self, device: Device, document: Document, action: SyncAction, version: int
    ) -> None:
        """Log a sync operation."""
        sync_log = SyncLog(
            device_id=device.id,
            document_id=document.id,
            document_path=document.path,  # CRITICAL: Required for orphaned logs after hard delete
            action=action,
            version=version,
        )
        self.db.add(sync_log)
        await self.db.flush()

    def _document_to_change(self, doc: Document) -> DocumentChange:
        """Convert Document model to DocumentChange schema.

        Deleted documents are returned with action=DELETE and deletion metadata.
        Active documents are returned with action=UPDATE.
        """
        # Determine action based on deletion status
        action = SyncActionEnum.DELETE if doc.deleted_at is not None else SyncActionEnum.UPDATE

        return DocumentChange(
            id=doc.id,
            action=action,
            path=doc.path,
            title=doc.title,
            content=doc.content,
            content_hash=doc.content_hash,
            modified_at=doc.modified_at,
            version=doc.version,
            deleted_at=doc.deleted_at,
            deleted_by_device_id=doc.deleted_by_device_id,
        )

    def _conflict_to_response(self, conflict: Conflict) -> ConflictResponse:
        """Convert Conflict model to ConflictResponse schema."""
        return ConflictResponse(
            id=conflict.id,
            document_id=conflict.document_id,
            local_version=conflict.local_version,
            remote_version=conflict.remote_version,
            local_modified_at=conflict.local_modified_at,
            remote_modified_at=conflict.remote_modified_at,
            local_content_hash=conflict.local_content_hash,
            remote_content_hash=conflict.remote_content_hash,
            status=conflict.status,
            created_at=conflict.created_at,
        )
