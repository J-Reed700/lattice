//! Garbage collection for the content-addressed library.
//!
//! One blob lives under `~/.lattice/files/{sha256}/` per distinct piece of
//! imported content, and `documents.checksum` is that same SHA-256. A blob is
//! garbage exactly when no document row carries its checksum and no import
//! holds a lease on it.
//!
//! This service owns the reference check; the storage owns the locking and the
//! filesystem. Both halves are needed, which is why neither can decide a
//! deletion alone:
//!
//! - [`LibraryGc::release`] runs after a document referencing a hash was
//!   deleted, or after an import failed before committing its row.
//! - [`LibraryGc::sweep`] runs at startup and collects everything an
//!   interrupted import, a crash, or a deleted database left behind.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;

use crate::application::ports::{
    BlobReferenceCheck, BlobRemoval, ContentAddressedStoragePort, DocumentRepositoryPort,
};
use crate::shared::error::Result;

/// What a [`LibraryGc::sweep`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SweepReport {
    /// Blobs removed because nothing referenced them.
    pub removed: usize,
    /// Blobs left in place (referenced, leased, or already gone).
    pub retained: usize,
    /// Disk space reclaimed, in bytes.
    pub bytes_freed: u64,
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

/// Removes library blobs that no document references.
pub struct LibraryGc {
    library: Arc<dyn ContentAddressedStoragePort>,
    documents: Arc<dyn DocumentRepositoryPort>,
}

impl LibraryGc {
    /// Create a collector over one library and one document table.
    pub fn new(
        library: Arc<dyn ContentAddressedStoragePort>,
        documents: Arc<dyn DocumentRepositoryPort>,
    ) -> Self {
        Self { library, documents }
    }

    fn references(&self) -> DocumentReferences {
        DocumentReferences {
            documents: Arc::clone(&self.documents),
        }
    }

    /// Consider `hash` for removal, after a document referencing it was
    /// deleted or failed to commit.
    ///
    /// Deduplication means a blob can outlive the document that triggered
    /// this: the storage refuses while any other document still points at the
    /// hash, and while another import is mid-flight on it.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if `hash` is not a library blob hash
    /// - `AppError::Database` if the reference count cannot be read
    /// - `AppError::FileStorage` if the blob cannot be removed
    pub async fn release(&self, hash: &str) -> Result<BlobRemoval> {
        let removal = self
            .library
            .remove_if_unreferenced(hash, &self.references())
            .await?;

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

    /// Remove every blob no document references.
    ///
    /// Loads the referenced checksums once, then walks the blobs on disk. A
    /// document committed between those two reads is still safe: the storage
    /// re-checks references under its own per-hash lock before deleting
    /// anything.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if the checksum set cannot be read
    /// - `AppError::FileStorage` if the library cannot be listed
    ///
    /// A blob that cannot be removed is logged and counted as retained rather
    /// than failing the whole sweep; the next startup tries again.
    pub async fn sweep(&self) -> Result<SweepReport> {
        let referenced: HashSet<String> =
            self.documents.list_checksums().await?.into_iter().collect();
        let hashes = self.library.list_hashes().await?;
        let references = self.references();

        let mut report = SweepReport::default();
        for hash in hashes {
            if referenced.contains(&hash) {
                report.retained += 1;
                continue;
            }
            match self
                .library
                .remove_if_unreferenced(&hash, &references)
                .await
            {
                Ok(BlobRemoval::Removed { bytes_freed }) => {
                    report.removed += 1;
                    report.bytes_freed = report.bytes_freed.saturating_add(bytes_freed);
                }
                Ok(BlobRemoval::Retained(reason)) => {
                    tracing::debug!(%hash, ?reason, "Sweep kept a library blob");
                    report.retained += 1;
                }
                Err(error) => {
                    tracing::warn!(%hash, %error, "Could not collect a library blob; leaving it for the next sweep");
                    report.retained += 1;
                }
            }
        }

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::RepositoryPort;
    use crate::domain::entities::Document;
    use crate::domain::value_objects::{ChunkingStrategy, FileMetadata};
    use crate::infrastructure::persistence::repositories::mocks::MockDocumentRepository;
    use crate::infrastructure::storage::ContentAddressedStorage;
    use crate::shared::domain_types::ValidatedFilePath;
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
            crate::domain::value_objects::Checksum::new(checksum.to_string()).unwrap(),
            "some indexed body text".to_string(),
            ChunkingStrategy::FixedSize { size: 64 },
        )
        .unwrap()
    }

    #[tokio::test]
    async fn sweep_removes_only_the_blobs_no_document_references() {
        let temp = TempDir::new().unwrap();
        let storage = Arc::new(ContentAddressedStorage::with_root(temp.path().join("lib")));
        let documents = Arc::new(MockDocumentRepository::new());

        let (kept_hash, kept_path) = import(&storage, temp.path(), "kept.txt", "keep me").await;
        let (orphan_hash, orphan_path) =
            import(&storage, temp.path(), "orphan.txt", "orphaned blob").await;

        documents
            .save(&document_for(&kept_path, &kept_hash))
            .await
            .unwrap();

        let gc = LibraryGc::new(
            Arc::clone(&storage) as Arc<dyn ContentAddressedStoragePort>,
            Arc::clone(&documents) as Arc<dyn DocumentRepositoryPort>,
        );
        let report = gc.sweep().await.unwrap();

        assert_eq!(report.removed, 1);
        assert_eq!(report.retained, 1);
        assert_eq!(report.bytes_freed, "orphaned blob".len() as u64);
        assert!(kept_path.exists(), "a referenced blob survives the sweep");
        assert!(!orphan_path.exists(), "an unreferenced blob is collected");
        assert!(!temp.path().join("lib").join(&orphan_hash).exists());
    }

    #[tokio::test]
    async fn sweep_of_an_empty_library_reports_nothing() {
        let temp = TempDir::new().unwrap();
        let gc = LibraryGc::new(
            Arc::new(ContentAddressedStorage::with_root(temp.path().join("lib"))),
            Arc::new(MockDocumentRepository::new()),
        );

        assert_eq!(gc.sweep().await.unwrap(), SweepReport::default());
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
        );

        assert!(gc.release("../../etc").await.is_err());
    }
}
