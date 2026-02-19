# File Storage Module

Content-addressed file storage system with automatic deduplication and reference counting.

## Quick Start

```rust
use crate::infrastructure::services::file_storage::{FileStorageService, FileRecord};

// Create service
let service = FileStorageService::new(vault_path, pool);

// Store a file (automatically deduplicated)
let record = service.store_file(path, "text/plain", None).await?;

// Get file path
let path = service.get_file_path(&record.id).await?;

// Delete file (uses reference counting)
service.delete_file(&record.id).await?;
```

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                FileStorageService                    │
│                  (Public API)                        │
└─────────────┬───────────────────────┬────────────────┘
              │                       │
    ┌─────────▼─────────┐   ┌────────▼─────────┐
    │   Database Layer   │   │  File System     │
    │    (queries.rs)    │   │   (fs_ops.rs)    │
    └─────────┬──────────┘   └────────┬─────────┘
              │                       │
              └───────────┬───────────┘
                          │
                  ┌───────▼────────┐
                  │   Hash Layer   │
                  │   (hash.rs)    │
                  └────────────────┘
```

## Module Structure

### Public Interface
- **mod.rs**: Module declarations and re-exports
- **Public types**: `FileStorageService`, `FileRecord`

### Data Layer
- **models.rs**: Data structures (`FileRecord`, constants)

### Storage Layer
- **hash.rs**: SHA256 hashing and validation
- **fs_ops.rs**: Atomic file operations

### Database Layer
- **queries.rs**: Database CRUD operations

### Service Layer
- **service.rs**: High-level orchestration

### Testing
- **tests.rs**: Complete test suite

## Features

### Content-Addressed Storage
Files are stored using their SHA256 hash:
```
vault/files/a1/b2c3d4e5...  # First 2 chars = directory
```

### Automatic Deduplication
Identical files share storage:
```rust
// Store same file twice - only stored once
let record1 = service.store_file(path1, "text/plain", None).await?;
let record2 = service.store_file(path2, "text/plain", None).await?;

// Same hash, incremented ref_count
assert_eq!(record1.content_hash, record2.content_hash);
assert_eq!(record2.ref_count, 2);
```

### Reference Counting
Files are deleted only when no longer referenced:
```rust
// First delete: decrements ref_count
service.delete_file(&id).await?;

// Second delete: removes file from disk
service.delete_file(&id).await?;
```

### Atomic Operations
File copies are atomic to prevent corruption:
```rust
// Writes to temp file, then atomic rename
copy_file_atomic(source, dest).await?;
```

### Integrity Verification
Hash verification ensures data integrity:
```rust
// Returns true if file exists and hash matches
let is_valid = service.verify_file(&file_id).await?;
```

## API Reference

### FileStorageService

#### Constructors

```rust
pub fn new(vault_path: PathBuf, pool: SqlitePool) -> Self
pub fn with_max_size(vault_path: PathBuf, pool: SqlitePool, max_file_size: u64) -> Self
```

#### Storage Operations

```rust
pub async fn store_file(
    &self,
    source_path: &Path,
    mime_type: &str,
    metadata: Option<serde_json::Value>,
) -> Result<FileRecord>
```

Stores a file with automatic deduplication. Returns existing record if file already exists.

```rust
pub async fn delete_file(&self, file_id: &str) -> Result<()>
```

Deletes a file using reference counting. Physically removes file when ref_count reaches 0.

#### Query Operations

```rust
pub async fn get_file_path(&self, file_id: &str) -> Result<PathBuf>
pub async fn get_file_by_hash(&self, hash: &str) -> Result<Option<FileRecord>>
pub async fn get_file_by_id(&self, file_id: &str) -> Result<Option<FileRecord>>
```

#### Maintenance Operations

```rust
pub async fn increment_ref_count(&self, file_id: &str) -> Result<()>
pub async fn cleanup_orphaned_files(&self) -> Result<usize>
pub async fn verify_file(&self, file_id: &str) -> Result<bool>
pub async fn mark_as_indexed(&self, file_id: &str) -> Result<()>
```

### FileRecord

```rust
pub struct FileRecord {
    pub id: String,
    pub content_hash: String,
    pub file_name: String,
    pub file_extension: Option<String>,
    pub mime_type: String,
    pub size_bytes: i64,
    pub storage_path: String,
    pub is_indexed: bool,
    pub created_at: i64,
    pub accessed_at: i64,
    pub ref_count: i32,
    pub metadata: Option<serde_json::Value>,
}
```

## Error Handling

### FileTooLarge
```rust
Err(AppError::FileTooLarge {
    path: "/path/to/file",
    size_bytes: 100_000_000,
    max_size_bytes: 50_000_000,
})
```

### FileStorage
```rust
Err(AppError::FileStorage("Failed to copy file".to_string()))
```

### NotFound
```rust
Err(AppError::NotFound("File not found: abc-123".to_string()))
```

### Database
```rust
Err(AppError::Database("Failed to insert record".to_string()))
```

## Testing

```bash
# Run all file storage tests
cargo test services::file_storage

# Run with output
cargo test services::file_storage -- --nocapture
```

### Test Coverage

- ✅ Store new file
- ✅ Deduplication
- ✅ Reference counting
- ✅ File deletion
- ✅ Integrity verification
- ✅ Orphaned file cleanup

## Performance Characteristics

### Storage
- **Time complexity**: O(n) for file size (streaming hash)
- **Space complexity**: O(1) (deduplicated)
- **Hash collision**: Detected by size comparison

### Deduplication
- **Lookup**: O(1) database query by hash
- **Storage savings**: Up to 100% for identical files

### Cleanup
- **Time complexity**: O(n) for orphaned files
- **Batch size**: Configurable

## Configuration

### Default Settings

```rust
const DEFAULT_MAX_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50MB
```

### Custom Settings

```rust
let service = FileStorageService::with_max_size(
    vault_path,
    pool,
    100 * 1024 * 1024  // 100MB
);
```

## Security Considerations

### Hash Validation
- SHA256 hashes are validated before use
- Hash collisions detected by size comparison

### Path Safety
- Storage paths generated from validated hashes
- No user input in paths

### File Size Limits
- Configurable max file size
- Prevents DoS via large files

### Atomic Operations
- Temp files used for writes
- Atomic renames prevent partial writes

## Maintenance

### Cleanup Orphaned Files

```rust
// Find and delete files with ref_count = 0
let count = service.cleanup_orphaned_files().await?;
println!("Cleaned up {} orphaned files", count);
```

### Verify Integrity

```rust
// Check if file exists and hash matches
if !service.verify_file(&file_id).await? {
    eprintln!("File {} is corrupted or missing", file_id);
}
```

## Migration Guide

See [REFACTORING.md](./REFACTORING.md) for details on the module refactoring.

### Backward Compatibility

All existing code continues to work:
```rust
// Before and after - same import
use crate::infrastructure::services::file_storage::FileStorageService;
```

---

**Module Size**: 1,094 lines across 7 files
**Largest File**: 357 lines (service.rs)
**Test Coverage**: 6 comprehensive tests
**Public API**: 2 types, 12 methods
