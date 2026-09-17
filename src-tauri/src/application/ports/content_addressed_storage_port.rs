//! Content-addressed storage port.
//!
//! This port defines the interface for the library of imported files: the
//! copy that Lattice owns and is therefore allowed to delete.
//!
//! # Purpose
//!
//! - Import files to library storage (`~/.lattice/files/{hash}/filename`)
//! - Detect duplicates by content hash (not file path)
//! - Preserve original filenames for UX
//! - Decide ownership: only files under the library root may be removed, and
//!   only by hash, never by an arbitrary path handed in from a row
//! - Keep an import and a concurrent delete of the same content from racing
//!
//! # Ownership rule
//!
//! Removal is expressed as [`ContentAddressedStoragePort::remove_if_unreferenced`],
//! which refuses while any import holds a [`BlobLease`] on the hash and while
//! the caller's [`BlobReferenceCheck`] still finds a document pointing at it.
//! The hash is validated as 64 lowercase hex characters before any filesystem
//! call, so a removal can never reach outside `{library_root}/{hash}`.
//!
//! # Infrastructure Implementations
//!
//! - `ContentAddressedStorage` - SHA-256 based storage in `~/.lattice/files/`

use crate::shared::result::Result;
use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Number of hex characters in a library blob hash (SHA-256).
pub const BLOB_HASH_LEN: usize = 64;

/// True when `value` is exactly a SHA-256 hex digest in lowercase.
///
/// Every path the library builds is `{root}/{hash}/{filename}`, so validating
/// the hash is what keeps `{hash}` a single directory name: no separators, no
/// `..`, no absolute path, nothing that could escape the library root.
#[must_use]
pub fn is_blob_hash(value: &str) -> bool {
    value.len() == BLOB_HASH_LEN
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// A file that now lives in the content-addressed library.
///
/// The `lease` must be held until the document row referencing this blob has
/// committed; dropping it earlier lets a concurrent sweep or delete remove the
/// blob out from under the row being written.
pub struct ImportedBlob {
    /// Path of the copy inside the library.
    pub path: PathBuf,
    /// SHA-256 of the content, hex-encoded, lowercase.
    pub hash: String,
    /// Held from the copy until the referencing row commits.
    pub lease: BlobLease,
}

impl fmt::Debug for ImportedBlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImportedBlob")
            .field("path", &self.path)
            .field("hash", &self.hash)
            .finish_non_exhaustive()
    }
}

/// Registry of in-flight imports, keyed by blob hash.
///
/// Reference-counted rather than a set: two imports of the same content can
/// overlap, and the blob only stops being protected when the last of them has
/// finished.
#[derive(Default)]
pub struct BlobLeases {
    in_flight: Mutex<HashMap<String, usize>>,
}

impl BlobLeases {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Take a lease on `hash`, held until the returned value is dropped.
    #[must_use]
    pub fn acquire(self: &Arc<Self>, hash: &str) -> BlobLease {
        *self
            .in_flight
            .lock()
            .entry(hash.to_string())
            .or_insert(0_usize) += 1;
        BlobLease {
            registry: Some(Arc::clone(self)),
            hash: hash.to_string(),
        }
    }

    /// True while at least one import holds a lease on `hash`.
    #[must_use]
    pub fn is_leased(&self, hash: &str) -> bool {
        self.in_flight.lock().get(hash).is_some_and(|n| *n > 0)
    }

    fn release(&self, hash: &str) {
        let mut in_flight = self.in_flight.lock();
        let Some(count) = in_flight.get_mut(hash) else {
            return;
        };
        *count = count.saturating_sub(1);
        if *count == 0 {
            in_flight.remove(hash);
        }
    }
}

/// Held by an import from the moment the blob is copied until the document row
/// referencing it commits. While any lease for a hash is alive, that blob
/// cannot be removed. Dropping the lease releases it.
pub struct BlobLease {
    registry: Option<Arc<BlobLeases>>,
    hash: String,
}

impl BlobLease {
    /// A lease that protects nothing, for test doubles and for blobs that were
    /// never copied by this process.
    #[must_use]
    pub fn detached(hash: &str) -> Self {
        Self {
            registry: None,
            hash: hash.to_string(),
        }
    }

    /// The blob hash this lease protects.
    #[must_use]
    pub fn hash(&self) -> &str {
        &self.hash
    }
}

impl fmt::Debug for BlobLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlobLease")
            .field("hash", &self.hash)
            .field("held", &self.registry.is_some())
            .finish()
    }
}

impl Drop for BlobLease {
    fn drop(&mut self) {
        if let Some(registry) = &self.registry {
            registry.release(&self.hash);
        }
    }
}

/// Why a blob was kept rather than removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetainReason {
    /// A document row still references this hash.
    Referenced,
    /// An import is mid-flight on this hash.
    Leased,
    /// Nothing is stored under this hash.
    Missing,
}

/// Outcome of a removal attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobRemoval {
    /// The blob directory was removed.
    ///
    /// `bytes_freed` is measured before the removal; only the storage knows
    /// the blob's size, and reporting it here saves every caller from
    /// re-walking a directory that no longer exists.
    Removed {
        /// Bytes reclaimed from disk.
        bytes_freed: u64,
    },
    /// The blob was left in place.
    Retained(RetainReason),
}

impl BlobRemoval {
    /// True when the blob directory was removed.
    #[must_use]
    pub fn was_removed(&self) -> bool {
        matches!(self, Self::Removed { .. })
    }

    /// Bytes reclaimed by this removal; zero when the blob was retained.
    #[must_use]
    pub fn bytes_freed(&self) -> u64 {
        match self {
            Self::Removed { bytes_freed } => *bytes_freed,
            Self::Retained(_) => 0,
        }
    }
}

/// Tells the storage whether a hash is still referenced by persisted state.
///
/// Supplied by the caller and consulted *inside* the storage's per-hash lock,
/// so a document committed between a caller's own read and the removal is
/// still seen.
#[async_trait]
pub trait BlobReferenceCheck: Send + Sync {
    /// True when at least one persisted record points at this blob.
    async fn is_referenced(&self, hash: &str) -> Result<bool>;
}

/// Port for content-addressed file storage.
///
/// Implementations must:
/// - Store files in content-addressed layout (hash-based directories)
/// - Detect duplicates by content hash (not file path)
/// - Preserve original filenames for UX
/// - Be thread-safe (`Send + Sync`)
#[async_trait]
pub trait ContentAddressedStoragePort: Send + Sync {
    /// Import a file into the library and lease the blob.
    ///
    /// Copies the source file to `{library_root}/{hash}/{filename}`. Content
    /// that is already in the library is not copied again; either way the
    /// returned [`ImportedBlob`] carries a lease that keeps the blob alive
    /// until the caller has committed the row referencing it.
    ///
    /// # Errors
    ///
    /// - `AppError::FileNotFound` if source file doesn't exist
    /// - `AppError::PermissionDenied` if cannot read source or write to library
    /// - `AppError::FileRead` if cannot read source file
    /// - `AppError::FileStorage` if cannot write to library
    async fn import_file(&self, source_path: &Path) -> Result<ImportedBlob>;

    /// Check if content with this hash is in the library.
    ///
    /// A hash that is not a well-formed blob hash is simply absent; no
    /// filesystem call is made for it.
    ///
    /// # Errors
    ///
    /// - `AppError::FileStorage` if library directory cannot be accessed
    async fn exists_by_hash(&self, hash: &str) -> Result<bool>;

    /// Get the library path of the file stored under `hash`.
    ///
    /// # Errors
    ///
    /// - `AppError::FileStorage` if library directory cannot be accessed
    async fn get_path_by_hash(&self, hash: &str) -> Result<Option<PathBuf>>;

    /// True when `path` is inside the library root.
    ///
    /// This is the ownership test for deletion: a path that fails it belongs
    /// to the user (a vault note, a file indexed in place) and is never
    /// removed by Lattice.
    fn owns(&self, path: &Path) -> bool;

    /// Every blob hash directory currently on disk.
    ///
    /// Entries that are not 64 lowercase hex characters are skipped: whatever
    /// they are, they are not blobs this storage wrote.
    ///
    /// # Errors
    ///
    /// - `AppError::FileStorage` if the library root cannot be listed
    async fn list_hashes(&self) -> Result<Vec<String>>;

    /// Remove `{root}/{hash}` unless something still needs it.
    ///
    /// Under the per-hash lock: refuse if an import holds a lease, refuse if
    /// `refs` says the hash is still referenced, otherwise remove the whole
    /// blob directory.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if `hash` is not a library blob hash
    /// - `AppError::FileStorage` if the directory cannot be removed
    /// - whatever `refs` returns
    async fn remove_if_unreferenced(
        &self,
        hash: &str,
        refs: &dyn BlobReferenceCheck,
    ) -> Result<BlobRemoval>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_anything_that_is_not_a_sha256_hex_digest() {
        assert!(is_blob_hash(&"a".repeat(64)));
        assert!(is_blob_hash(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        assert!(
            !is_blob_hash(&"A".repeat(64)),
            "upper case is not the format we write"
        );
        assert!(!is_blob_hash(&"a".repeat(63)));
        assert!(!is_blob_hash(&"a".repeat(65)));
        assert!(!is_blob_hash(""));
        assert!(!is_blob_hash("../../etc/passwd"));
        assert!(!is_blob_hash(&format!("{}/x", "a".repeat(62))));
    }

    #[test]
    fn lease_is_held_until_dropped_and_counts_overlapping_imports() {
        let leases = BlobLeases::new();
        let hash = "b".repeat(64);
        assert!(!leases.is_leased(&hash));

        let first = leases.acquire(&hash);
        let second = leases.acquire(&hash);
        assert!(leases.is_leased(&hash));

        drop(first);
        assert!(leases.is_leased(&hash), "second import still holds it");

        drop(second);
        assert!(!leases.is_leased(&hash));
    }

    #[test]
    fn detached_lease_protects_nothing() {
        let leases = BlobLeases::new();
        let hash = "c".repeat(64);
        let lease = BlobLease::detached(&hash);

        assert_eq!(lease.hash(), hash);
        assert!(!leases.is_leased(&hash));
    }

    #[test]
    fn removal_reports_freed_bytes_only_when_it_removed_something() {
        let removed = BlobRemoval::Removed { bytes_freed: 42 };
        assert!(removed.was_removed());
        assert_eq!(removed.bytes_freed(), 42);

        let retained = BlobRemoval::Retained(RetainReason::Leased);
        assert!(!retained.was_removed());
        assert_eq!(retained.bytes_freed(), 0);
    }
}
