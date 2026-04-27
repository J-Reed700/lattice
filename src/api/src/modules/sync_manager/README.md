# Sync Manager Module

Multi-device document synchronization for Recall Vault.

## Overview

The Sync Manager implements a simple timestamp-based sync protocol with last-write-wins conflict resolution. It's designed for desktop-to-desktop sync, with future mobile client support planned.

## Architecture

### Components

1. **Models** (`/home/user/Recall/src/api/src/models/sync.py`):
   - `Device`: Registered user devices
   - `Document`: Synced documents with version tracking
   - `SyncLog`: Audit log of sync operations
   - `Conflict`: Detected sync conflicts

2. **Service** (`service.py`):
   - `SyncService`: Core sync logic and conflict detection

3. **API** (`/home/user/Recall/src/api/src/api/v1/sync.py`):
   - REST endpoints for sync operations

4. **Schemas** (`/home/user/Recall/src/api/src/schemas/sync.py`):
   - Pydantic models for request/response validation

## Sync Protocol

### Flow

```
1. Device Registration:
   POST /api/v1/sync/devices
   - Client sends device_id (UUID) and device_name
   - Server creates Device record

2. Pull (Desktop → Backend):
   POST /api/v1/sync/pull
   - Client sends: device_id, since_timestamp (optional)
   - Server returns: changes since timestamp, conflicts
   - Client applies changes to local database

3. Push (Desktop → Backend):
   POST /api/v1/sync/push
   - Client sends: device_id, list of changes
   - Server validates, detects conflicts
   - Server returns: accepted changes, conflicts
   - Client updates local sync status

4. Conflict Resolution:
   POST /api/v1/sync/conflicts/resolve
   - Client sends: conflict_id, resolution choice
   - Server applies resolution
```

### Conflict Detection

A conflict occurs when:
1. Same document modified on multiple devices
2. Timestamps within 5 minutes of each other
3. Content hashes differ

**Resolution strategies:**
- `RESOLVED_LOCAL`: Keep local version
- `RESOLVED_REMOTE`: Keep server version
- `RESOLVED_MERGE`: User provides merged content

## API Reference

### Register Device

```http
POST /api/v1/sync/devices
Authorization: Bearer <jwt_token>
Content-Type: application/json

{
  "device_id": "550e8400-e29b-41d4-a716-446655440000",
  "device_name": "John's MacBook Pro"
}

Response 201:
{
  "id": 1,
  "device_id": "550e8400-e29b-41d4-a716-446655440000",
  "device_name": "John's MacBook Pro",
  "last_seen_at": "2025-11-15T12:00:00Z",
  "created_at": "2025-11-15T12:00:00Z"
}
```

### Pull Changes

```http
POST /api/v1/sync/pull
Authorization: Bearer <jwt_token>
Content-Type: application/json

{
  "device_id": "550e8400-e29b-41d4-a716-446655440000",
  "since_timestamp": "2025-11-15T10:00:00Z"  // null for first sync
}

Response 200:
{
  "changes": [
    {
      "id": 123,
      "action": "update",
      "path": "/documents/notes.md",
      "title": "Meeting Notes",
      "content": "Updated content...",
      "content_hash": "abc123...",
      "modified_at": "2025-11-15T11:30:00Z",
      "version": 5
    }
  ],
  "conflicts": [
    {
      "id": 1,
      "document_id": 123,
      "local_version": 4,
      "remote_version": 5,
      "local_modified_at": "2025-11-15T11:28:00Z",
      "remote_modified_at": "2025-11-15T11:30:00Z",
      "local_content_hash": "def456...",
      "remote_content_hash": "abc123...",
      "status": "pending",
      "created_at": "2025-11-15T11:31:00Z"
    }
  ],
  "new_timestamp": "2025-11-15T12:00:00Z",
  "total_changes": 1
}
```

### Push Changes

```http
POST /api/v1/sync/push
Authorization: Bearer <jwt_token>
Content-Type: application/json

{
  "device_id": "550e8400-e29b-41d4-a716-446655440000",
  "changes": [
    {
      "action": "create",
      "path": "/documents/new.md",
      "title": "New Document",
      "content": "Content here...",
      "content_hash": "xyz789...",
      "modified_at": "2025-11-15T12:00:00Z",
      "version": 1
    },
    {
      "action": "update",
      "path": "/documents/existing.md",
      "title": "Updated Document",
      "content": "Updated content...",
      "content_hash": "uvw456...",
      "modified_at": "2025-11-15T12:01:00Z",
      "version": 3
    },
    {
      "action": "delete",
      "path": "/documents/old.md",
      "modified_at": "2025-11-15T12:02:00Z",
      "version": 2
    }
  ]
}

Response 200:
{
  "accepted": [
    "/documents/new.md",
    "/documents/existing.md",
    "/documents/old.md"
  ],
  "conflicts": [],
  "timestamp": "2025-11-15T12:05:00Z",
  "total_accepted": 3,
  "total_conflicts": 0
}
```

### Resolve Conflict

```http
POST /api/v1/sync/conflicts/resolve
Authorization: Bearer <jwt_token>
Content-Type: application/json

{
  "conflict_id": 1,
  "resolution": "resolved_local",  // or "resolved_remote" or "resolved_merge"
  "merged_content": null  // required if resolution is "resolved_merge"
}

Response 204: No Content
```

### Get Sync Status

```http
GET /api/v1/sync/status?device_id=550e8400-e29b-41d4-a716-446655440000
Authorization: Bearer <jwt_token>

Response 200:
{
  "device_id": "550e8400-e29b-41d4-a716-446655440000",
  "last_pull_timestamp": "2025-11-15T12:00:00Z",
  "last_push_timestamp": "2025-11-15T12:05:00Z",
  "pending_conflicts": 2,
  "synced_documents": 150
}
```

## Desktop Client Implementation Guide

### 1. Device Registration (First Launch)

```typescript
// Generate or retrieve stored device ID
const deviceId = localStorage.getItem('device_id') || uuidv4();
localStorage.setItem('device_id', deviceId);

// Register with backend
const response = await fetch('/api/v1/sync/devices', {
  method: 'POST',
  headers: {
    'Authorization': `Bearer ${jwt_token}`,
    'Content-Type': 'application/json',
  },
  body: JSON.stringify({
    device_id: deviceId,
    device_name: os.hostname(),  // or user-provided name
  }),
});
```

### 2. Background Sync (Every 5 minutes)

```typescript
async function backgroundSync() {
  // 1. Pull changes from server
  const lastPullTimestamp = localStorage.getItem('last_pull_timestamp');

  const pullResponse = await fetch('/api/v1/sync/pull', {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${jwt_token}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      device_id: deviceId,
      since_timestamp: lastPullTimestamp,
    }),
  });

  const pullData = await pullResponse.json();

  // 2. Apply changes to local database
  for (const change of pullData.changes) {
    await applyChangeLocally(change);
  }

  // 3. Store new timestamp
  localStorage.setItem('last_pull_timestamp', pullData.new_timestamp);

  // 4. Handle conflicts (show UI)
  if (pullData.conflicts.length > 0) {
    showConflictUI(pullData.conflicts);
  }

  // 5. Push local changes
  const localChanges = await getLocalChanges();

  if (localChanges.length > 0) {
    const pushResponse = await fetch('/api/v1/sync/push', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${jwt_token}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        device_id: deviceId,
        changes: localChanges,
      }),
    });

    const pushData = await pushResponse.json();

    // 6. Update local sync status
    for (const path of pushData.accepted) {
      await markAsSynced(path);
    }

    // 7. Handle push conflicts
    if (pushData.conflicts.length > 0) {
      showConflictUI(pushData.conflicts);
    }
  }
}

// Run every 5 minutes
setInterval(backgroundSync, 5 * 60 * 1000);
```

### 3. Local Database Schema

```sql
-- Add sync tracking to local SQLite
ALTER TABLE documents ADD COLUMN sync_status TEXT;
ALTER TABLE documents ADD COLUMN remote_modified_at INTEGER;
ALTER TABLE documents ADD COLUMN last_synced_at INTEGER;

-- Track device state
CREATE TABLE sync_state (
  device_id TEXT PRIMARY KEY,
  last_pull_timestamp INTEGER,
  last_push_timestamp INTEGER
);
```

### 4. Change Tracking

```typescript
// When document is modified locally
async function onDocumentModified(document) {
  // Update in local DB
  await db.execute(`
    UPDATE documents
    SET
      modified_at = ?,
      sync_status = 'pending',
      version = version + 1
    WHERE id = ?
  `, [Date.now(), document.id]);

  // Trigger sync (debounced)
  scheduleSyncSoon();
}

async function getLocalChanges() {
  // Get all documents with sync_status = 'pending'
  const docs = await db.query(`
    SELECT * FROM documents
    WHERE sync_status = 'pending'
  `);

  return docs.map(doc => ({
    action: doc.deleted ? 'delete' : (doc.created_at === doc.modified_at ? 'create' : 'update'),
    path: doc.path,
    title: doc.title,
    content: doc.content,
    content_hash: sha256(doc.content),
    modified_at: new Date(doc.modified_at).toISOString(),
    version: doc.version,
  }));
}
```

## Database Schema

See `/home/user/Recall/src/api/migrations/versions/002_sync_tables.py` for full schema.

### Tables

1. **devices**: User's registered devices
2. **documents**: Synced documents with version tracking
3. **sync_logs**: Audit trail of sync operations
4. **conflicts**: Detected sync conflicts

### Indexes

- `idx_device_user`: Fast device lookup per user
- `idx_document_user_path`: Unique document paths per user
- `idx_document_user_modified`: Efficient change queries
- `idx_sync_log_device_timestamp`: Sync history queries
- `idx_conflict_document_status`: Pending conflict lookup

## Configuration

### Environment Variables

```env
# Database (sync requires PostgreSQL for multi-user)
DATABASE_URL=postgresql+asyncpg://user:pass@localhost:5432/vault

# JWT Auth (required for sync)
JWT_SECRET_KEY=your-secret-key
JWT_ALGORITHM=HS256
JWT_ACCESS_TOKEN_EXPIRE_MINUTES=30
```

### Service Constants

```python
# In SyncService class
CONFLICT_THRESHOLD_SECONDS = 300  # 5 minutes
```

## Testing

### Manual Testing

```bash
# 1. Register device A
curl -X POST http://localhost:8000/api/v1/sync/devices \
  -H "Authorization: Bearer $TOKEN_A" \
  -H "Content-Type: application/json" \
  -d '{"device_id": "device-a", "device_name": "Device A"}'

# 2. Register device B
curl -X POST http://localhost:8000/api/v1/sync/devices \
  -H "Authorization: Bearer $TOKEN_B" \
  -H "Content-Type: application/json" \
  -d '{"device_id": "device-b", "device_name": "Device B"}'

# 3. Push change from device A
curl -X POST http://localhost:8000/api/v1/sync/push \
  -H "Authorization: Bearer $TOKEN_A" \
  -H "Content-Type: application/json" \
  -d '{
    "device_id": "device-a",
    "changes": [{
      "action": "create",
      "path": "/test.md",
      "title": "Test",
      "content": "Hello",
      "content_hash": "abc123",
      "modified_at": "2025-11-15T12:00:00Z",
      "version": 1
    }]
  }'

# 4. Pull on device B (should see the change)
curl -X POST http://localhost:8000/api/v1/sync/pull \
  -H "Authorization: Bearer $TOKEN_B" \
  -H "Content-Type: application/json" \
  -d '{
    "device_id": "device-b",
    "since_timestamp": null
  }'
```

### Automated Tests

See `/home/user/Recall/src/api/tests/integration/test_sync.py` (to be created).

## Performance Considerations

- **Indexes**: All timestamp and user_id columns indexed for fast queries
- **Pagination**: Not yet implemented (add if >1000 documents)
- **Compression**: Document content not compressed (add gzip if needed)
- **Batch size**: No limit on push changes (add chunking if needed)

## Future Enhancements

### Phase 2: Full Content Sync
- Currently syncs metadata only
- Add optional full content sync with compression
- Embedding sync for mobile clients

### Phase 3: Mobile Support
- Cloud embedding generation
- Cloud semantic search
- Batch processing for mobile uploads

### Phase 4: Advanced Features
- Operational transforms for real-time collaboration
- WebSocket push notifications
- Selective sync (folder filtering)
- Bandwidth optimization (delta sync)

## Troubleshooting

### Common Issues

1. **Conflict loop**: If same conflict keeps recurring, check client change tracking
2. **Missing changes**: Verify `since_timestamp` is stored correctly
3. **Performance**: Add indexes if queries are slow (check with `EXPLAIN ANALYZE`)
4. **Data loss**: Check sync_logs table for audit trail

### Debug Logging

```python
import logging
logging.getLogger('src.modules.sync_manager').setLevel(logging.DEBUG)
```

## Security

- **Authentication**: All endpoints require valid JWT token
- **Authorization**: Users can only sync their own documents
- **Validation**: All inputs validated with Pydantic schemas
- **SQL Injection**: Protected by SQLAlchemy ORM
- **Rate Limiting**: Applied via global middleware

## Migration Guide

To enable sync on existing backend:

```bash
# 1. Run migration
cd /home/user/Recall/src/api
poetry run alembic upgrade head

# 2. Verify tables created
psql $DATABASE_URL -c "\dt devices documents sync_logs conflicts"

# 3. Test endpoints
curl http://localhost:8000/api/v1/sync/health
```

## Support

For issues or questions:
- Check logs: `tail -f logs/vault.log`
- Check database: `psql $DATABASE_URL`
- Review code: `/home/user/Recall/src/api/src/modules/sync_manager/`
