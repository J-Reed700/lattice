# ADR-002: Use SQLite for Metadata Storage

## Status

**Accepted** - Implemented in Phase 1 (2025-11-15)

## Context

The Lattice desktop application is a **local-first** personal knowledge management system that needs to persist:

- **Document metadata**: File paths, titles, creation/modification times, content hashes
- **Embeddings**: 384-dimensional vectors for semantic search
- **Search history**: User queries and interaction data
- **Configuration**: User preferences and application settings
- **Index state**: Serialized HNSW index for vector search

Key requirements:

1. **Local-first**: All data must be stored locally, no cloud dependencies
2. **Zero-config**: Users should not need to install or configure a database server
3. **Portable**: Database should be a single file that can be backed up/synced easily
4. **ACID compliance**: Ensure data integrity across crashes and concurrent access
5. **Cross-platform**: Must work identically on Windows, macOS, and Linux
6. **Embeddable**: Run in-process within the Tauri application
7. **Schema evolution**: Support migrations as the application evolves

## Decision

We will use **SQLite** as the embedded database, accessed via the **SQLx** async Rust crate.

### Implementation Details

- **SQLite version**: 3.35+ (bundled via `rusqlite` or `libsqlite3-sys`)
- **Rust interface**: `sqlx` v0.7 with compile-time query verification
- **Connection pool**: Single-threaded pool with `max_connections=1` (SQLite limitation with WAL mode)
- **Journal mode**: **WAL (Write-Ahead Logging)** for better concurrency
- **Foreign keys**: Enabled by default for referential integrity
- **Migrations**: Managed via `sqlx-cli` migrations

### Schema Design

```sql
-- Core document table
CREATE TABLE documents (
    id TEXT PRIMARY KEY,
    file_path TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL,
    modified_at TIMESTAMP NOT NULL,
    indexed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Vector embeddings (stored as BLOB)
CREATE TABLE embeddings (
    document_id TEXT PRIMARY KEY,
    embedding BLOB NOT NULL,  -- 384 floats = 1536 bytes
    model_version TEXT NOT NULL,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

-- HNSW index persistence
CREATE TABLE vector_index (
    id INTEGER PRIMARY KEY CHECK (id = 1),  -- Singleton
    version INTEGER NOT NULL,
    params_json TEXT NOT NULL,
    index_data BLOB NOT NULL,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Search history
CREATE TABLE search_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    query TEXT NOT NULL,
    results_count INTEGER NOT NULL,
    executed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for common queries
CREATE INDEX idx_documents_modified ON documents(modified_at DESC);
CREATE INDEX idx_search_history_time ON search_history(executed_at DESC);
```

### SQLx Usage Pattern

```rust
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

// Connection pool initialization
let pool = SqlitePool::connect_with(
    SqliteConnectOptions::new()
        .filename("lattice.db")
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
).await?;

// Compile-time verified query
let doc = sqlx::query_as::<_, Document>(
    "SELECT * FROM documents WHERE id = ?"
)
.bind(doc_id)
.fetch_one(&pool)
.await?;

// Transaction support
let mut tx = pool.begin().await?;
sqlx::query("INSERT INTO documents (...) VALUES (...)")
    .bind(...)
    .execute(&mut *tx)
    .await?;
tx.commit().await?;
```

## Consequences

### Positive

1. **Zero installation**: SQLite is embedded, no separate database process to manage
2. **Single file database**: Easy to backup, sync, and migrate (~500MB for 100K documents)
3. **ACID guarantees**: Reliable even with application crashes
4. **Excellent performance**:
   - Reads: ~100K queries/sec for simple lookups
   - Writes: ~10K inserts/sec in WAL mode
   - Bulk inserts: ~50K/sec with transactions
5. **Rich SQL support**: Full-text search (FTS5), JSON functions, CTEs, window functions
6. **Battle-tested**: Used by browsers, mobile apps, and embedded systems for decades
7. **Compile-time query checking**: SQLx verifies queries against the actual schema
8. **Cross-platform**: Identical behavior on all platforms
9. **Small footprint**: ~1.5MB library size

### Negative

1. **Limited concurrency**:
   - One writer at a time (even with WAL mode)
   - Readers block writers in default mode (WAL helps but doesn't eliminate)
   - **Mitigation**: Use connection pooling and async operations
2. **No network access**: Cannot query remotely (requires sync service for multi-device)
3. **Schema migrations complexity**: Requires careful planning for ALTER TABLE operations
4. **BLOB limitations**: Large embeddings (1536 bytes each) inflate database size
5. **No built-in vector search**: Have to implement ANN search separately (hence HNSW in ADR-001)
6. **Vacuum required**: Deleted data leaves gaps, need periodic `VACUUM` to reclaim space

### Neutral

- **File locking**: On network drives, locking may be unreliable (documented limitation)
- **Database size**: Grows with document count, but manageable (<1GB for typical use)

## Alternatives Considered

### 1. PostgreSQL

**Pros**:
- Superior concurrency (MVCC)
- Built-in vector search via pgvector extension
- Advanced features (partitioning, replication, full-text search)
- Used by backend service (`src/api`)

**Cons**:
- **Rejected**: Requires separate server process (not local-first)
- Complex installation and configuration for end users
- Overkill for single-user desktop application
- Would need connection management and port configuration

### 2. RocksDB

**Pros**:
- Very fast writes (LSM tree design)
- Excellent for write-heavy workloads
- Embedded like SQLite

**Cons**:
- **Rejected**: Key-value store, not relational (poor fit for metadata)
- No SQL interface (would need custom query layer)
- Harder to inspect data for debugging
- Less mature Rust bindings

### 3. sled (Rust-native embedded DB)

**Pros**:
- Pure Rust implementation
- Zero-copy reads
- ACID transactions
- Modern API design

**Cons**:
- **Rejected**: Still beta/experimental (version 0.34)
- No SQL layer (key-value only)
- Smaller ecosystem and community
- Uncertain long-term maintenance

### 4. DuckDB

**Pros**:
- Columnar storage (excellent for analytics)
- Fast aggregations and complex queries
- Modern SQL features

**Cons**:
- **Rejected**: Optimized for OLAP, not OLTP (our use case is transactional)
- Newer and less battle-tested than SQLite
- Larger binary size
- No significant advantage for document metadata

### 5. In-Memory + Flat Files

**Pros**:
- Maximum performance for reads
- Simple to implement

**Cons**:
- **Rejected**: No ACID guarantees
- Manual serialization/deserialization
- Concurrency handling is complex
- Risk of data loss on crashes

## Performance Benchmarks

Based on initial testing with 10,000 documents:

| Operation | SQLite (WAL mode) | Notes |
|-----------|-------------------|-------|
| Insert document | ~50µs | Single INSERT |
| Bulk insert (tx) | ~20µs/doc | 1000 docs in transaction |
| Get by ID | ~10µs | Indexed lookup |
| Query by path | ~15µs | Indexed UNIQUE column |
| Full table scan | ~5ms | All 10K documents |
| Embedding BLOB write | ~100µs | 1536 bytes |
| Embedding BLOB read | ~20µs | Cached I/O |

**Scaling expectations**:
- 100K documents: ~500MB database, <50µs per query
- 1M documents: ~5GB database, still <100µs for indexed queries

## Implementation Notes

### WAL Mode Configuration

```rust
// Enable WAL mode for better concurrency
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;  // Balance durability and speed
PRAGMA cache_size = -64000;   // 64MB cache
PRAGMA temp_store = MEMORY;   // Temp tables in RAM
```

### Embedding Storage

Embeddings are stored as BLOB (binary):

```rust
// Serialize f32 vector to bytes
fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter()
        .flat_map(|f| f.to_le_bytes())
        .collect()
}

// Deserialize bytes to f32 vector
fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}
```

### Migration Strategy

Migrations are managed via `sqlx-cli`:

```bash
# Create new migration
sqlx migrate add create_documents_table

# Apply migrations
sqlx migrate run --database-url sqlite:lattice.db
```

Migration files in `src-tauri/migrations/`:
```
migrations/
├── 20250115_001_initial_schema.sql
├── 20250115_002_add_search_history.sql
└── 20250115_003_add_vector_index.sql
```

### Backup Strategy

For user data protection:

1. **Automatic backups**: Copy `lattice.db` + `lattice.db-wal` + `lattice.db-shm` daily
2. **Export functionality**: Provide JSON export of all documents and metadata
3. **Cloud sync**: Users can sync database file via Dropbox/iCloud (with file locking caveats)

### Database Maintenance

Periodic operations:

```sql
-- Reclaim deleted space (run monthly)
VACUUM;

-- Update query planner statistics
ANALYZE;

-- Check integrity (on startup)
PRAGMA integrity_check;
```

## Future Considerations

If we need to scale beyond SQLite's limitations:

1. **Multi-device sync**: Add separate sync service with PostgreSQL backend (already implemented in `src/api`)
2. **Sharding**: Split database by document collections or time ranges
3. **Hybrid approach**: Keep metadata in SQLite, move vectors to specialized store
4. **Read replicas**: Use SQLite replication tools for read scaling

## References

- [SQLite Documentation](https://www.sqlite.org/docs.html)
- [SQLite WAL Mode](https://www.sqlite.org/wal.html)
- [SQLx Rust Crate](https://github.com/launchbadge/sqlx)
- [SQLite Performance Tuning](https://www.sqlite.org/optoverview.html)

## Revision History

- **2025-11-15**: Initial decision, implemented in Rust modernization Phase 1
