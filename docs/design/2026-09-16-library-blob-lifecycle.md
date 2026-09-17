# Library blob lifecycle: one deletion pipeline, no orphans

Status: implemented (2026-09-16); verified with the full Rust and frontend suites

## Problem

Imported files are copied into the content-addressed library at
`~/.lattice/files/{sha256}/{filename}`, and `documents.file_path` points at
that copy. `documents.checksum` is the same SHA-256, so the blob directory name
and the document checksum agree. Deleting a document is supposed to remove the
copy. Today:

1. `DeleteDocumentUseCase` deletes `documents.file_path` through an unconfined
   `FileStoragePort`. Whatever the path is, it is removed. Nothing checks that
   Lattice owns the file, and nothing checks whether another document still
   references the blob.
2. There is no orphan sweep. A blob copied by an import that crashed before
   commit, or left behind by a deleted database, stays forever and gets packed
   into every backup.
3. Import and delete can race on the same hash: import copies the blob, a
   concurrent delete of the previous document removes the directory, then the
   new document commits pointing at a missing file.
4. A second, dead storage system exists: `FileStorageService` with the `files`
   and `file_references` tables, used only by the actor-based indexing engine.
   That engine is built in `setup/app.rs` and immediately dropped; the DI
   container always holds `DegradedIndexingService`. `reindex_web_archive`
   therefore always fails, and pause/resume only touch `IndexingState`.
5. `remove_indexed_folder` deletes document rows directly in SQL, bypassing
   vectors, sparse terms, summaries, the query cache, and blobs. Nothing writes
   `watch_folders`, and the UI never calls the command.
6. Backups pack the whole library directory, orphans included.

## Design

### Ownership rule

Lattice deletes a file from disk only when it owns the file:

- Library blobs under the content-addressed root. Removed by hash, never by
  arbitrary path, and only when no document references the hash and no import
  holds a lease on it.
- Web archive articles under `~/.lattice/web-archive`. Removed through
  `WebArchiveServiceTrait::delete_article`.
- Anything else (a vault note, a user file indexed in place) is left alone and
  logged.

### Port: `ContentAddressedStoragePort` (application/ports)

```rust
pub struct ImportedBlob { pub path: PathBuf, pub hash: String, pub lease: BlobLease }

/// Held by an import from copy until the document row commits. While any
/// lease for a hash is alive, that blob cannot be removed. Drop releases it.
pub struct BlobLease { .. }

pub enum BlobRemoval { Removed, Retained(RetainReason) }
pub enum RetainReason { Referenced, Leased, Missing }

#[async_trait]
pub trait BlobReferenceCheck: Send + Sync {
    async fn is_referenced(&self, hash: &str) -> Result<bool>;
}

#[async_trait]
pub trait ContentAddressedStoragePort: Send + Sync {
    async fn import_file(&self, source: &Path) -> Result<ImportedBlob>;
    async fn exists_by_hash(&self, hash: &str) -> Result<bool>;
    async fn get_path_by_hash(&self, hash: &str) -> Result<Option<PathBuf>>;
    /// True when `path` is inside the library root.
    fn owns(&self, path: &Path) -> bool;
    /// Every blob hash directory currently on disk.
    async fn list_hashes(&self) -> Result<Vec<String>>;
    /// Under the per-hash lock: refuse if leased, refuse if `refs` says the
    /// hash is still referenced, otherwise remove `{root}/{hash}` entirely.
    async fn remove_if_unreferenced(&self, hash: &str, refs: &dyn BlobReferenceCheck)
        -> Result<BlobRemoval>;
}
```

`ContentAddressedStorage` (infrastructure/storage) implements it with a keyed
async lock per hash and an in-flight lease registry. Hashes are validated as 64
lowercase hex characters before any filesystem call, so removal can never
touch anything but `{root}/{hash}`. The duplicate trait declaration inside the
infrastructure file goes away; only the application port remains.

### Repository additions: `DocumentRepositoryPort`

```rust
async fn count_by_checksum(&self, checksum: &str) -> Result<u64>;
async fn list_checksums(&self) -> Result<Vec<String>>;
```

### `LibraryGc` (features/indexing/library_gc.rs)

Application-layer service owning the reference check:

```rust
pub struct LibraryGc { library: Arc<dyn ContentAddressedStoragePort>, documents: Arc<dyn DocumentRepositoryPort> }
impl LibraryGc {
    /// After a document referencing `hash` was deleted or failed to commit.
    pub async fn release(&self, hash: &str) -> Result<BlobRemoval>;
    /// Remove every blob no document references. Runs at startup.
    pub async fn sweep(&self) -> Result<SweepReport>; // { removed, retained, bytes_freed }
}
```

Sweep loads `list_checksums()` into a set, walks `list_hashes()`, and calls
`remove_if_unreferenced` for each hash not in the set. The storage re-checks
references under its lock, so a document committed between the two reads is
safe.

### Import: lease from copy to commit

`IndexFileUseCase::prepare_for_indexing` keeps the `BlobLease` from
`import_file` inside the prepared document. `PreparedDocument` becomes a struct
(document, embeddings, library_path, imported_new, sparse_terms, lease) instead
of a tuple. `commit_prepared` drops the lease only after the unit of work
commits. If preparation fails after a fresh import (`imported_new`), the use
case calls `LibraryGc::release(hash)` so the failed import leaves no orphan.

### Delete: one pipeline

`DeleteDocumentUseCase` loses `FileStoragePort` and gains `LibraryGc`,
`ContentAddressedStoragePort` (for `owns`), and `WebArchiveServiceTrait` (for
`owns` and `delete_article`). Order:

1. Load the document (path, checksum). Not found is an error.
2. Notify summaries, then in one unit of work delete chunks and the document.
   Foreign keys cascade embeddings, sparse terms, images, tags, favorites,
   memberships, web assets.
3. Commit. Then remove vector keys, then dispose of the artifact:
   - library owns path: `library_gc.release(checksum)`
   - web archive owns path: `web_archive.delete_article(path)`
   - otherwise: log and leave the file.
4. Invalidate the query cache.

`delete_document`, `remove_indexed_file`, and `delete_file_index` all already
call this use case. `remove_indexed_folder` is removed rather than repaired.

### Startup sweep

`setup/app.rs` spawns `container.library_gc().sweep()` alongside the existing
download reconciliation tasks and logs the report.

### Backup packs only referenced blobs

`snapshot.rs` reads `SELECT DISTINCT checksum FROM documents` from the snapshot
database it just produced and packs only those top-level hash directories from
the files root. The referenced set comes from the very database being archived,
so archive and manifest always agree.

### Dead code removed

- `features/indexing/engine/{actor,builder,transaction,queue,events,error_ext}.rs`
  and the engine tests that only exercise them. `chunker`, `extraction`,
  `metadata_extractor`, `storage`, `state`, `progress`, `error` stay.
- `IndexingServiceTrait`, `DegradedIndexingService`, `MockIndexingService`,
  `IndexingDi.indexing_service`, `Container::indexing_service()`,
  `initialize_indexing_layer`.
- `infrastructure/services/file_storage/`, `FileStorageServiceTrait`,
  `MockFileStorageService`, and the `files` and `file_references` tables (edited
  out of the single squashed migration, per the no-legacy rule).
- `reindex_web_archive` command and its binding. `pause_indexing` and
  `resume_indexing` keep only the `IndexingState` flag.
- `remove_indexed_folder` command, `FileLibraryPort::remove_folder`, and their
  frontend wrapper. `watch_folders` stays (read by allowed roots).

## Verification

`cargo fmt --all -- --check`, `cargo clippy --all-targets`, `cargo test --lib`,
`export_bindings --check`, the layer, barrier, and SQL contract scripts, and
`npm run type-check && npm run lint && npm run contracts:check && npx vitest run`.
