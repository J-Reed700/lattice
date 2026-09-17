//! Content-addressed file storage for the library.
//!
//! This module implements content-addressed storage that:
//! - Copies files to `~/.lattice/files/{sha256_hash}/original_filename.ext`
//! - Computes SHA256 hash of file content for deduplication
//! - Detects duplicates by hash (not path)
//! - Preserves original filename for UX
//! - Owns the deletion of those copies, and only those copies
//!
//! # Architecture
//!
//! Content-addressed storage solves the problem of files being moved or deleted
//! after indexing. Once a file is imported, it lives in the library permanently
//! at a hash-based location.
//!
//! # Storage Layout
//!
//! ```text
//! ~/.lattice/files/
//!   ├── a3d5f7.../
//!   │   └── document.pdf
//!   ├── b8e2c1.../
//!   │   └── notes.txt
//!   └── f4d9a2.../
//!       └── paper.pdf
//! ```
//!
//! # Concurrency
//!
//! Every mutation of a blob happens under a lock keyed by that blob's hash, so
//! an import and a delete of the same content serialize. An import additionally
//! takes a `BlobLease` before it copies and holds it past that lock, which is
//! what stops a delete arriving *after* the copy but *before* the document row
//! commits from removing a blob that is about to be referenced.

use crate::application::ports::content_addressed_storage_port::{
    is_blob_hash, BlobLeases, BlobReferenceCheck, BlobRemoval, ContentAddressedStoragePort,
    ImportedBlob, RetainReason,
};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tokio::fs;

/// Above this many cached per-hash locks, drop the ones nobody is holding.
/// A sweep touches every blob in the library, and a lock no task holds carries
/// no state worth keeping.
const LOCK_CACHE_HIGH_WATER: usize = 256;

/// Content-addressed file storage implementation.
///
/// Stores files in `~/.lattice/files/{hash}/filename` layout.
///
/// # Thread Safety
///
/// All operations are async and use Tokio's thread-safe file I/O. Mutations of
/// one blob are serialized by a per-hash async lock.
///
/// # Deduplication
///
/// Files with identical content (same SHA256) are stored only once.
/// Subsequent imports return the existing library path.
pub struct ContentAddressedStorage {
    /// Root directory for library storage (~/.lattice/files/)
    library_root: PathBuf,
    /// Imports currently in flight, by hash.
    leases: Arc<BlobLeases>,
    /// Per-hash mutual exclusion for copy and remove.
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl ContentAddressedStorage {
    /// Create a new content-addressed storage instance.
    ///
    /// Uses default library root: `~/.lattice/files/`
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidState` if home directory cannot be determined
    pub fn new() -> Result<Self> {
        let library_root = Self::default_library_root()?;
        Ok(Self::with_root(library_root))
    }

    /// Create storage with custom library root.
    pub fn with_root(library_root: PathBuf) -> Self {
        Self {
            library_root,
            leases: BlobLeases::new(),
            locks: Mutex::new(HashMap::new()),
        }
    }

    /// Get default library root directory.
    ///
    /// Returns `~/.lattice/files/`
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidState` if home directory cannot be determined
    pub(crate) fn default_library_root() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::InvalidState("Cannot determine home directory".to_string()))?;

        Ok(home.join(".lattice").join("files"))
    }

    /// The lock guarding mutations of one blob.
    fn hash_lock(&self, hash: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.locks.lock();
        if locks.len() > LOCK_CACHE_HIGH_WATER {
            // A strong count of 1 means this map holds the only handle, so no
            // task is inside — or waiting for — that lock.
            locks.retain(|_, lock| Arc::strong_count(lock) > 1);
        }
        Arc::clone(
            locks
                .entry(hash.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    }

    /// Compute SHA256 hash of file content.
    ///
    /// # Errors
    ///
    /// - `AppError::FileNotFound` if file doesn't exist
    /// - `AppError::FileRead` if cannot read file
    async fn compute_file_hash(&self, path: &Path) -> Result<String> {
        let content = fs::read(path).await.map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => AppError::FileNotFound {
                path: path.to_string_lossy().to_string(),
            },
            std::io::ErrorKind::PermissionDenied => {
                AppError::PermissionDenied(format!("Cannot read file: {}", path.display()))
            }
            _ => AppError::FileRead {
                path: path.to_string_lossy().to_string(),
                reason: e.to_string(),
            },
        })?;

        let mut hasher = Sha256::new();
        hasher.update(&content);
        let hash = hasher.finalize();

        Ok(format!("{:x}", hash))
    }

    /// Get directory path for a hash.
    ///
    /// Only ever called with a hash that passed `is_blob_hash`, which is what
    /// keeps `{hash}` a single, inert directory name.
    fn hash_directory(&self, hash: &str) -> PathBuf {
        self.library_root.join(hash)
    }

    /// Extract filename from path.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if path has no filename component
    fn extract_filename(path: &Path) -> Result<String> {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                AppError::InvalidInput(format!("Path has no filename: {}", path.display()))
            })
    }

    /// Place the source file's content under `{root}/{hash}`, or return the
    /// copy that is already there. The caller holds the per-hash lock.
    async fn place_in_library(&self, source_path: &Path, hash: &str) -> Result<PathBuf> {
        let hash_dir = self.hash_directory(hash);

        if hash_dir.exists() {
            let filename = Self::extract_filename(source_path)?;
            let library_path = hash_dir.join(&filename);

            if library_path.exists() {
                tracing::info!(
                    %hash,
                    path = %library_path.display(),
                    "File already in library (duplicate detected by hash)"
                );
                return Ok(library_path);
            }

            // Same content under a different filename is still a duplicate.
            let mut entries = fs::read_dir(&hash_dir).await.map_err(|e| {
                AppError::FileStorage(format!("Failed to read hash directory: {}", e))
            })?;

            if let Some(entry) = entries.next_entry().await.map_err(|e| {
                AppError::FileStorage(format!("Failed to read directory entry: {}", e))
            })? {
                let existing_path = entry.path();
                tracing::info!(
                    %hash,
                    existing = %existing_path.display(),
                    source = %source_path.display(),
                    "Duplicate content detected (different filename)"
                );
                return Ok(existing_path);
            }

            // Directory exists but is empty — fall through and copy.
        }

        fs::create_dir_all(&hash_dir).await.map_err(|e| {
            AppError::FileStorage(format!("Failed to create hash directory: {}", e))
        })?;

        let filename = Self::extract_filename(source_path)?;
        let library_path = hash_dir.join(&filename);

        fs::copy(source_path, &library_path)
            .await
            .map_err(|e| AppError::FileStorage(format!("Failed to copy file to library: {}", e)))?;

        tracing::info!(
            %hash,
            source = %source_path.display(),
            library = %library_path.display(),
            "File imported to library"
        );

        Ok(library_path)
    }

    /// Total size of every file under `dir`.
    ///
    /// `Ok(None)` when the directory itself does not exist.
    async fn directory_size(dir: &Path) -> Result<Option<u64>> {
        let mut pending = vec![dir.to_path_buf()];
        let mut total = 0_u64;
        let mut read_the_root = false;

        while let Some(current) = pending.pop() {
            let mut entries = match fs::read_dir(&current).await {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if read_the_root {
                        // Raced with something removing a subdirectory.
                        continue;
                    }
                    return Ok(None);
                }
                Err(error) => {
                    return Err(AppError::FileStorage(format!(
                        "Failed to read {}: {error}",
                        current.display()
                    )))
                }
            };
            read_the_root = true;

            while let Some(entry) = entries.next_entry().await.map_err(|error| {
                AppError::FileStorage(format!("Failed to read directory entry: {error}"))
            })? {
                match entry.metadata().await {
                    Ok(metadata) if metadata.is_dir() => pending.push(entry.path()),
                    Ok(metadata) => total = total.saturating_add(metadata.len()),
                    Err(error) => tracing::debug!(
                        path = %entry.path().display(),
                        %error,
                        "Could not size a library entry; counting it as zero"
                    ),
                }
            }
        }

        Ok(Some(total))
    }
}

#[async_trait]
impl ContentAddressedStoragePort for ContentAddressedStorage {
    async fn import_file(&self, source_path: &Path) -> Result<ImportedBlob> {
        let hash = self.compute_file_hash(source_path).await?;
        if !is_blob_hash(&hash) {
            return Err(AppError::InvalidState(format!(
                "Computed content hash is not a SHA-256 digest: {hash}"
            )));
        }

        let lock = self.hash_lock(&hash);
        let _guard = lock.lock().await;
        // Taken before the copy and handed to the caller: the blob is
        // protected from the moment it exists until the row referencing it has
        // committed and the caller drops the lease.
        let lease = self.leases.acquire(&hash);
        let path = self.place_in_library(source_path, &hash).await?;

        Ok(ImportedBlob { path, hash, lease })
    }

    async fn exists_by_hash(&self, hash: &str) -> Result<bool> {
        if !is_blob_hash(hash) {
            return Ok(false);
        }
        Ok(self.hash_directory(hash).exists())
    }

    async fn get_path_by_hash(&self, hash: &str) -> Result<Option<PathBuf>> {
        if !is_blob_hash(hash) {
            return Ok(None);
        }

        let mut entries = match fs::read_dir(self.hash_directory(hash)).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(AppError::FileStorage(format!(
                    "Failed to read hash directory: {error}"
                )))
            }
        };

        let entry = entries
            .next_entry()
            .await
            .map_err(|e| AppError::FileStorage(format!("Failed to read directory entry: {}", e)))?;

        Ok(entry.map(|entry| entry.path()))
    }

    fn owns(&self, path: &Path) -> bool {
        // `starts_with` compares components, but `root/../../etc` also starts
        // with `root`. A path that climbs out is not ours, whatever it prefixes.
        if path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return false;
        }
        path.starts_with(&self.library_root)
    }

    async fn list_hashes(&self) -> Result<Vec<String>> {
        let mut entries = match fs::read_dir(&self.library_root).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(AppError::FileStorage(format!(
                    "Failed to read library root: {error}"
                )))
            }
        };

        let mut hashes = Vec::new();
        while let Some(entry) = entries.next_entry().await.map_err(|error| {
            AppError::FileStorage(format!("Failed to read library entry: {error}"))
        })? {
            let name = entry.file_name().to_string_lossy().to_string();
            if !is_blob_hash(&name) {
                tracing::debug!(
                    entry = %name,
                    "Skipping library entry that is not a blob directory"
                );
                continue;
            }
            match entry.metadata().await {
                Ok(metadata) if metadata.is_dir() => hashes.push(name),
                Ok(_) => tracing::debug!(entry = %name, "Skipping non-directory library entry"),
                Err(error) => {
                    tracing::debug!(entry = %name, %error, "Skipping unreadable library entry");
                }
            }
        }

        Ok(hashes)
    }

    async fn remove_if_unreferenced(
        &self,
        hash: &str,
        refs: &dyn BlobReferenceCheck,
    ) -> Result<BlobRemoval> {
        if !is_blob_hash(hash) {
            return Err(AppError::InvalidInput(format!(
                "Not a library blob hash, refusing to touch the filesystem: {hash}"
            )));
        }

        let lock = self.hash_lock(hash);
        let _guard = lock.lock().await;

        if self.leases.is_leased(hash) {
            return Ok(BlobRemoval::Retained(RetainReason::Leased));
        }
        // Re-read references under the lock: a document committed since the
        // caller built its own list is still seen here.
        if refs.is_referenced(hash).await? {
            return Ok(BlobRemoval::Retained(RetainReason::Referenced));
        }

        let hash_dir = self.hash_directory(hash);
        let Some(bytes_freed) = Self::directory_size(&hash_dir).await? else {
            return Ok(BlobRemoval::Retained(RetainReason::Missing));
        };

        fs::remove_dir_all(&hash_dir).await.map_err(|error| {
            AppError::FileStorage(format!(
                "Failed to remove library blob {}: {error}",
                hash_dir.display()
            ))
        })?;

        tracing::info!(%hash, bytes_freed, "Removed unreferenced library blob");
        Ok(BlobRemoval::Removed { bytes_freed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Reference check with a fixed answer.
    struct Refs(bool);

    #[async_trait]
    impl BlobReferenceCheck for Refs {
        async fn is_referenced(&self, _hash: &str) -> Result<bool> {
            Ok(self.0)
        }
    }

    /// Reference check that records what it was asked.
    struct RecordingRefs {
        asked: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl BlobReferenceCheck for RecordingRefs {
        async fn is_referenced(&self, hash: &str) -> Result<bool> {
            self.asked.lock().push(hash.to_string());
            Ok(false)
        }
    }

    async fn write_source(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, content).await.unwrap();
        path
    }

    #[tokio::test]
    async fn test_import_file_new() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let source_file = write_source(temp_dir.path(), "source.txt", "Test content").await;

        let storage = ContentAddressedStorage::with_root(library_root.clone());

        let blob = storage.import_file(&source_file).await.unwrap();

        assert!(blob.path.starts_with(&library_root));
        assert_eq!(
            blob.path.parent().unwrap().file_name().unwrap().to_str(),
            Some(blob.hash.as_str())
        );
        assert_eq!(blob.path.file_name().unwrap(), "source.txt");

        assert!(blob.path.exists());
        let content = fs::read_to_string(&blob.path).await.unwrap();
        assert_eq!(content, "Test content");

        assert!(is_blob_hash(&blob.hash));
    }

    #[tokio::test]
    async fn test_import_file_duplicate() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let source_file1 = write_source(temp_dir.path(), "file1.txt", "Identical content").await;
        let source_file2 = write_source(temp_dir.path(), "file2.txt", "Identical content").await;

        let storage = ContentAddressedStorage::with_root(library_root);

        let first = storage.import_file(&source_file1).await.unwrap();
        let second = storage.import_file(&source_file2).await.unwrap();

        assert_eq!(first.hash, second.hash);
        assert_eq!(first.path.parent(), second.path.parent());
    }

    #[tokio::test]
    async fn test_import_file_different_content() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let source_file1 = write_source(temp_dir.path(), "file1.txt", "Content 1").await;
        let source_file2 = write_source(temp_dir.path(), "file2.txt", "Content 2").await;

        let storage = ContentAddressedStorage::with_root(library_root);

        let first = storage.import_file(&source_file1).await.unwrap();
        let second = storage.import_file(&source_file2).await.unwrap();

        assert_ne!(first.hash, second.hash);
        assert_ne!(first.path, second.path);
        assert!(first.path.exists());
        assert!(second.path.exists());
    }

    #[tokio::test]
    async fn test_exists_by_hash() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let source_file = write_source(temp_dir.path(), "file.txt", "Test content").await;

        let storage = ContentAddressedStorage::with_root(library_root);

        let blob = storage.import_file(&source_file).await.unwrap();

        assert!(storage.exists_by_hash(&blob.hash).await.unwrap());
        assert!(!storage.exists_by_hash(&"f".repeat(64)).await.unwrap());
        assert!(!storage.exists_by_hash("nonexistent_hash").await.unwrap());
    }

    #[tokio::test]
    async fn test_get_path_by_hash() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let source_file = write_source(temp_dir.path(), "file.txt", "Test content").await;

        let storage = ContentAddressedStorage::with_root(library_root);

        let blob = storage.import_file(&source_file).await.unwrap();

        let retrieved_path = storage.get_path_by_hash(&blob.hash).await.unwrap();
        assert_eq!(retrieved_path, Some(blob.path.clone()));
    }

    #[tokio::test]
    async fn test_get_path_by_hash_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ContentAddressedStorage::with_root(temp_dir.path().join("library"));

        assert_eq!(storage.get_path_by_hash("nonexistent").await.unwrap(), None);
        assert_eq!(
            storage.get_path_by_hash(&"a".repeat(64)).await.unwrap(),
            None
        );
    }

    #[test]
    fn test_extract_filename() {
        let filename =
            ContentAddressedStorage::extract_filename(Path::new("/path/to/file.txt")).unwrap();
        assert_eq!(filename, "file.txt");

        let filename = ContentAddressedStorage::extract_filename(Path::new("file.txt")).unwrap();
        assert_eq!(filename, "file.txt");
    }

    #[tokio::test]
    async fn test_import_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ContentAddressedStorage::with_root(temp_dir.path().join("library"));

        let result = storage
            .import_file(&temp_dir.path().join("nonexistent.txt"))
            .await;
        assert!(matches!(result, Err(AppError::FileNotFound { .. })));
    }

    #[tokio::test]
    async fn owns_only_paths_inside_the_library_root() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let storage = ContentAddressedStorage::with_root(library_root.clone());

        assert!(storage.owns(&library_root.join("a".repeat(64)).join("paper.pdf")));
        assert!(storage.owns(&library_root));
        assert!(!storage.owns(Path::new("/Users/someone/vault/note.md")));
        assert!(!storage.owns(&temp_dir.path().join("web-archive").join("article.md")));
        assert!(
            !storage.owns(&library_root.join("..").join("escape.txt")),
            "a path that climbs out of the root is not ours"
        );
    }

    #[tokio::test]
    async fn removes_an_unreferenced_blob_and_its_directory() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let source_file = write_source(temp_dir.path(), "paper.txt", "unreferenced").await;

        let storage = ContentAddressedStorage::with_root(library_root.clone());
        let blob = storage.import_file(&source_file).await.unwrap();
        let hash = blob.hash.clone();
        let blob_dir = library_root.join(&hash);
        // The import's lease ends here; nothing references the blob.
        drop(blob);

        let removal = storage
            .remove_if_unreferenced(&hash, &Refs(false))
            .await
            .unwrap();

        assert!(removal.was_removed());
        assert_eq!(removal.bytes_freed(), "unreferenced".len() as u64);
        assert!(!blob_dir.exists(), "the hash directory goes too");
        assert!(library_root.exists(), "the library root stays");
    }

    #[tokio::test]
    async fn keeps_a_blob_a_document_still_references() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let source_file = write_source(temp_dir.path(), "paper.txt", "referenced").await;

        let storage = ContentAddressedStorage::with_root(library_root.clone());
        let blob = storage.import_file(&source_file).await.unwrap();
        let hash = blob.hash.clone();
        drop(blob);

        let removal = storage
            .remove_if_unreferenced(&hash, &Refs(true))
            .await
            .unwrap();

        assert_eq!(removal, BlobRemoval::Retained(RetainReason::Referenced));
        assert!(library_root.join(&hash).exists());
    }

    #[tokio::test]
    async fn a_live_lease_blocks_removal() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let source_file = write_source(temp_dir.path(), "paper.txt", "leased").await;

        let storage = ContentAddressedStorage::with_root(library_root);
        // Lease still held: an import that has not committed its row yet.
        let blob = storage.import_file(&source_file).await.unwrap();
        let hash = blob.hash.clone();

        let refs = RecordingRefs {
            asked: Mutex::new(Vec::new()),
        };
        let removal = storage.remove_if_unreferenced(&hash, &refs).await.unwrap();

        assert_eq!(removal, BlobRemoval::Retained(RetainReason::Leased));
        assert!(
            refs.asked.lock().is_empty(),
            "a leased blob is refused before the reference check is even consulted"
        );
        assert!(blob.path.exists());

        // Once the import is done with it, the blob can go.
        drop(blob);
        assert!(storage
            .remove_if_unreferenced(&hash, &Refs(false))
            .await
            .unwrap()
            .was_removed());
    }

    #[tokio::test]
    async fn reports_a_blob_that_is_not_there() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ContentAddressedStorage::with_root(temp_dir.path().join("library"));

        let removal = storage
            .remove_if_unreferenced(&"e".repeat(64), &Refs(false))
            .await
            .unwrap();

        assert_eq!(removal, BlobRemoval::Retained(RetainReason::Missing));
    }

    #[tokio::test]
    async fn refuses_a_hash_that_is_not_a_blob_name() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let victim = temp_dir.path().join("precious.txt");
        fs::write(&victim, "do not delete me").await.unwrap();

        let storage = ContentAddressedStorage::with_root(library_root);

        let uppercase = "A".repeat(64);
        let too_short = "a".repeat(63);
        for candidate in [
            "../precious.txt",
            "..",
            "",
            uppercase.as_str(),
            too_short.as_str(),
            "not-a-hash",
        ] {
            let result = storage
                .remove_if_unreferenced(candidate, &Refs(false))
                .await;
            assert!(
                matches!(result, Err(AppError::InvalidInput(_))),
                "expected {candidate:?} to be refused"
            );
        }

        assert!(victim.exists());
    }

    #[tokio::test]
    async fn list_hashes_emits_only_blob_directories() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let source_file = write_source(temp_dir.path(), "paper.txt", "listed").await;

        let storage = ContentAddressedStorage::with_root(library_root.clone());
        let blob = storage.import_file(&source_file).await.unwrap();

        // Things that are not blobs: a stray file, an upper-case name, a short name.
        fs::write(library_root.join("README.md"), "hello")
            .await
            .unwrap();
        fs::create_dir_all(library_root.join("A".repeat(64)))
            .await
            .unwrap();
        fs::create_dir_all(library_root.join("tmp")).await.unwrap();

        let hashes = storage.list_hashes().await.unwrap();

        assert_eq!(hashes, vec![blob.hash.clone()]);
    }

    #[tokio::test]
    async fn list_hashes_on_a_library_that_does_not_exist_yet() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ContentAddressedStorage::with_root(temp_dir.path().join("never-created"));

        assert!(storage.list_hashes().await.unwrap().is_empty());
    }

    /// An import and a sweep race for the same content. Whatever the
    /// interleaving, the import must never come back holding a path the sweep
    /// deleted: that is exactly the document-less dangling row the lease
    /// exists to prevent.
    #[tokio::test]
    async fn concurrent_import_and_remove_never_strand_the_importer() {
        for _ in 0..25 {
            let temp_dir = TempDir::new().unwrap();
            let library_root = temp_dir.path().join("library");
            let source_file = write_source(temp_dir.path(), "racy.txt", "contended content").await;

            let storage = Arc::new(ContentAddressedStorage::with_root(library_root));
            // Seed the blob, then let go of it so a removal is possible at all.
            let hash = storage.import_file(&source_file).await.unwrap().hash;

            let importer = {
                let storage = Arc::clone(&storage);
                let source_file = source_file.clone();
                tokio::spawn(async move { storage.import_file(&source_file).await })
            };
            let remover = {
                let storage = Arc::clone(&storage);
                let hash = hash.clone();
                tokio::spawn(
                    async move { storage.remove_if_unreferenced(&hash, &Refs(false)).await },
                )
            };

            let blob = importer.await.unwrap().unwrap();
            let removal = remover.await.unwrap().unwrap();

            match removal {
                BlobRemoval::Removed { .. } => {
                    // The removal won the lock and finished before the import
                    // took its lease, so the import re-copied the blob.
                    assert!(
                        blob.path.exists(),
                        "an import that ran after a removal must have re-copied the blob"
                    );
                }
                BlobRemoval::Retained(reason) => {
                    assert_eq!(reason, RetainReason::Leased);
                    assert!(blob.path.exists(), "a leased blob is never deleted");
                }
            }

            // The import still holds its lease, so a second sweep also refuses.
            assert_eq!(
                storage
                    .remove_if_unreferenced(&hash, &Refs(false))
                    .await
                    .unwrap(),
                BlobRemoval::Retained(RetainReason::Leased)
            );
            assert!(blob.path.exists());
        }
    }
}
