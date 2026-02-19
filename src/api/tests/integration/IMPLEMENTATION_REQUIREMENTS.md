# Implementation Requirements for Deletion Propagation Tests

## Overview

This document specifies **exactly what needs to be implemented** for all deletion propagation tests to pass.

Based on: **Option A+B Hybrid** from `DELETION_PROTOCOL_LIMITATION.md`

---

## 1. Database Schema Changes

### Documents Table
```sql
-- Add soft delete columns
ALTER TABLE documents
ADD COLUMN deleted_at TIMESTAMP NULL,
ADD COLUMN deleted_by_device_id INTEGER NULL REFERENCES devices(id),
ADD COLUMN cleanup_after TIMESTAMP NULL;

-- Add index for cleanup job performance
CREATE INDEX idx_documents_cleanup_after
ON documents(cleanup_after)
WHERE cleanup_after IS NOT NULL;

-- Add index for pull queries
CREATE INDEX idx_documents_deleted_at
ON documents(deleted_at)
WHERE deleted_at IS NOT NULL;
```

### SyncLogs Table
```sql
-- Add document_path column
ALTER TABLE sync_logs
ADD COLUMN document_path VARCHAR(1024);

-- Make document_id nullable (for orphaned logs after cleanup)
ALTER TABLE sync_logs
ALTER COLUMN document_id DROP NOT NULL;

-- Add index for orphaned logs
CREATE INDEX idx_sync_logs_orphaned
ON sync_logs(document_path)
WHERE document_id IS NULL;
```

---

## 2. Model Updates

### Document Model (`src/models/sync.py`)

```python
class Document(Base):
    __tablename__ = "documents"

    # Existing fields...
    id: Mapped[int] = mapped_column(primary_key=True)
    user_id: Mapped[int] = mapped_column(...)
    path: Mapped[str] = mapped_column(...)
    # ... other existing fields ...

    # NEW FIELDS FOR SOFT DELETE
    deleted_at: Mapped[Optional[datetime]] = mapped_column(
        DateTime,
        nullable=True,
        index=True,
        default=None
    )
    deleted_by_device_id: Mapped[Optional[int]] = mapped_column(
        Integer,
        ForeignKey("devices.id", ondelete="SET NULL"),
        nullable=True
    )
    cleanup_after: Mapped[Optional[datetime]] = mapped_column(
        DateTime,
        nullable=True,
        index=True,
        default=None
    )

    # NEW RELATIONSHIP
    deleted_by_device: Mapped[Optional["Device"]] = relationship(
        "Device",
        foreign_keys=[deleted_by_device_id]
    )
```

### SyncLog Model (`src/models/sync.py`)

```python
class SyncLog(Base):
    __tablename__ = "sync_logs"

    # Existing fields...
    id: Mapped[int] = mapped_column(primary_key=True)
    device_id: Mapped[int] = mapped_column(...)

    # MODIFIED: Make nullable for orphaned logs
    document_id: Mapped[Optional[int]] = mapped_column(
        Integer,
        ForeignKey("documents.id"),
        nullable=True,  # Changed from False
        index=True
    )

    # NEW FIELD: Preserve path for orphaned logs
    document_path: Mapped[Optional[str]] = mapped_column(
        String(1024),
        nullable=True
    )

    action: Mapped[SyncAction] = mapped_column(...)
    timestamp: Mapped[datetime] = mapped_column(...)
    version: Mapped[int] = mapped_column(...)
```

---

## 3. Schema Updates

### DocumentChange Schema (`src/schemas/sync.py`)

```python
class DocumentChange(BaseModel):
    """Schema for document changes in sync operations."""

    # Existing fields...
    id: Optional[int] = None
    action: SyncActionEnum
    path: str
    title: Optional[str] = None
    content: Optional[str] = None
    content_hash: Optional[str] = None
    modified_at: datetime
    version: int

    # NEW FIELDS FOR DELETION PROPAGATION
    deleted_at: Optional[datetime] = None
    deleted_by_device_id: Optional[int] = None
    cleanup_after: Optional[datetime] = None

    @property
    def is_deleted(self) -> bool:
        """Check if this change represents a deletion."""
        return self.deleted_at is not None or self.action == SyncActionEnum.DELETE
```

---

## 4. Service Layer Changes

### SyncService.push_changes() - DELETE Action Handler

**File**: `src/modules/sync_manager/service.py`

```python
async def push_changes(...) -> PushResponse:
    """Push changes from device to server."""

    for change in changes:
        if change.action == SyncActionEnum.DELETE:
            if existing:
                # SOFT DELETE (don't hard delete)
                now = datetime.now(timezone.utc)

                # Set soft delete fields
                existing.deleted_at = now
                existing.deleted_by_device_id = device.id
                existing.cleanup_after = now + timedelta(days=30)  # 30-day window

                # Create sync log with path BEFORE potential cleanup
                await self._log_sync(
                    device=device,
                    document=existing,
                    action=SyncAction.DELETE,
                    version=existing.version,
                    document_path=change.path  # NEW: Pass path
                )

                accepted_paths.append(change.path)
            else:
                # Already deleted or never existed - idempotent
                accepted_paths.append(change.path)
```

### SyncService.pull_changes() - Include Deleted Docs

```python
async def pull_changes(...) -> PullResponse:
    """Pull changes from server since given timestamp."""

    # Modified query to include soft-deleted documents within window
    query = select(Document).where(
        and_(
            Document.user_id == user_id,
            or_(
                Document.last_modified_device_id.is_(None),  # Orphaned
                Document.last_modified_device_id != device.id  # Other devices
            )
        )
    )

    if since_timestamp:
        # Include both modified and recently deleted documents
        query = query.where(
            or_(
                # Regular modified documents (not deleted)
                and_(
                    Document.deleted_at.is_(None),
                    Document.modified_at > since_timestamp
                ),
                # Recently deleted documents (within window)
                and_(
                    Document.deleted_at.isnot(None),
                    Document.deleted_at > since_timestamp,
                    Document.cleanup_after > datetime.now(timezone.utc)  # Not expired
                )
            )
        )
    else:
        # First sync: exclude deleted documents
        query = query.where(Document.deleted_at.is_(None))

    # Convert to response format
    changes = [self._document_to_change(doc) for doc in changed_docs]
    # ...
```

### SyncService._document_to_change() - Include Deletion Fields

```python
def _document_to_change(self, doc: Document) -> DocumentChange:
    """Convert Document model to DocumentChange schema."""

    # Determine action based on deleted_at
    action = SyncActionEnum.DELETE if doc.deleted_at else SyncActionEnum.UPDATE

    return DocumentChange(
        id=doc.id,
        action=action,  # Changed: Can be DELETE
        path=doc.path,
        title=doc.title,
        content=doc.content if not doc.deleted_at else None,  # Clear content for deletes
        content_hash=doc.content_hash if not doc.deleted_at else None,
        modified_at=doc.modified_at,
        version=doc.version,
        # NEW: Include deletion fields
        deleted_at=doc.deleted_at,
        deleted_by_device_id=doc.deleted_by_device_id,
        cleanup_after=doc.cleanup_after,
    )
```

### SyncService._log_sync() - Accept document_path

```python
async def _log_sync(
    self,
    device: Device,
    document: Document,
    action: SyncAction,
    version: int,
    document_path: Optional[str] = None  # NEW parameter
) -> None:
    """Log a sync operation."""

    sync_log = SyncLog(
        device_id=device.id,
        document_id=document.id,
        document_path=document_path or document.path,  # NEW: Store path
        action=action,
        version=version,
    )
    self.db.add(sync_log)
    await self.db.flush()
```

---

## 5. NEW: Cleanup Job Implementation

### SyncService.cleanup_expired_deletions()

**File**: `src/modules/sync_manager/service.py`

```python
async def cleanup_expired_deletions(self) -> int:
    """Hard-delete documents past their cleanup window.

    This job should run daily to remove soft-deleted documents
    that have exceeded their 30-day propagation window.

    Returns:
        Number of documents hard-deleted
    """
    now = datetime.now(timezone.utc)

    # Find expired soft-deleted documents
    query = select(Document).where(
        and_(
            Document.deleted_at.isnot(None),  # Is soft-deleted
            Document.cleanup_after <= now      # Past cleanup window
        )
    )

    result = await self.db.execute(query)
    expired_docs = result.scalars().all()

    # IMPORTANT: Update sync logs BEFORE deleting documents
    # This orphans the sync logs (document_id -> NULL) but preserves document_path
    for doc in expired_docs:
        # Update sync logs to orphaned state
        await self.db.execute(
            update(SyncLog)
            .where(SyncLog.document_id == doc.id)
            .values(document_id=None)
            # document_path already set, preserved
        )

        # Hard delete the document
        await self.db.delete(doc)

    await self.db.commit()

    logger.info(f"Cleanup job: hard-deleted {len(expired_docs)} expired documents")
    return len(expired_docs)
```

### Background Job (Celery/APScheduler)

```python
# src/tasks/cleanup.py
from celery import shared_task
from src.db.session import async_session
from src.modules.sync_manager.service import SyncService

@shared_task
async def cleanup_expired_deletions_task():
    """Daily cleanup job for expired soft-deleted documents."""
    async with async_session() as session:
        service = SyncService(session)
        deleted_count = await service.cleanup_expired_deletions()
        return {"deleted_count": deleted_count}

# Schedule: Run daily at 2 AM
# Celery Beat configuration
from celery.schedules import crontab

app.conf.beat_schedule = {
    'cleanup-expired-deletions': {
        'task': 'src.tasks.cleanup.cleanup_expired_deletions_task',
        'schedule': crontab(hour=2, minute=0),  # 2:00 AM daily
    },
}
```

---

## 6. Conflict Detection Enhancement

### _has_conflict() - Detect Modify After Delete

```python
async def _has_conflict(self, existing: Document, change: DocumentChange) -> bool:
    """Check if change conflicts with existing document."""

    # NEW: Check if trying to modify a deleted document
    if existing.deleted_at and change.action == SyncActionEnum.UPDATE:
        logger.warning(
            f"Conflict detected for {change.path}: "
            f"attempting to modify deleted document"
        )
        return True

    # Existing conflict detection logic...
    time_diff = (existing.modified_at - change.modified_at).total_seconds()
    # ...
```

### _create_conflict() - Include Deletion Info

```python
async def _create_conflict(
    self,
    document: Document,
    local_version: int,
    local_modified_at: datetime,
    local_content_hash: Optional[str]
) -> Conflict:
    """Create a conflict record."""

    # Determine conflict description
    if document.deleted_at:
        description = (
            f"Document was deleted on {document.deleted_at.isoformat()} "
            f"but client attempted to modify it"
        )
    else:
        description = "Concurrent modification conflict"

    conflict = Conflict(
        document_id=document.id,
        local_version=local_version,
        remote_version=document.version,
        local_modified_at=local_modified_at,
        remote_modified_at=document.modified_at,
        local_content_hash=local_content_hash,
        remote_content_hash=document.content_hash,
        status=ConflictStatus.PENDING,
        description=description,  # NEW field
    )
    self.db.add(conflict)
    await self.db.flush()
    return conflict
```

---

## 7. Database Migration

### Alembic Migration File

```python
"""Add soft delete support for deletion propagation.

Revision ID: add_soft_delete
Revises: previous_revision
Create Date: 2025-01-15

"""
from alembic import op
import sqlalchemy as sa
from sqlalchemy.dialects import postgresql

# revision identifiers
revision = 'add_soft_delete'
down_revision = 'previous_revision'
branch_labels = None
depends_on = None


def upgrade() -> None:
    # Documents table changes
    op.add_column('documents', sa.Column('deleted_at', sa.DateTime(), nullable=True))
    op.add_column('documents', sa.Column('deleted_by_device_id', sa.Integer(), nullable=True))
    op.add_column('documents', sa.Column('cleanup_after', sa.DateTime(), nullable=True))

    op.create_foreign_key(
        'fk_documents_deleted_by_device',
        'documents', 'devices',
        ['deleted_by_device_id'], ['id'],
        ondelete='SET NULL'
    )

    op.create_index(
        'idx_documents_deleted_at',
        'documents',
        ['deleted_at'],
        postgresql_where=sa.text('deleted_at IS NOT NULL')
    )

    op.create_index(
        'idx_documents_cleanup_after',
        'documents',
        ['cleanup_after'],
        postgresql_where=sa.text('cleanup_after IS NOT NULL')
    )

    # SyncLogs table changes
    op.add_column('sync_logs', sa.Column('document_path', sa.String(1024), nullable=True))
    op.alter_column('sync_logs', 'document_id', nullable=True)

    op.create_index(
        'idx_sync_logs_orphaned',
        'sync_logs',
        ['document_path'],
        postgresql_where=sa.text('document_id IS NULL')
    )


def downgrade() -> None:
    # Drop indexes
    op.drop_index('idx_sync_logs_orphaned', table_name='sync_logs')
    op.drop_index('idx_documents_cleanup_after', table_name='documents')
    op.drop_index('idx_documents_deleted_at', table_name='documents')

    # Revert SyncLogs changes
    op.drop_column('sync_logs', 'document_path')
    op.alter_column('sync_logs', 'document_id', nullable=False)

    # Revert Documents changes
    op.drop_constraint('fk_documents_deleted_by_device', 'documents', type_='foreignkey')
    op.drop_column('documents', 'cleanup_after')
    op.drop_column('documents', 'deleted_by_device_id')
    op.drop_column('documents', 'deleted_at')
```

---

## 8. Configuration

### Add Cleanup Window Configuration

```python
# src/config/settings.py
class SyncSettings(BaseSettings):
    """Sync service settings."""

    # Existing settings...

    # NEW: Deletion propagation settings
    deletion_cleanup_window_days: int = Field(
        default=30,
        description="Days to keep soft-deleted documents before hard delete"
    )

    deletion_cleanup_job_enabled: bool = Field(
        default=True,
        description="Enable automatic cleanup of expired deletions"
    )
```

---

## 9. Summary of Implementation Steps

### Step 1: Database Migration
1. Run Alembic migration to add columns
2. Verify schema changes with `psql`

### Step 2: Update Models
1. Add fields to `Document` model
2. Update `SyncLog` model
3. Update Pydantic schemas

### Step 3: Update Service Layer
1. Modify `push_changes()` DELETE handler
2. Modify `pull_changes()` query
3. Update `_document_to_change()`
4. Update `_log_sync()`
5. Add `cleanup_expired_deletions()`
6. Update `_has_conflict()`

### Step 4: Add Background Job
1. Create cleanup task
2. Configure scheduler (Celery Beat)
3. Test manual cleanup execution

### Step 5: Run Tests
```bash
pytest tests/integration/test_deletion_propagation.py -v
pytest tests/integration/test_sync.py::TestDeleteBehavior -v
```

### Step 6: Monitor & Tune
1. Add metrics for cleanup job
2. Monitor deletion propagation latency
3. Tune cleanup window if needed

---

## 10. Expected Test Results After Implementation

### ✅ All Tests Should Pass (24/24)

```bash
tests/integration/test_deletion_propagation.py::TestSoftDeleteOnPush::test_delete_sets_deleted_at_field PASSED
tests/integration/test_deletion_propagation.py::TestSoftDeleteOnPush::test_delete_sets_cleanup_after_30_days PASSED
tests/integration/test_deletion_propagation.py::TestSoftDeleteOnPush::test_delete_creates_sync_log_with_path PASSED
tests/integration/test_deletion_propagation.py::TestDeletionPropagation::test_deleted_doc_appears_in_pull_within_window PASSED
tests/integration/test_deletion_propagation.py::TestDeletionPropagation::test_two_device_deletion_propagation_scenario PASSED
tests/integration/test_deletion_propagation.py::TestDeletionPropagation::test_deletion_does_not_propagate_to_different_user PASSED
tests/integration/test_deletion_propagation.py::TestDeletionWindowExpiry::test_deleted_doc_not_in_pull_after_window PASSED
tests/integration/test_deletion_propagation.py::TestDeletionWindowExpiry::test_deleted_doc_in_pull_within_window PASSED
tests/integration/test_deletion_propagation.py::TestCleanupJob::test_cleanup_removes_expired_soft_deletes PASSED
tests/integration/test_deletion_propagation.py::TestCleanupJob::test_cleanup_preserves_non_expired_soft_deletes PASSED
tests/integration/test_deletion_propagation.py::TestCleanupJob::test_cleanup_preserves_sync_log_with_path PASSED
tests/integration/test_deletion_propagation.py::TestCleanupJob::test_cleanup_batch_processing PASSED
tests/integration/test_deletion_propagation.py::TestDeleteModifyConflicts::test_modify_after_delete_creates_conflict PASSED
tests/integration/test_deletion_propagation.py::TestDeleteModifyConflicts::test_delete_after_modify_accepted PASSED
tests/integration/test_deletion_propagation.py::TestOrphanedSyncLogs::test_pull_from_old_timestamp_shows_orphaned_deletion PASSED
tests/integration/test_deletion_propagation.py::TestOrphanedSyncLogs::test_orphaned_sync_log_preserves_all_metadata PASSED
tests/integration/test_deletion_propagation.py::TestDeletionEdgeCases::test_delete_already_deleted_document PASSED
tests/integration/test_deletion_propagation.py::TestDeletionEdgeCases::test_delete_nonexistent_document PASSED
tests/integration/test_deletion_propagation.py::TestDeletionEdgeCases::test_cleanup_window_boundary_exactly_30_days PASSED
tests/integration/test_deletion_propagation.py::TestDeletionEdgeCases::test_multiple_devices_delete_same_document PASSED
tests/integration/test_sync.py::TestPushChanges::test_push_delete_document PASSED
tests/integration/test_sync.py::TestDeleteBehavior::test_delete_logs_sync_properly PASSED
tests/integration/test_sync.py::TestDeleteBehavior::test_delete_soft_deletes_document PASSED
tests/integration/test_sync.py::TestTwoDeviceScenarios::test_device_a_deletes_device_b_pulls PASSED

======================== 24 passed in 12.34s ========================
```

---

## 11. Verification Checklist

Before considering implementation complete:

- [ ] Database migration applied successfully
- [ ] All model fields added with correct types
- [ ] Soft delete logic implemented in `push_changes()`
- [ ] Pull query includes soft-deleted documents within window
- [ ] Pull query excludes expired deletions
- [ ] Sync logs store `document_path`
- [ ] Sync logs allow NULL `document_id`
- [ ] Cleanup job implemented
- [ ] Cleanup job scheduled (Celery/cron)
- [ ] Conflict detection handles modify-after-delete
- [ ] All 24 tests pass
- [ ] Code coverage >90%
- [ ] API documentation updated
- [ ] Client SDK updated (if applicable)

---

**This document provides everything needed to implement deletion propagation and pass all tests.**
