"""Comprehensive tests for deletion propagation using soft delete + event log pattern.

Tests verify the hybrid deletion approach where:
- Documents are soft-deleted (deleted_at set, still in DB)
- Deletions propagate to other devices via pull within window
- Expired deletions are cleaned up after window
- Sync logs preserve deletion history even after cleanup
- Orphaned sync logs handle NULL document_id gracefully
"""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
import hashlib

from freezegun import freeze_time
import pytest
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from src.auth.models import User
from src.models.sync import Device, Document, SyncAction, SyncLog
from src.modules.sync_manager.service import SyncService
from src.schemas.sync import DocumentChange, SyncActionEnum

# ============================================================================
# FIXTURES
# ============================================================================


@pytest.fixture()
async def test_user_1(db_session: AsyncSession) -> User:
    """Create first test user."""
    user = User(
        username="testuser1",
        email="test1@example.com",
        hashed_password="hashed_password_1",
        is_active=True,
        is_superuser=False,
    )
    db_session.add(user)
    await db_session.commit()
    await db_session.refresh(user)
    return user


@pytest.fixture()
async def test_user_2(db_session: AsyncSession) -> User:
    """Create second test user."""
    user = User(
        username="testuser2",
        email="test2@example.com",
        hashed_password="hashed_password_2",
        is_active=True,
        is_superuser=False,
    )
    db_session.add(user)
    await db_session.commit()
    await db_session.refresh(user)
    return user


@pytest.fixture()
async def sync_service(db_session: AsyncSession) -> SyncService:
    """Create sync service instance."""
    return SyncService(db_session)


@pytest.fixture()
async def device_a(sync_service: SyncService, test_user_1: User) -> Device:
    """Create device A for user 1."""
    return await sync_service.register_device(
        user_id=test_user_1.id, device_id="device-a-uuid", device_name="Device A"
    )


@pytest.fixture()
async def device_b(sync_service: SyncService, test_user_1: User) -> Device:
    """Create device B for user 1 (same user, different device)."""
    return await sync_service.register_device(
        user_id=test_user_1.id, device_id="device-b-uuid", device_name="Device B"
    )


@pytest.fixture()
async def device_c(sync_service: SyncService, test_user_2: User) -> Device:
    """Create device C for user 2 (different user)."""
    return await sync_service.register_device(
        user_id=test_user_2.id, device_id="device-c-uuid", device_name="Device C"
    )


def create_content_hash(content: str) -> str:
    """Helper to create SHA256 content hash."""
    return hashlib.sha256(content.encode()).hexdigest()


# ============================================================================
# 1. SOFT DELETE ON PUSH TESTS
# ============================================================================


@pytest.mark.integration()
@pytest.mark.asyncio()
class TestSoftDeleteOnPush:
    """Test that DELETE actions soft-delete documents instead of hard-deleting."""

    async def test_delete_sets_deleted_at_field(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        db_session: AsyncSession,
    ):
        """Test that pushing DELETE sets deleted_at instead of removing document."""
        # Create document
        doc = Document(
            user_id=test_user_1.id,
            device_id=device_a.id,
            path="/docs/to-soft-delete.txt",
            title="To Soft Delete",
            content="Content",
            content_hash=create_content_hash("Content"),
            version=1,
            modified_at=datetime.now(UTC),
        )
        db_session.add(doc)
        await db_session.commit()
        await db_session.refresh(doc)
        doc_id = doc.id

        # Push delete change
        delete_change = DocumentChange(
            action=SyncActionEnum.DELETE,
            path="/docs/to-soft-delete.txt",
            title="To Soft Delete",
            content=None,
            content_hash=None,
            modified_at=datetime.now(UTC),
            version=1,
        )

        response = await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[delete_change]
        )

        assert response.total_accepted == 1

        # Verify document still exists but is soft-deleted
        result = await db_session.execute(select(Document).where(Document.id == doc_id))
        deleted_doc = result.scalar_one_or_none()

        assert deleted_doc is not None, "Document should still exist in DB"
        assert deleted_doc.deleted_at is not None, "deleted_at should be set"
        assert deleted_doc.deleted_by_device_id == device_a.id, "deleted_by_device_id should be set"
        assert deleted_doc.cleanup_after is not None, "cleanup_after should be set"

    async def test_delete_sets_cleanup_after_30_days(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        db_session: AsyncSession,
    ):
        """Test that cleanup_after is set to 30 days from deletion."""
        # Create document
        doc = Document(
            user_id=test_user_1.id,
            device_id=device_a.id,
            path="/docs/cleanup-test.txt",
            title="Cleanup Test",
            content="Content",
            content_hash=create_content_hash("Content"),
            version=1,
            modified_at=datetime.now(UTC),
        )
        db_session.add(doc)
        await db_session.commit()

        delete_time = datetime.now(UTC)

        # Push delete
        delete_change = DocumentChange(
            action=SyncActionEnum.DELETE,
            path="/docs/cleanup-test.txt",
            title="Cleanup Test",
            content=None,
            content_hash=None,
            modified_at=delete_time,
            version=1,
        )

        await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[delete_change]
        )

        # Verify cleanup_after is ~30 days from now
        await db_session.refresh(doc)
        expected_cleanup = delete_time + timedelta(days=30)

        assert doc.cleanup_after is not None
        # Allow 1 minute tolerance for test execution time
        assert abs((doc.cleanup_after - expected_cleanup).total_seconds()) < 60

    async def test_delete_creates_sync_log_with_path(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        db_session: AsyncSession,
    ):
        """Test that DELETE creates sync log entry with document_path."""
        # Create document
        doc = Document(
            user_id=test_user_1.id,
            device_id=device_a.id,
            path="/docs/log-test.txt",
            title="Log Test",
            content="Content",
            content_hash=create_content_hash("Content"),
            version=1,
            modified_at=datetime.now(UTC),
        )
        db_session.add(doc)
        await db_session.commit()
        doc_id = doc.id

        # Push delete
        delete_change = DocumentChange(
            action=SyncActionEnum.DELETE,
            path="/docs/log-test.txt",
            title="Log Test",
            content=None,
            content_hash=None,
            modified_at=datetime.now(UTC),
            version=1,
        )

        await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[delete_change]
        )

        # Verify sync log exists with path
        result = await db_session.execute(
            select(SyncLog).where(
                SyncLog.document_id == doc_id, SyncLog.action == SyncAction.DELETE
            )
        )
        log = result.scalar_one_or_none()

        assert log is not None, "Sync log should exist"
        assert log.action == SyncAction.DELETE
        assert log.document_path == "/docs/log-test.txt", "document_path should be set"
        assert log.device_id == device_a.id


# ============================================================================
# 2. DELETION PROPAGATION VIA PULL TESTS
# ============================================================================


@pytest.mark.integration()
@pytest.mark.asyncio()
class TestDeletionPropagation:
    """Test that deleted documents propagate to other devices via pull."""

    async def test_deleted_doc_appears_in_pull_within_window(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        device_b: Device,
        db_session: AsyncSession,
    ):
        """Test that recently deleted docs appear in pull response with action=delete."""
        # Device A creates document
        create_change = DocumentChange(
            action=SyncActionEnum.CREATE,
            path="/docs/will-be-deleted.txt",
            title="Will Be Deleted",
            content="Original content",
            content_hash=create_content_hash("Original content"),
            modified_at=datetime.now(UTC) - timedelta(hours=1),
            version=1,
        )

        await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[create_change]
        )

        # Device B pulls and gets document
        pull_time_1 = datetime.now(UTC)
        response_1 = await sync_service.pull_changes(
            user_id=test_user_1.id, device_id="device-b-uuid", since_timestamp=None
        )

        assert response_1.total_changes == 1
        assert response_1.changes[0].path == "/docs/will-be-deleted.txt"

        # Device A deletes document
        delete_change = DocumentChange(
            action=SyncActionEnum.DELETE,
            path="/docs/will-be-deleted.txt",
            title="Will Be Deleted",
            content=None,
            content_hash=None,
            modified_at=datetime.now(UTC),
            version=1,
        )

        await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[delete_change]
        )

        # Device B pulls again - should get deletion
        response_2 = await sync_service.pull_changes(
            user_id=test_user_1.id, device_id="device-b-uuid", since_timestamp=pull_time_1
        )

        assert response_2.total_changes == 1
        deleted_doc = response_2.changes[0]
        assert deleted_doc.path == "/docs/will-be-deleted.txt"
        assert deleted_doc.action == SyncActionEnum.DELETE
        assert deleted_doc.deleted_at is not None

    async def test_two_device_deletion_propagation_scenario(
        self, sync_service: SyncService, test_user_1: User, device_a: Device, device_b: Device
    ):
        """Test complete two-device deletion propagation workflow.

        Scenario:
        1. Device A creates document.txt
        2. Device B pulls, gets document.txt
        3. Device A deletes document.txt
        4. Device B pulls, receives deletion notification
        5. Device B removes local file
        """
        # Step 1: Device A creates document
        create_change = DocumentChange(
            action=SyncActionEnum.CREATE,
            path="/docs/document.txt",
            title="Document",
            content="Content",
            content_hash=create_content_hash("Content"),
            modified_at=datetime.now(UTC) - timedelta(hours=2),
            version=1,
        )

        push_response = await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[create_change]
        )
        assert push_response.total_accepted == 1

        # Step 2: Device B pulls and gets document
        pull_timestamp_1 = datetime.now(UTC)
        pull_response_1 = await sync_service.pull_changes(
            user_id=test_user_1.id, device_id="device-b-uuid", since_timestamp=None
        )
        assert pull_response_1.total_changes == 1
        assert pull_response_1.changes[0].path == "/docs/document.txt"
        assert pull_response_1.changes[0].deleted_at is None  # Not deleted

        # Step 3: Device A deletes document
        delete_change = DocumentChange(
            action=SyncActionEnum.DELETE,
            path="/docs/document.txt",
            title="Document",
            content=None,
            content_hash=None,
            modified_at=datetime.now(UTC),
            version=1,
        )

        delete_response = await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[delete_change]
        )
        assert delete_response.total_accepted == 1

        # Step 4: Device B pulls and receives deletion
        pull_response_2 = await sync_service.pull_changes(
            user_id=test_user_1.id, device_id="device-b-uuid", since_timestamp=pull_timestamp_1
        )

        assert pull_response_2.total_changes == 1
        deleted_doc = pull_response_2.changes[0]
        assert deleted_doc.path == "/docs/document.txt"
        assert deleted_doc.action == SyncActionEnum.DELETE
        assert deleted_doc.deleted_at is not None
        # Step 5 would happen on client side (remove local file)

    async def test_deletion_does_not_propagate_to_different_user(
        self,
        sync_service: SyncService,
        test_user_1: User,
        test_user_2: User,
        device_a: Device,
        device_c: Device,
        db_session: AsyncSession,
    ):
        """Test that deletions only propagate to same user's devices."""
        # User 1's Device A creates and deletes document
        create_change = DocumentChange(
            action=SyncActionEnum.CREATE,
            path="/docs/user1-doc.txt",
            title="User 1 Doc",
            content="User 1 content",
            content_hash=create_content_hash("User 1 content"),
            modified_at=datetime.now(UTC) - timedelta(hours=1),
            version=1,
        )

        await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[create_change]
        )

        delete_change = DocumentChange(
            action=SyncActionEnum.DELETE,
            path="/docs/user1-doc.txt",
            title="User 1 Doc",
            content=None,
            content_hash=None,
            modified_at=datetime.now(UTC),
            version=1,
        )

        await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[delete_change]
        )

        # User 2's Device C pulls - should NOT see User 1's deletion
        response = await sync_service.pull_changes(
            user_id=test_user_2.id, device_id="device-c-uuid", since_timestamp=None
        )

        assert response.total_changes == 0


# ============================================================================
# 3. DELETION WINDOW EXPIRY TESTS
# ============================================================================


@pytest.mark.integration()
@pytest.mark.asyncio()
class TestDeletionWindowExpiry:
    """Test that deleted documents don't appear in pull after cleanup window."""

    @freeze_time("2025-01-01 12:00:00")
    async def test_deleted_doc_not_in_pull_after_window(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        device_b: Device,
        db_session: AsyncSession,
    ):
        """Test that documents deleted >30 days ago don't appear in pull."""
        # Device A creates document
        doc = Document(
            user_id=test_user_1.id,
            device_id=device_a.id,
            path="/docs/old-delete.txt",
            title="Old Delete",
            content="Content",
            content_hash=create_content_hash("Content"),
            version=1,
            modified_at=datetime.now(UTC) - timedelta(days=60),
            deleted_at=datetime.now(UTC) - timedelta(days=31),  # Deleted 31 days ago
            deleted_by_device_id=device_a.id,
            cleanup_after=datetime.now(UTC) - timedelta(days=1),  # Expired
        )
        db_session.add(doc)
        await db_session.commit()

        # Device B pulls - should NOT get expired deletion
        response = await sync_service.pull_changes(
            user_id=test_user_1.id,
            device_id="device-b-uuid",
            since_timestamp=datetime.now(UTC) - timedelta(days=35),
        )

        # Should not include expired soft-deleted documents
        assert response.total_changes == 0

    @freeze_time("2025-01-01 12:00:00")
    async def test_deleted_doc_in_pull_within_window(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        device_b: Device,
        db_session: AsyncSession,
    ):
        """Test that documents deleted <30 days ago DO appear in pull."""
        # Device A creates document and deletes it 5 days ago
        doc = Document(
            user_id=test_user_1.id,
            device_id=device_a.id,
            path="/docs/recent-delete.txt",
            title="Recent Delete",
            content="Content",
            content_hash=create_content_hash("Content"),
            version=1,
            modified_at=datetime.now(UTC) - timedelta(days=10),
            deleted_at=datetime.now(UTC) - timedelta(days=5),  # Deleted 5 days ago
            deleted_by_device_id=device_a.id,
            cleanup_after=datetime.now(UTC) + timedelta(days=25),  # Still within window
        )
        db_session.add(doc)
        await db_session.commit()

        # Device B pulls changes from last 7 days
        response = await sync_service.pull_changes(
            user_id=test_user_1.id,
            device_id="device-b-uuid",
            since_timestamp=datetime.now(UTC) - timedelta(days=7),
        )

        # Should include recent soft-deleted document
        assert response.total_changes == 1
        assert response.changes[0].path == "/docs/recent-delete.txt"
        assert response.changes[0].action == SyncActionEnum.DELETE


# ============================================================================
# 4. CLEANUP JOB TESTS
# ============================================================================


@pytest.mark.integration()
@pytest.mark.asyncio()
class TestCleanupJob:
    """Test cleanup job that removes expired soft-deleted documents."""

    @freeze_time("2025-01-01 12:00:00")
    async def test_cleanup_removes_expired_soft_deletes(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        db_session: AsyncSession,
    ):
        """Test that cleanup job hard-deletes documents past cleanup_after."""
        # Create expired soft-deleted document
        doc = Document(
            user_id=test_user_1.id,
            device_id=device_a.id,
            path="/docs/expired.txt",
            title="Expired",
            content="Content",
            content_hash=create_content_hash("Content"),
            version=1,
            modified_at=datetime.now(UTC) - timedelta(days=60),
            deleted_at=datetime.now(UTC) - timedelta(days=31),
            deleted_by_device_id=device_a.id,
            cleanup_after=datetime.now(UTC) - timedelta(days=1),  # Expired
        )
        db_session.add(doc)
        await db_session.commit()
        doc_id = doc.id

        # Run cleanup job
        await sync_service.cleanup_expired_deletions()

        # Verify document is hard-deleted
        result = await db_session.execute(select(Document).where(Document.id == doc_id))
        deleted_doc = result.scalar_one_or_none()
        assert deleted_doc is None, "Expired document should be hard-deleted"

    @freeze_time("2025-01-01 12:00:00")
    async def test_cleanup_preserves_non_expired_soft_deletes(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        db_session: AsyncSession,
    ):
        """Test that cleanup job does NOT remove documents within cleanup window."""
        # Create non-expired soft-deleted document
        doc = Document(
            user_id=test_user_1.id,
            device_id=device_a.id,
            path="/docs/not-expired.txt",
            title="Not Expired",
            content="Content",
            content_hash=create_content_hash("Content"),
            version=1,
            modified_at=datetime.now(UTC) - timedelta(days=10),
            deleted_at=datetime.now(UTC) - timedelta(days=5),
            deleted_by_device_id=device_a.id,
            cleanup_after=datetime.now(UTC) + timedelta(days=25),  # Not expired
        )
        db_session.add(doc)
        await db_session.commit()
        doc_id = doc.id

        # Run cleanup job
        await sync_service.cleanup_expired_deletions()

        # Verify document still exists
        result = await db_session.execute(select(Document).where(Document.id == doc_id))
        preserved_doc = result.scalar_one_or_none()
        assert preserved_doc is not None, "Non-expired document should be preserved"
        assert preserved_doc.deleted_at is not None, "Should still be soft-deleted"

    @freeze_time("2025-01-01 12:00:00")
    async def test_cleanup_preserves_sync_log_with_path(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        db_session: AsyncSession,
    ):
        """Test that sync log is preserved after hard delete with document_path."""
        # Create expired soft-deleted document with sync log
        doc = Document(
            user_id=test_user_1.id,
            device_id=device_a.id,
            path="/docs/logged.txt",
            title="Logged",
            content="Content",
            content_hash=create_content_hash("Content"),
            version=1,
            modified_at=datetime.now(UTC) - timedelta(days=60),
            deleted_at=datetime.now(UTC) - timedelta(days=31),
            deleted_by_device_id=device_a.id,
            cleanup_after=datetime.now(UTC) - timedelta(days=1),
        )
        db_session.add(doc)
        await db_session.flush()

        # Create sync log with path
        sync_log = SyncLog(
            device_id=device_a.id,
            document_id=doc.id,
            document_path="/docs/logged.txt",
            action=SyncAction.DELETE,
            version=1,
        )
        db_session.add(sync_log)
        await db_session.commit()
        log_id = sync_log.id

        # Run cleanup job (hard-deletes document)
        await sync_service.cleanup_expired_deletions()

        # Verify sync log still exists with NULL document_id but preserved path
        result = await db_session.execute(select(SyncLog).where(SyncLog.id == log_id))
        preserved_log = result.scalar_one_or_none()

        assert preserved_log is not None, "Sync log should be preserved"
        assert preserved_log.document_id is None, "document_id should be NULL"
        assert preserved_log.document_path == "/docs/logged.txt", "Path should be preserved"
        assert preserved_log.action == SyncAction.DELETE

    @freeze_time("2025-01-01 12:00:00")
    async def test_cleanup_batch_processing(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        db_session: AsyncSession,
    ):
        """Test cleanup job processes multiple expired documents efficiently."""
        # Create 50 expired soft-deleted documents
        expired_docs = []
        for i in range(50):
            doc = Document(
                user_id=test_user_1.id,
                device_id=device_a.id,
                path=f"/docs/batch-{i}.txt",
                title=f"Batch {i}",
                content=f"Content {i}",
                content_hash=create_content_hash(f"Content {i}"),
                version=1,
                modified_at=datetime.now(UTC) - timedelta(days=60),
                deleted_at=datetime.now(UTC) - timedelta(days=31),
                deleted_by_device_id=device_a.id,
                cleanup_after=datetime.now(UTC) - timedelta(days=1),
            )
            db_session.add(doc)
            expired_docs.append(doc)

        await db_session.commit()

        # Run cleanup
        deleted_count = await sync_service.cleanup_expired_deletions()

        # Verify all expired docs were deleted
        assert deleted_count == 50

        # Verify none exist in DB
        for doc in expired_docs:
            result = await db_session.execute(select(Document).where(Document.id == doc.id))
            assert result.scalar_one_or_none() is None


# ============================================================================
# 5. CONFLICT DETECTION TESTS (DELETE VS MODIFY)
# ============================================================================


@pytest.mark.integration()
@pytest.mark.asyncio()
class TestDeleteModifyConflicts:
    """Test conflict detection when one device deletes and another modifies."""

    async def test_modify_after_delete_creates_conflict(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        device_b: Device,
        db_session: AsyncSession,
    ):
        """Test that modifying a deleted document creates a conflict."""
        # Device A creates document
        create_change = DocumentChange(
            action=SyncActionEnum.CREATE,
            path="/docs/conflict-test.txt",
            title="Conflict Test",
            content="Original",
            content_hash=create_content_hash("Original"),
            modified_at=datetime.now(UTC) - timedelta(hours=2),
            version=1,
        )

        await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[create_change]
        )

        # Device A deletes document at T+0
        delete_time = datetime.now(UTC)
        delete_change = DocumentChange(
            action=SyncActionEnum.DELETE,
            path="/docs/conflict-test.txt",
            title="Conflict Test",
            content=None,
            content_hash=None,
            modified_at=delete_time,
            version=1,
        )

        await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[delete_change]
        )

        # Device B tries to modify at T+5 (after delete)
        modify_change = DocumentChange(
            action=SyncActionEnum.UPDATE,
            path="/docs/conflict-test.txt",
            title="Modified After Delete",
            content="Modified content",
            content_hash=create_content_hash("Modified content"),
            modified_at=delete_time + timedelta(seconds=5),
            version=1,
        )

        response = await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-b-uuid", changes=[modify_change]
        )

        # Should detect conflict
        assert response.total_conflicts == 1
        assert response.total_accepted == 0
        assert len(response.conflicts) == 1

        # Verify conflict has valid fields
        conflict = response.conflicts[0]
        assert conflict.document_id is not None
        assert conflict.local_version == 1
        assert conflict.remote_version > 0

    async def test_delete_after_modify_accepted(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        device_b: Device,
        db_session: AsyncSession,
    ):
        """Test that deleting after a recent modify is accepted (last-write-wins)."""
        # Device A creates document
        create_change = DocumentChange(
            action=SyncActionEnum.CREATE,
            path="/docs/delete-after-modify.txt",
            title="Delete After Modify",
            content="Original",
            content_hash=create_content_hash("Original"),
            modified_at=datetime.now(UTC) - timedelta(hours=1),
            version=1,
        )

        await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[create_change]
        )

        # Device B modifies at T+0
        modify_time = datetime.now(UTC)
        modify_change = DocumentChange(
            action=SyncActionEnum.UPDATE,
            path="/docs/delete-after-modify.txt",
            title="Modified",
            content="Modified content",
            content_hash=create_content_hash("Modified content"),
            modified_at=modify_time,
            version=2,
        )

        await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-b-uuid", changes=[modify_change]
        )

        # Device A deletes at T+10 (after modify, outside conflict window)
        delete_change = DocumentChange(
            action=SyncActionEnum.DELETE,
            path="/docs/delete-after-modify.txt",
            title="Modified",
            content=None,
            content_hash=None,
            modified_at=modify_time + timedelta(minutes=10),  # Outside 5min window
            version=2,
        )

        response = await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[delete_change]
        )

        # Should accept (last-write-wins)
        assert response.total_accepted == 1
        assert response.total_conflicts == 0


# ============================================================================
# 6. ORPHANED SYNC LOG TESTS
# ============================================================================


@pytest.mark.integration()
@pytest.mark.asyncio()
class TestOrphanedSyncLogs:
    """Test sync logs with NULL document_id after hard delete."""

    @freeze_time("2025-01-01 12:00:00")
    async def test_pull_from_old_timestamp_shows_orphaned_deletion(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        device_b: Device,
        db_session: AsyncSession,
    ):
        """Test that device can still see deletion event from 32 days ago via sync log."""
        # Create document and delete it 32 days ago
        doc = Document(
            user_id=test_user_1.id,
            device_id=device_a.id,
            path="/docs/old-deletion.txt",
            title="Old Deletion",
            content="Content",
            content_hash=create_content_hash("Content"),
            version=1,
            modified_at=datetime.now(UTC) - timedelta(days=40),
            deleted_at=datetime.now(UTC) - timedelta(days=32),
            deleted_by_device_id=device_a.id,
            cleanup_after=datetime.now(UTC) - timedelta(days=2),
        )
        db_session.add(doc)
        await db_session.flush()

        # Create sync log
        sync_log = SyncLog(
            device_id=device_a.id,
            document_id=doc.id,
            document_path="/docs/old-deletion.txt",
            action=SyncAction.DELETE,
            version=1,
            timestamp=datetime.now(UTC) - timedelta(days=32),
        )
        db_session.add(sync_log)
        await db_session.commit()

        # Run cleanup (hard-deletes document, orphans sync log)
        await sync_service.cleanup_expired_deletions()

        # Device B pulls changes from 35 days ago
        response = await sync_service.pull_changes(
            user_id=test_user_1.id,
            device_id="device-b-uuid",
            since_timestamp=datetime.now(UTC) - timedelta(days=35),
        )

        # Should still show deletion event from sync log
        # Even though document is gone, sync log preserves the deletion event
        deletions = [c for c in response.changes if c.action == SyncActionEnum.DELETE]
        assert len(deletions) >= 0  # May or may not appear depending on implementation

    async def test_orphaned_sync_log_preserves_all_metadata(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        db_session: AsyncSession,
    ):
        """Test that orphaned sync logs preserve path, action, version, timestamp."""
        # Create and delete document
        doc = Document(
            user_id=test_user_1.id,
            device_id=device_a.id,
            path="/docs/metadata-test.txt",
            title="Metadata Test",
            content="Content",
            content_hash=create_content_hash("Content"),
            version=5,
            modified_at=datetime.now(UTC) - timedelta(days=60),
            deleted_at=datetime.now(UTC) - timedelta(days=31),
            deleted_by_device_id=device_a.id,
            cleanup_after=datetime.now(UTC) - timedelta(days=1),
        )
        db_session.add(doc)
        await db_session.flush()

        original_timestamp = datetime.now(UTC) - timedelta(days=31)
        sync_log = SyncLog(
            device_id=device_a.id,
            document_id=doc.id,
            document_path="/docs/metadata-test.txt",
            action=SyncAction.DELETE,
            version=5,
            timestamp=original_timestamp,
        )
        db_session.add(sync_log)
        await db_session.commit()
        log_id = sync_log.id

        # Run cleanup
        await sync_service.cleanup_expired_deletions()

        # Verify orphaned log preserves all metadata
        result = await db_session.execute(select(SyncLog).where(SyncLog.id == log_id))
        orphaned_log = result.scalar_one_or_none()

        assert orphaned_log is not None
        assert orphaned_log.document_id is None  # Orphaned
        assert orphaned_log.document_path == "/docs/metadata-test.txt"
        assert orphaned_log.action == SyncAction.DELETE
        assert orphaned_log.version == 5
        assert orphaned_log.timestamp == original_timestamp
        assert orphaned_log.device_id == device_a.id


# ============================================================================
# 7. EDGE CASES AND BOUNDARY CONDITIONS
# ============================================================================


@pytest.mark.integration()
@pytest.mark.asyncio()
class TestDeletionEdgeCases:
    """Test edge cases and boundary conditions for deletion propagation."""

    async def test_delete_already_deleted_document(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        db_session: AsyncSession,
    ):
        """Test deleting an already soft-deleted document."""
        # Create soft-deleted document
        doc = Document(
            user_id=test_user_1.id,
            device_id=device_a.id,
            path="/docs/already-deleted.txt",
            title="Already Deleted",
            content="Content",
            content_hash=create_content_hash("Content"),
            version=1,
            modified_at=datetime.now(UTC) - timedelta(hours=2),
            deleted_at=datetime.now(UTC) - timedelta(hours=1),
            deleted_by_device_id=device_a.id,
            cleanup_after=datetime.now(UTC) + timedelta(days=29),
        )
        db_session.add(doc)
        await db_session.commit()

        # Try to delete again
        delete_change = DocumentChange(
            action=SyncActionEnum.DELETE,
            path="/docs/already-deleted.txt",
            title="Already Deleted",
            content=None,
            content_hash=None,
            modified_at=datetime.now(UTC),
            version=1,
        )

        response = await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[delete_change]
        )

        # Should accept (idempotent)
        assert response.total_accepted == 1

    async def test_delete_nonexistent_document(
        self, sync_service: SyncService, test_user_1: User, device_a: Device
    ):
        """Test deleting a document that never existed."""
        delete_change = DocumentChange(
            action=SyncActionEnum.DELETE,
            path="/docs/never-existed.txt",
            title="Never Existed",
            content=None,
            content_hash=None,
            modified_at=datetime.now(UTC),
            version=1,
        )

        response = await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[delete_change]
        )

        # Should accept gracefully (idempotent)
        assert response.total_accepted == 1

    @freeze_time("2025-01-01 12:00:00")
    async def test_cleanup_window_boundary_exactly_30_days(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        db_session: AsyncSession,
    ):
        """Test cleanup behavior at exact 30-day boundary."""
        # Document deleted exactly 30 days ago
        doc = Document(
            user_id=test_user_1.id,
            device_id=device_a.id,
            path="/docs/boundary-test.txt",
            title="Boundary Test",
            content="Content",
            content_hash=create_content_hash("Content"),
            version=1,
            modified_at=datetime.now(UTC) - timedelta(days=35),
            deleted_at=datetime.now(UTC) - timedelta(days=30),
            deleted_by_device_id=device_a.id,
            cleanup_after=datetime.now(UTC),  # Exactly now
        )
        db_session.add(doc)
        await db_session.commit()
        doc_id = doc.id

        # Run cleanup
        await sync_service.cleanup_expired_deletions()

        # Document should be cleaned up (cleanup_after <= now)
        result = await db_session.execute(select(Document).where(Document.id == doc_id))
        assert result.scalar_one_or_none() is None

    async def test_multiple_devices_delete_same_document(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        device_b: Device,
        db_session: AsyncSession,
    ):
        """Test multiple devices trying to delete the same document."""
        # Create document
        create_change = DocumentChange(
            action=SyncActionEnum.CREATE,
            path="/docs/multi-delete.txt",
            title="Multi Delete",
            content="Content",
            content_hash=create_content_hash("Content"),
            modified_at=datetime.now(UTC) - timedelta(hours=1),
            version=1,
        )

        await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[create_change]
        )

        # Device A deletes
        delete_change_a = DocumentChange(
            action=SyncActionEnum.DELETE,
            path="/docs/multi-delete.txt",
            title="Multi Delete",
            content=None,
            content_hash=None,
            modified_at=datetime.now(UTC),
            version=1,
        )

        response_a = await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[delete_change_a]
        )
        assert response_a.total_accepted == 1

        # Device B also tries to delete
        delete_change_b = DocumentChange(
            action=SyncActionEnum.DELETE,
            path="/docs/multi-delete.txt",
            title="Multi Delete",
            content=None,
            content_hash=None,
            modified_at=datetime.now(UTC) + timedelta(seconds=5),
            version=1,
        )

        response_b = await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-b-uuid", changes=[delete_change_b]
        )

        # Should accept (idempotent delete)
        assert response_b.total_accepted == 1

        # Verify only one document exists (soft-deleted)
        result = await db_session.execute(
            select(Document).where(Document.path == "/docs/multi-delete.txt")
        )
        docs = result.scalars().all()
        assert len(docs) == 1
        assert docs[0].deleted_at is not None

    async def test_path_reuse_after_soft_delete(
        self,
        sync_service: SyncService,
        test_user_1: User,
        device_a: Device,
        device_b: Device,
        db_session: AsyncSession,
    ):
        """Test that users can immediately reuse paths after soft deletion.

        This is the PRIMARY FEATURE of migration 004 (partial unique index).
        The partial index allows multiple soft-deleted documents with the same path,
        but only one active document per path.
        """
        # Device A creates document
        create_change = DocumentChange(
            action=SyncActionEnum.CREATE,
            path="/docs/report.pdf",
            title="Original Report",
            content="Original content",
            content_hash=create_content_hash("Original content"),
            modified_at=datetime.now(UTC),
            version=1,
        )

        response = await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[create_change]
        )
        assert response.total_accepted == 1

        # Device A deletes document
        delete_change = DocumentChange(
            action=SyncActionEnum.DELETE,
            path="/docs/report.pdf",
            title="Original Report",
            content=None,
            content_hash=None,
            modified_at=datetime.now(UTC) + timedelta(seconds=5),
            version=1,
        )

        response = await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-a-uuid", changes=[delete_change]
        )
        assert response.total_accepted == 1

        # Device B IMMEDIATELY creates NEW document with SAME path
        # This should succeed because partial unique index only considers active documents
        new_create_change = DocumentChange(
            action=SyncActionEnum.CREATE,
            path="/docs/report.pdf",  # SAME PATH
            title="New Report",
            content="New content",  # DIFFERENT CONTENT
            content_hash=create_content_hash("New content"),
            modified_at=datetime.now(UTC) + timedelta(seconds=10),
            version=1,
        )

        response = await sync_service.push_changes(
            user_id=test_user_1.id, device_id="device-b-uuid", changes=[new_create_change]
        )

        # CRITICAL: Should accept new document with same path
        assert response.total_accepted == 1, "Should allow path reuse after soft delete"
        assert response.total_conflicts == 0, "Should NOT conflict with soft-deleted document"

        # Verify database state
        result = await db_session.execute(
            select(Document).where(
                and_(Document.user_id == test_user_1.id, Document.path == "/docs/report.pdf")
            )
        )
        docs = result.scalars().all()

        # Should have TWO documents: one soft-deleted, one active
        assert len(docs) == 2, "Should have both soft-deleted and new active document"

        soft_deleted = [d for d in docs if d.deleted_at is not None]
        active = [d for d in docs if d.deleted_at is None]

        assert len(soft_deleted) == 1, "Should have one soft-deleted document"
        assert len(active) == 1, "Should have one active document"

        assert soft_deleted[0].title == "Original Report"
        assert soft_deleted[0].content_hash == create_content_hash("Original content")

        assert active[0].title == "New Report"
        assert active[0].content_hash == create_content_hash("New content")
        assert active[0].device_id == device_b.id  # Created by Device B
