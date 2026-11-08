//! Garbage collection for the content-addressed library.
//!
//! One blob lives under `~/.lattice/files/{sha256}/` per distinct piece of
//! imported content, and `documents.checksum` is that same SHA-256. A blob is
//! removed only when a record this database wrote names it (`file_intents`:
//! an import that never committed, or a delete that let it go), no document
//! references it and no import holds a lease on it. That no document
//! references a blob is not enough on its own: a restored backup, rows a
//! migration lost or another database sharing the library all leave real
//! files without rows here.
//!
//! A database whose sweep gate is shut does not own the library: another
//! database may reference any blob in it, so it removes none, and records
//! nothing past its next start, when its sweep clears them.
//!
//! This service owns the records and the reference check; the storage owns the
//! locking and the filesystem:
//!
//! - [`LibraryGc::release`] runs after a document referencing a hash was
//!   deleted.
//! - [`LibraryGc::import_failed`] runs after an import failed before
//!   committing its row.
//! - [`LibraryGc::sweep`] runs at startup, once the sweep gate is open for the
//!   database that owns the library (`library_owner`), and finishes what the
//!   records name.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;

use lattice_core::application::ports::content_addressed_storage_port::is_blob_hash;
use lattice_core::application::ports::{
    BlobReferenceCheck, BlobRemoval, ContentAddressedStoragePort, DocumentRepositoryPort,
    RetainReason,
};
use lattice_core::shared::error::Result;
use lattice_core::shared::fs::SweepGate;

use crate::infrastructure::storage::{FileIntents, FileStore, IntentKind};

/// What a [`LibraryGc::sweep`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SweepReport {
    /// Blobs a record named and nothing referenced, removed.
    pub removed: usize,
    /// Blobs a record named that were kept: referenced again, leased, already
    /// gone, or not removable now.
    pub retained: usize,
    /// Disk space reclaimed, in bytes.
    pub bytes_freed: u64,
    /// Blobs no document references and no record names: kept.
    pub unexplained: usize,
}

/// Reference check backed by the document table.
///
/// Handed to the storage so the "is anything still using this blob?" question
/// is answered *inside* the per-hash lock, closing the window in which a
/// document could commit between the sweep's read and the removal.
struct DocumentReferences {
    documents: Arc<dyn DocumentRepositoryPort>,
}

#[async_trait]
impl BlobReferenceCheck for DocumentReferences {
    async fn is_referenced(&self, hash: &str) -> Result<bool> {
        Ok(self.documents.count_by_checksum(hash).await? > 0)
    }
}

/// Whether a removal attempt settled what the records for its hash asked: the
/// blob is gone, claimed again, or could never have been a blob. A lease, a
/// shut gate or an error leaves them for the next start.
fn settles(hash: &str, outcome: &Result<BlobRemoval>) -> bool {
    match outcome {
        Ok(BlobRemoval::Retained(RetainReason::Leased | RetainReason::NotOwned)) => false,
        Ok(_) => true,
        Err(_) => !is_blob_hash(hash),
    }
}

/// Removes library blobs that a record names and no document references.
pub struct LibraryGc {
    library: Arc<dyn ContentAddressedStoragePort>,
    documents: Arc<dyn DocumentRepositoryPort>,
    intents: Option<FileIntents>,
    gate: Arc<SweepGate>,
}

impl LibraryGc {
    /// Create a collector over one library and one document table, which
    /// removes a blob only while `gate` is open. Without
    /// [`Self::with_intents`] it keeps no records, so its sweep removes nothing.
    pub fn new(
        library: Arc<dyn ContentAddressedStoragePort>,
        documents: Arc<dyn DocumentRepositoryPort>,
        gate: Arc<SweepGate>,
    ) -> Self {
        Self {
            library,
            documents,
            intents: None,
            gate,
        }
    }

    #[must_use]
    pub fn with_intents(mut self, intents: FileIntents) -> Self {
        self.intents = Some(intents);
        self
    }

    fn references(&self) -> DocumentReferences {
        DocumentReferences {
            documents: Arc::clone(&self.documents),
        }
    }

    async fn forget(&self, ids: &[i64]) {
        if let Some(intents) = &self.intents {
            if let Err(error) = intents.forget(ids).await {
                tracing::warn!(%error, "Could not clear a settled library record; the next start clears it");
            }
        }
    }

    async fn remove(&self, hash: &str) -> Result<BlobRemoval> {
        if !self.gate.is_open() && is_blob_hash(hash) {
            return Ok(BlobRemoval::Retained(RetainReason::NotOwned));
        }
        self.library
            .remove_if_unreferenced(hash, &self.references())
            .await
    }

    /// Record, before an import writes `hash`'s bytes, that it is about to.
    pub async fn begin_import(&self, hash: &str) -> Result<Option<i64>> {
        match &self.intents {
            Some(intents) => Ok(Some(
                intents
                    .record(FileStore::Library, IntentKind::Import, hash)
                    .await?,
            )),
            None => Ok(None),
        }
    }

    /// The import that recorded `intent` no longer needs the record: its
    /// document committed, or the content was already in the library.
    pub async fn end_import(&self, intent: Option<i64>) {
        if let Some(id) = intent {
            self.forget(&[id]).await;
        }
    }

    /// Give back `hash` after the import that recorded `intent` failed before
    /// its document committed. The record stays when the blob could not be
    /// dealt with, so the next start finishes it.
    pub async fn import_failed(&self, hash: &str, intent: Option<i64>) {
        let outcome = self.remove(hash).await;
        let settled = settles(hash, &outcome);
        match outcome {
            Ok(removal) => tracing::info!(
                %hash,
                ?removal,
                "Rolled back the library blob of a failed import"
            ),
            Err(error) => tracing::warn!(
                %hash,
                %error,
                "Could not roll back a failed import's blob; the next startup sweep will collect it"
            ),
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if settled {
            if let Some(id) = intent {
                self.forget(&[id]).await;
            }
        }
    }

    /// Consider `hash` for removal, after a document referencing it was
    /// deleted, and clear the release records of that delete once the blob is
    /// gone or another document still references it.
    ///
    /// Deduplication means a blob can outlive the document that triggered
    /// this: the storage refuses while any other document still points at the
    /// hash, and while another import is mid-flight on it.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if `hash` is not a library blob hash
    /// - `AppError::Database` if the records or the reference count cannot be read
    /// - `AppError::FileStorage` if the blob cannot be removed
    pub async fn release(&self, hash: &str) -> Result<BlobRemoval> {
        let released = match &self.intents {
            Some(intents) => intents.releases_of(FileStore::Library, hash).await?,
            None => Vec::new(),
        };
        let outcome = self.remove(hash).await;
        if settles(hash, &outcome) {
            self.forget(&released).await;
        }
        let removal = outcome?;

        match removal {
            BlobRemoval::Removed { bytes_freed } => {
                tracing::info!(%hash, bytes_freed, "Released library blob")
            }
            BlobRemoval::Retained(reason) => {
                tracing::debug!(%hash, ?reason, "Library blob kept")
            }
        }

        Ok(removal)
    }

    /// Remove every blob a record names that no document references, and
    /// count the blobs no record explains, which stay. While the gate is shut
    /// it removes nothing and clears the records, which this database will
    /// never act on; an import this process is running keeps its own.
    ///
    /// An import this process recorded may still be running, so its record is
    /// left alone. The storage re-checks references and leases under its own
    /// per-hash lock before deleting anything.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if the records or the checksum set cannot be read
    /// - `AppError::FileStorage` if the library cannot be listed
    ///
    /// A blob that cannot be removed is logged and counted as retained rather
    /// than failing the whole sweep; its record stays for the next start.
    pub async fn sweep(&self) -> Result<SweepReport> {
        let mut named: BTreeMap<String, Vec<i64>> = BTreeMap::new();
        let mut in_flight: HashSet<String> = HashSet::new();
        if let Some(intents) = &self.intents {
            for intent in intents.pending(FileStore::Library).await? {
                if intent.kind == IntentKind::Import && intent.this_run {
                    in_flight.insert(intent.name);
                } else {
                    named.entry(intent.name).or_default().push(intent.id);
                }
            }
        }
        if !self.gate.is_open() {
            tracing::debug!("Left the library alone: this database does not own its files");
            let ids: Vec<i64> = named.into_values().flatten().collect();
            self.forget(&ids).await;
            return Ok(SweepReport::default());
        }

        let mut report = SweepReport::default();
        for (hash, ids) in &named {
            let outcome = self.remove(hash).await;
            let settled = settles(hash, &outcome);
            match outcome {
                Ok(BlobRemoval::Removed { bytes_freed }) => {
                    report.removed += 1;
                    report.bytes_freed = report.bytes_freed.saturating_add(bytes_freed);
                }
                Ok(BlobRemoval::Retained(reason)) => {
                    tracing::debug!(%hash, ?reason, "Sweep kept a library blob a record names");
                    report.retained += 1;
                }
                Err(error) => {
                    tracing::warn!(%hash, %error, "Could not collect a library blob; leaving it for the next sweep");
                    report.retained += 1;
                }
            }
            if settled {
                self.forget(ids).await;
            }
        }

        let referenced: HashSet<String> =
            self.documents.list_checksums().await?.into_iter().collect();
        report.unexplained = self
            .library
            .list_hashes()
            .await?
            .into_iter()
            .filter(|hash| {
                !referenced.contains(hash) && !named.contains_key(hash) && !in_flight.contains(hash)
            })
            .count();

        Ok(report)
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use crate::infrastructure::storage::FileIntents;

    /// A migrated in-memory database and the records kept in it.
    pub(crate) async fn file_intents() -> (sqlx::SqlitePool, FileIntents) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let intents = FileIntents::new(pool.clone());
        (pool, intents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::persistence::repositories::mocks::MockDocumentRepository;
    use crate::infrastructure::storage::ContentAddressedStorage;
    use lattice_core::application::ports::RepositoryPort;
    use lattice_core::domain::entities::Document;
    use lattice_core::domain::value_objects::{ChunkingStrategy, FileMetadata};
    use lattice_core::shared::types::ValidatedFilePath;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;
    use tokio::fs;

    /// Import `content` into `storage` and register a document for it.
    async fn import(
        storage: &ContentAddressedStorage,
        source_dir: &Path,
        name: &str,
        content: &str,
    ) -> (String, PathBuf) {
        let source = source_dir.join(name);
        fs::write(&source, content).await.unwrap();
        let blob = storage.import_file(&source).await.unwrap();
        (blob.hash.clone(), blob.path.clone())
    }

    fn open_gate() -> Arc<SweepGate> {
        let gate = SweepGate::default();
        gate.open();
        Arc::new(gate)
    }

    fn document_for(path: &Path, checksum: &str) -> Document {
        let metadata = FileMetadata::new(
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("file.txt")
                .to_string(),
            "text/plain".to_string(),
            16,
            chrono::Utc::now(),
        )
        .unwrap();
        Document::from_file(
            ValidatedFilePath::new(path.to_path_buf()).unwrap(),
            metadata,
            lattice_core::domain::value_objects::Checksum::new(checksum.to_string()).unwrap(),
            "some indexed body text".to_string(),
            ChunkingStrategy::FixedSize { size: 64 },
        )
        .unwrap()
    }

    /// A blob no document references is not garbage on that alone: the
    /// database may be a restored backup, or one that lost rows.
    #[tokio::test]
    async fn sweep_keeps_the_blobs_no_record_names() {
        let temp = TempDir::new().unwrap();
        let storage = Arc::new(ContentAddressedStorage::with_root(temp.path().join("lib")));
        let documents = Arc::new(MockDocumentRepository::new());

        let (kept_hash, kept_path) = import(&storage, temp.path(), "kept.txt", "keep me").await;
        let (_, unnamed_path) =
            import(&storage, temp.path(), "unnamed.txt", "no row names me").await;

        documents
            .save(&document_for(&kept_path, &kept_hash))
            .await
            .unwrap();

        let gc = LibraryGc::new(
            Arc::clone(&storage) as Arc<dyn ContentAddressedStoragePort>,
            Arc::clone(&documents) as Arc<dyn DocumentRepositoryPort>,
            open_gate(),
        );
        let report = gc.sweep().await.unwrap();

        assert_eq!(report.removed, 0);
        assert_eq!(report.bytes_freed, 0);
        assert!(kept_path.exists(), "a referenced blob survives the sweep");
        assert!(unnamed_path.exists(), "a blob no record names is kept");
    }

    #[tokio::test]
    async fn sweep_of_an_empty_library_reports_nothing() {
        let temp = TempDir::new().unwrap();
        let gc = LibraryGc::new(
            Arc::new(ContentAddressedStorage::with_root(temp.path().join("lib"))),
            Arc::new(MockDocumentRepository::new()),
            open_gate(),
        );

        assert_eq!(gc.sweep().await.unwrap(), SweepReport::default());
    }

    #[tokio::test]
    async fn a_shut_gate_leaves_every_blob_alone() {
        let temp = TempDir::new().unwrap();
        let storage = Arc::new(ContentAddressedStorage::with_root(temp.path().join("lib")));
        let (_, orphan_path) = import(&storage, temp.path(), "orphan.txt", "orphaned blob").await;
        let gc = LibraryGc::new(
            Arc::clone(&storage) as Arc<dyn ContentAddressedStoragePort>,
            Arc::new(MockDocumentRepository::new()),
            Arc::new(SweepGate::default()),
        );

        let report = gc.sweep().await.unwrap();

        assert_eq!(report, SweepReport::default());
        assert!(
            orphan_path.exists(),
            "a shut gate keeps an unreferenced blob"
        );
    }

    #[tokio::test]
    async fn release_frees_a_blob_whose_last_document_is_gone() {
        let temp = TempDir::new().unwrap();
        let storage = Arc::new(ContentAddressedStorage::with_root(temp.path().join("lib")));
        let documents = Arc::new(MockDocumentRepository::new());

        let (hash, path) = import(&storage, temp.path(), "paper.txt", "released").await;
        let gc = LibraryGc::new(
            Arc::clone(&storage) as Arc<dyn ContentAddressedStoragePort>,
            Arc::clone(&documents) as Arc<dyn DocumentRepositoryPort>,
            open_gate(),
        );

        assert!(gc.release(&hash).await.unwrap().was_removed());
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn release_keeps_a_blob_another_document_still_shares() {
        let temp = TempDir::new().unwrap();
        let storage = Arc::new(ContentAddressedStorage::with_root(temp.path().join("lib")));
        let documents = Arc::new(MockDocumentRepository::new());

        let (hash, path) = import(&storage, temp.path(), "paper.txt", "shared content").await;
        // A second document still points at the same content.
        documents.save(&document_for(&path, &hash)).await.unwrap();

        let gc = LibraryGc::new(
            Arc::clone(&storage) as Arc<dyn ContentAddressedStoragePort>,
            Arc::clone(&documents) as Arc<dyn DocumentRepositoryPort>,
            open_gate(),
        );

        assert!(!gc.release(&hash).await.unwrap().was_removed());
        assert!(path.exists());
    }

    #[tokio::test]
    async fn release_refuses_a_hash_that_is_not_a_blob_name() {
        let temp = TempDir::new().unwrap();
        let gc = LibraryGc::new(
            Arc::new(ContentAddressedStorage::with_root(temp.path().join("lib"))),
            Arc::new(MockDocumentRepository::new()),
            open_gate(),
        );

        assert!(gc.release("../../etc").await.is_err());
    }

    /// A collector over `storage` and `documents` that keeps its records in a
    /// migrated database of its own.
    async fn recording(
        storage: &Arc<ContentAddressedStorage>,
        documents: &Arc<MockDocumentRepository>,
    ) -> (LibraryGc, FileIntents, sqlx::SqlitePool) {
        let (pool, intents) = test_support::file_intents().await;
        let gc = LibraryGc::new(
            Arc::clone(storage) as Arc<dyn ContentAddressedStoragePort>,
            Arc::clone(documents) as Arc<dyn DocumentRepositoryPort>,
            open_gate(),
        )
        .with_intents(intents.clone());
        (gc, intents, pool)
    }

    /// A record a process that has since ended wrote.
    async fn earlier_record(pool: &sqlx::SqlitePool, kind: IntentKind, hash: &str) {
        sqlx::query(
            "INSERT INTO file_intents (store, kind, name, run_id) \
             VALUES ('library', ?, ?, 'an earlier run')",
        )
        .bind(kind.as_str())
        .bind(hash)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn an_import_whose_record_outlived_its_process_is_collected() {
        let temp = TempDir::new().unwrap();
        let storage = Arc::new(ContentAddressedStorage::with_root(temp.path().join("lib")));
        let documents = Arc::new(MockDocumentRepository::new());
        let (gc, intents, pool) = recording(&storage, &documents).await;
        let (hash, path) = import(
            &storage,
            temp.path(),
            "crashed.txt",
            "copied, never committed",
        )
        .await;
        let (_, unnamed) = import(&storage, temp.path(), "unnamed.txt", "no record names me").await;
        earlier_record(&pool, IntentKind::Import, &hash).await;

        let report = gc.sweep().await.unwrap();

        assert_eq!(report.removed, 1);
        assert_eq!(report.unexplained, 1);
        assert!(!path.exists());
        assert!(unnamed.exists());
        assert!(intents
            .pending(FileStore::Library)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn an_import_this_process_is_still_running_is_left_alone() {
        let temp = TempDir::new().unwrap();
        let storage = Arc::new(ContentAddressedStorage::with_root(temp.path().join("lib")));
        let documents = Arc::new(MockDocumentRepository::new());
        let (gc, intents, _pool) = recording(&storage, &documents).await;
        let (hash, path) = import(&storage, temp.path(), "running.txt", "being imported").await;
        gc.begin_import(&hash).await.unwrap();

        let report = gc.sweep().await.unwrap();

        assert_eq!(report, SweepReport::default());
        assert!(path.exists());
        assert_eq!(intents.pending(FileStore::Library).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_delete_whose_removal_did_not_finish_is_collected_at_the_next_start() {
        let temp = TempDir::new().unwrap();
        let storage = Arc::new(ContentAddressedStorage::with_root(temp.path().join("lib")));
        let documents = Arc::new(MockDocumentRepository::new());
        let (gc, intents, pool) = recording(&storage, &documents).await;
        let (hash, path) = import(&storage, temp.path(), "deleted.txt", "its document went").await;
        earlier_record(&pool, IntentKind::Release, &hash).await;

        let report = gc.sweep().await.unwrap();

        assert_eq!(report.removed, 1);
        assert_eq!(report.bytes_freed, "its document went".len() as u64);
        assert!(!path.exists());
        assert!(intents
            .pending(FileStore::Library)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn a_record_whose_hash_is_referenced_again_is_cleared_and_the_blob_kept() {
        let temp = TempDir::new().unwrap();
        let storage = Arc::new(ContentAddressedStorage::with_root(temp.path().join("lib")));
        let documents = Arc::new(MockDocumentRepository::new());
        let (gc, intents, pool) = recording(&storage, &documents).await;
        let (hash, path) = import(&storage, temp.path(), "again.txt", "imported again").await;
        earlier_record(&pool, IntentKind::Release, &hash).await;
        earlier_record(&pool, IntentKind::Import, &hash).await;
        documents.save(&document_for(&path, &hash)).await.unwrap();

        let report = gc.sweep().await.unwrap();

        assert_eq!(report.removed, 0);
        assert_eq!(report.retained, 1);
        assert!(path.exists());
        assert!(intents
            .pending(FileStore::Library)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn release_clears_the_record_of_its_delete_once_the_blob_is_gone() {
        let temp = TempDir::new().unwrap();
        let storage = Arc::new(ContentAddressedStorage::with_root(temp.path().join("lib")));
        let documents = Arc::new(MockDocumentRepository::new());
        let (gc, intents, pool) = recording(&storage, &documents).await;
        let (hash, path) = import(&storage, temp.path(), "paper.txt", "released").await;
        earlier_record(&pool, IntentKind::Release, &hash).await;

        assert!(gc.release(&hash).await.unwrap().was_removed());
        assert!(!path.exists());
        assert!(intents
            .pending(FileStore::Library)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn a_failed_import_gives_back_its_blob_and_its_record() {
        let temp = TempDir::new().unwrap();
        let storage = Arc::new(ContentAddressedStorage::with_root(temp.path().join("lib")));
        let documents = Arc::new(MockDocumentRepository::new());
        let (gc, intents, _pool) = recording(&storage, &documents).await;
        let (hash, path) = import(&storage, temp.path(), "failed.txt", "never committed").await;
        let intent = gc.begin_import(&hash).await.unwrap();

        gc.import_failed(&hash, intent).await;

        assert!(!path.exists());
        assert!(intents
            .pending(FileStore::Library)
            .await
            .unwrap()
            .is_empty());
    }
}
