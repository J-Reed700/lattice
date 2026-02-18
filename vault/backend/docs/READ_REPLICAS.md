# Read Replica Strategy

This document outlines the strategy for implementing read replicas in the Recall Vault backend for improved performance and scalability.

## Table of Contents

- [Overview](#overview)
- [Current Architecture](#current-architecture)
- [Read Replica Benefits](#read-replica-benefits)
- [Implementation Strategy](#implementation-strategy)
- [Query Classification](#query-classification)
- [Configuration](#configuration)
- [Deployment](#deployment)
- [Monitoring](#monitoring)
- [Failover Strategy](#failover-strategy)

---

## Overview

**Read replicas** are read-only copies of the primary database that serve SELECT queries, reducing load on the primary database and improving read performance for high-traffic applications.

### Key Concepts

- **Primary (Master)**: Handles all writes (INSERT, UPDATE, DELETE) and some reads
- **Replica (Slave)**: Handles read-only queries (SELECT), replicates from primary
- **Replication Lag**: Time delay between primary and replica (typically <1 second)
- **Query Routing**: Intelligent routing of queries to appropriate database

### When to Implement

Consider read replicas when:
- ✅ Read traffic significantly exceeds write traffic (>70% reads)
- ✅ Primary database CPU/IO is consistently >70%
- ✅ Query latency increases during peak hours
- ✅ You need geographic distribution for lower latency

**Current Status**: Not yet implemented. Primary database handles all traffic.

---

## Current Architecture

### Single Primary Setup

```
┌─────────────────┐
│  Vault Backend  │
│    (FastAPI)    │
└────────┬────────┘
         │
         │ All queries (read + write)
         ▼
┌─────────────────┐
│    PostgreSQL   │
│     Primary     │
│   (read/write)  │
└─────────────────┘
```

**Characteristics:**
- Simple architecture, easy to maintain
- No replication lag concerns
- Single point of failure
- Limited horizontal scaling
- All load on one database

**Query Breakdown** (estimated):
- 60% SELECT (document retrieval, search, sync pulls)
- 30% INSERT (document creation, sync pushes, indexing)
- 10% UPDATE/DELETE (document updates, sync resolution)

---

## Read Replica Benefits

### Performance Improvements

1. **Reduced Primary Load**
   - Offload 60-80% of SELECT queries to replicas
   - Primary focuses on write operations
   - Better resource utilization

2. **Improved Query Latency**
   - Distribute read queries across multiple nodes
   - Geographic replicas reduce network latency
   - Parallel processing of read requests

3. **Higher Throughput**
   - Primary: 1000 writes/sec
   - Primary + 2 Replicas: 1000 writes/sec + 5000 reads/sec

### Availability Improvements

4. **Failover Capability**
   - Promote replica to primary if primary fails
   - Maintain read access even if primary is down
   - Faster disaster recovery

5. **Maintenance Windows**
   - Upgrade replicas without downtime
   - Test schema changes on replica first
   - Zero-downtime deployments

---

## Implementation Strategy

### Phase 1: Infrastructure Setup (Week 1-2)

#### 1.1 Database Provisioning

**PostgreSQL Streaming Replication:**
```bash
# Primary configuration (postgresql.conf)
wal_level = replica
max_wal_senders = 3
wal_keep_size = 1024
hot_standby = on
```

**Replica configuration:**
```bash
# Standby mode
primary_conninfo = 'host=primary-db port=5432 user=replicator password=xxx'
primary_slot_name = 'replica_1'
hot_standby = on
hot_standby_feedback = on
```

#### 1.2 Connection Configuration

Add replica DSN to configuration:

```python
# src/config/settings.py
from pydantic_settings import BaseSettings

class Settings(BaseSettings):
    # Primary database (read/write)
    database_url: str
    
    # Read replica (read-only)
    database_replica_url: Optional[str] = None
    
    # Enable replica routing
    use_read_replica: bool = False
    
    # Maximum acceptable replication lag (seconds)
    max_replication_lag_seconds: int = 5
```

#### 1.3 Database Connection Manager

Create routing abstraction:

```python
# src/db/routing.py
from typing import AsyncGenerator
from sqlalchemy.ext.asyncio import AsyncEngine, AsyncSession, create_async_engine
from src.config import settings

# Primary engine (read/write)
primary_engine: AsyncEngine = create_async_engine(
    settings.database_url,
    pool_size=20,
    max_overflow=10,
)

# Replica engine (read-only)
replica_engine: Optional[AsyncEngine] = None
if settings.database_replica_url and settings.use_read_replica:
    replica_engine = create_async_engine(
        settings.database_replica_url,
        pool_size=20,
        max_overflow=10,
        pool_pre_ping=True,  # Check connection health
    )


async def get_db_session(*, read_only: bool = False) -> AsyncGenerator[AsyncSession, None]:
    """Get database session with intelligent routing.
    
    Args:
        read_only: If True, route to replica if available
        
    Yields:
        Async database session
    """
    engine = primary_engine
    
    # Route to replica if:
    # 1. Read-only query requested
    # 2. Replica is configured and enabled
    # 3. Replica is healthy (checked via pool_pre_ping)
    if read_only and replica_engine is not None:
        engine = replica_engine
    
    async with AsyncSession(engine, expire_on_commit=False) as session:
        yield session
```

### Phase 2: Query Classification (Week 2-3)

#### 2.1 Mark Read-Only Endpoints

Update service dependencies:

```python
# src/services/deps.py
from typing import Annotated
from fastapi import Depends

# Write operations - always use primary
async def get_write_db() -> AsyncGenerator[AsyncSession, None]:
    async for session in get_db_session(read_only=False):
        yield session

# Read operations - use replica if available
async def get_read_db() -> AsyncGenerator[AsyncSession, None]:
    async for session in get_db_session(read_only=True):
        yield session

# Type aliases for clarity
WriteDB = Annotated[AsyncSession, Depends(get_write_db)]
ReadDB = Annotated[AsyncSession, Depends(get_read_db)]
```

#### 2.2 Update API Endpoints

```python
# src/api/routes/documents.py
from fastapi import APIRouter
from src.services.deps import ReadDB, WriteDB

router = APIRouter()

@router.get("/documents/{doc_id}")
async def get_document(doc_id: int, db: ReadDB):
    """Read-only: uses replica."""
    return await document_service.get_by_id(db, doc_id)

@router.post("/documents")
async def create_document(data: DocumentCreate, db: WriteDB):
    """Write operation: uses primary."""
    return await document_service.create(db, data)

@router.get("/documents/search")
async def search_documents(query: str, db: ReadDB):
    """Read-only search: uses replica."""
    return await search_service.search(db, query)
```

### Phase 3: Monitoring & Tuning (Week 3-4)

#### 3.1 Replication Lag Monitoring

```python
# src/observability/replication.py
from sqlalchemy import text

async def check_replication_lag(replica_session: AsyncSession) -> float:
    """Check replication lag in seconds.
    
    Returns:
        Replication lag in seconds, or -1 if unable to determine
    """
    try:
        result = await replica_session.execute(
            text("""
                SELECT EXTRACT(EPOCH FROM (now() - pg_last_xact_replay_timestamp()))
                AS lag_seconds
            """)
        )
        lag = result.scalar()
        return lag if lag is not None else -1
    except Exception as e:
        logger.error(f"Failed to check replication lag: {e}")
        return -1
```

Add to metrics:

```python
# Expose replication lag metric
replication_lag_gauge = meter.create_gauge(
    "db.replica.lag_seconds",
    description="Replication lag in seconds",
    unit="s"
)

# Update periodically
async def update_replication_metrics():
    """Background task to monitor replication."""
    while True:
        if replica_engine:
            async with AsyncSession(replica_engine) as session:
                lag = await check_replication_lag(session)
                replication_lag_gauge.set(lag, {"replica": "primary_replica"})
        
        await asyncio.sleep(10)  # Check every 10 seconds
```

#### 3.2 Fallback Strategy

If replica lag exceeds threshold, route to primary:

```python
async def get_db_session(*, read_only: bool = False) -> AsyncGenerator[AsyncSession, None]:
    """Get database session with lag-aware routing."""
    engine = primary_engine
    
    if read_only and replica_engine is not None:
        # Check if replica lag is acceptable
        lag = await check_replication_lag_cached()
        
        if lag < settings.max_replication_lag_seconds:
            engine = replica_engine
        else:
            logger.warning(f"Replica lag {lag}s exceeds threshold, using primary")
    
    async with AsyncSession(engine, expire_on_commit=False) as session:
        yield session
```

---

## Query Classification

### Read-Only Queries (Route to Replica)

**Safe for replica with replication lag tolerance:**

✅ **Document retrieval**
- `GET /documents/{id}` - Single document fetch
- `GET /documents` - List documents
- Acceptable lag: 1-5 seconds

✅ **Search operations**
- `POST /search` - Semantic search
- `GET /search/history` - Search history
- Acceptable lag: 1-5 seconds

✅ **Sync pulls (with caveat)**
- `POST /sync/pull` - Pull changes from server
- **Important**: Use primary for conflict-sensitive pulls
- Acceptable lag: 0-1 seconds

✅ **Analytics/reporting**
- `GET /analytics/stats` - Usage statistics
- `GET /documents/recent` - Recent documents
- Acceptable lag: 5-60 seconds

### Write Queries (Route to Primary)

**Must use primary database:**

❌ **Document creation/updates**
- `POST /documents` - Create document
- `PUT /documents/{id}` - Update document
- `DELETE /documents/{id}` - Delete document

❌ **Sync pushes**
- `POST /sync/push` - Push changes to server
- Must use primary to avoid conflicts

❌ **Conflict resolution**
- `POST /sync/conflicts/{id}/resolve`
- Critical to use primary for consistency

❌ **User management**
- `POST /users` - Create user
- `PUT /users/{id}` - Update user settings

### Mixed Queries (Route Based on Operation)

**Conditional routing:**

🔀 **Sync operations**
- Pull: Replica (if lag <1s, else primary)
- Push: Always primary
- Status: Replica

🔀 **Indexing**
- Job status: Replica
- Trigger indexing: Primary
- Search index: Replica

---

## Configuration

### Environment Variables

```bash
# .env.production

# Primary database (required)
DATABASE_URL=postgresql+asyncpg://user:pass@primary-db:5432/recall

# Read replica (optional)
DATABASE_REPLICA_URL=postgresql+asyncpg://user:pass@replica-db:5432/recall

# Enable replica routing
USE_READ_REPLICA=true

# Maximum acceptable lag (seconds)
MAX_REPLICATION_LAG_SECONDS=5

# Connection pool settings
DB_POOL_SIZE=20
DB_MAX_OVERFLOW=10
DB_POOL_TIMEOUT=30
```

### Docker Compose Example

```yaml
version: '3.8'

services:
  postgres-primary:
    image: postgres:15
    environment:
      POSTGRES_USER: recall
      POSTGRES_PASSWORD: ${DB_PASSWORD}
      POSTGRES_DB: recall
    volumes:
      - primary-data:/var/lib/postgresql/data
      - ./config/primary-postgresql.conf:/etc/postgresql/postgresql.conf
    command: postgres -c config_file=/etc/postgresql/postgresql.conf
    ports:
      - "5432:5432"
  
  postgres-replica:
    image: postgres:15
    environment:
      POSTGRES_USER: recall
      POSTGRES_PASSWORD: ${DB_PASSWORD}
      PGUSER: replicator
      PGPASSWORD: ${REPLICATION_PASSWORD}
    volumes:
      - replica-data:/var/lib/postgresql/data
      - ./config/replica-postgresql.conf:/etc/postgresql/postgresql.conf
    command: |
      bash -c "
      if [ ! -f /var/lib/postgresql/data/PG_VERSION ]; then
        pg_basebackup -h postgres-primary -D /var/lib/postgresql/data -U replicator -v -P
      fi
      postgres -c config_file=/etc/postgresql/postgresql.conf
      "
    ports:
      - "5433:5432"
    depends_on:
      - postgres-primary

volumes:
  primary-data:
  replica-data:
```

---

## Deployment

### Rollout Plan

**Step 1: Setup Replica (No routing)**
- Deploy replica database
- Configure streaming replication
- Verify replication is working
- Monitor lag and performance

**Step 2: Enable Routing (Gradual)**
```python
# Feature flag for gradual rollout
USE_READ_REPLICA_PERCENTAGE = 10  # Start with 10% of reads

if read_only and replica_engine:
    if random.random() < (USE_READ_REPLICA_PERCENTAGE / 100):
        engine = replica_engine
```

**Step 3: Increase Traffic**
- 10% → 25% → 50% → 75% → 100%
- Monitor metrics at each step
- Rollback if issues detected

**Step 4: Full Deployment**
- All read-only queries use replica
- Monitor for 1 week
- Document any issues

### Rollback Plan

If issues occur:
```python
# Emergency rollback
USE_READ_REPLICA = False

# Or set percentage to 0
USE_READ_REPLICA_PERCENTAGE = 0
```

Replica remains available but unused. No code changes required.

---

## Monitoring

### Key Metrics

**Replication Health:**
- `db.replica.lag_seconds` - Replication lag
- `db.replica.connected` - Replica connectivity status
- `db.replica.bytes_behind` - Bytes behind primary

**Query Distribution:**
- `db.query.count{target="primary"}` - Queries to primary
- `db.query.count{target="replica"}` - Queries to replica
- `db.query.fallback{reason="lag"}` - Fallback to primary due to lag

**Performance:**
- `db.query.latency{target="primary"}` - Primary query latency
- `db.query.latency{target="replica"}` - Replica query latency
- `db.pool.checked_out{target="replica"}` - Replica connection usage

### Alerts

**Critical:**
- Replication lag >10 seconds for >5 minutes
- Replica disconnected/unavailable
- Primary database down

**Warning:**
- Replication lag >5 seconds for >2 minutes
- Replica connection pool exhausted
- High fallback rate (>20%)

### Grafana Dashboard

Create dashboard with:
1. Replication lag over time
2. Query distribution (primary vs replica)
3. Query latency comparison
4. Connection pool utilization
5. Fallback rate

---

## Failover Strategy

### Automatic Failover

If primary fails:
```python
async def get_db_session(*args, **kwargs) -> AsyncGenerator[AsyncSession, None]:
    """Get DB session with automatic failover."""
    try:
        # Try primary first
        async with AsyncSession(primary_engine) as session:
            await session.execute(text("SELECT 1"))  # Health check
            yield session
    except Exception as e:
        logger.error(f"Primary database unavailable: {e}")
        
        if replica_engine:
            logger.warning("Failing over to replica for writes (DANGEROUS)")
            # Promote replica or use read-only mode
            async with AsyncSession(replica_engine) as session:
                yield session
        else:
            raise
```

**Important**: Automatic write failover requires replica promotion to primary.

### Manual Failover

**Promote replica to primary:**
```bash
# On replica server
pg_ctl promote -D /var/lib/postgresql/data

# Update application config
DATABASE_URL=postgresql+asyncpg://user:pass@replica-db:5432/recall

# Restart application
docker-compose restart backend
```

**Setup new replica from old primary:**
```bash
# Restore old primary as replica
pg_basebackup -h replica-db -D /var/lib/postgresql/data -U replicator
```

---

## Future Enhancements

### Geographic Distribution

Deploy replicas in multiple regions:
```
US-East (Primary) → US-West (Replica 1)
                  → EU-West (Replica 2)
                  → Asia-Pacific (Replica 3)
```

Route queries based on user location.

### Read-Your-Writes Consistency

Track last write timestamp per user:
```python
# After write
user_last_write[user_id] = time.time()

# Before read
if (time.time() - user_last_write[user_id]) < replication_lag:
    # Use primary to guarantee consistency
    engine = primary_engine
```

### Connection Pooling

Use **PgBouncer** between application and databases:
- Reduce connection overhead
- Better resource utilization
- Transaction-level pooling

---

## Summary

### Implementation Checklist

- [ ] Setup PostgreSQL streaming replication
- [ ] Configure primary for replication (wal_level, max_wal_senders)
- [ ] Create replica with pg_basebackup
- [ ] Verify replication is working
- [ ] Add DATABASE_REPLICA_URL to configuration
- [ ] Implement database routing abstraction
- [ ] Classify queries as read-only or write
- [ ] Update API endpoints with ReadDB/WriteDB
- [ ] Add replication lag monitoring
- [ ] Implement fallback strategy for high lag
- [ ] Deploy gradually with feature flags
- [ ] Monitor metrics and alerts
- [ ] Document failover procedures

### Performance Goals

- **Primary CPU**: Reduce from 80% → 40%
- **Read latency**: Improve p95 from 100ms → 50ms
- **Throughput**: Increase from 2000 → 5000 req/sec
- **Replication lag**: Maintain <1 second p99

---

**Last Updated**: 2025-11-17

*Update this document as read replica implementation progresses.*
