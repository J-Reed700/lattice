//! Content-addressed file storage for the library.
//!
//! This module implements content-addressed storage that:
//! - Copies files to `~/.lattice/files/{sha256_hash}/original_filename.ext`
//! - Computes SHA256 hash of file content for deduplication
//! - Detects duplicates by hash (not path)
//! - Preserves original filename for UX
//! - Returns (library_path, hash) for indexing
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
//! # Example
//!
//! ```rust,no_run
//! use lattice::infrastructure::storage::ContentAddressedStorage;
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let storage = ContentAddressedStorage::new()?;
//!
//!     // Import file to library
//!     let (library_path, hash) = storage.import_file(
//!         Path::new("/downloads/document.pdf")
//!     ).await?;
//!
//!     println!("Imported to: {}", library_path.display());
//!     println!("Hash: {}", hash);
//!
//!     // Check if another file is already in library
//!     if storage.exists_by_hash(&hash).await? {
//!         println!("This file is already in the library!");
//!     }
//!
//!     Ok(())
//! }
//! ```

use crate::application::ports::ContentAddressedStoragePort as ContentAddressedStoragePortTrait;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Content-addressed storage port (re-exported from application ports).
///
/// Defines the interface for importing files to content-addressed storage
/// and checking for duplicates by hash.
#[async_trait]
pub trait ContentAddressedStoragePort: Send + Sync {
    /// Import file to content-addressed storage.
    ///
    /// # Arguments
    ///
    /// * `source_path` - Path to source file to import
    ///
    /// # Returns
    ///
    /// Tuple of (library_path, sha256_hash)
    ///
    /// # Errors
    ///
    /// - `AppError::FileNotFound` if source file doesn't exist
    /// - `AppError::FileRead` if cannot read source file
    /// - `AppError::FileStorage` if cannot write to library
    ///
    /// # Behavior
    ///
    /// - If file with same hash exists, returns existing library path (no copy)
    /// - If file is new, copies to `{library_root}/{hash}/original_filename`
    async fn import_file(&self, source_path: &Path) -> Result<(PathBuf, String)>;

    /// Check if file exists in library by hash.
    ///
    /// # Arguments
    ///
    /// * `hash` - SHA256 hash of file content
    ///
    /// # Returns
    ///
    /// `true` if a file with this hash exists in library
    async fn exists_by_hash(&self, hash: &str) -> Result<bool>;

    /// Get library path for a given hash.
    ///
    /// # Arguments
    ///
    /// * `hash` - SHA256 hash of file content
    ///
    /// # Returns
    ///
    /// `Some(PathBuf)` if file exists in library, `None` otherwise
    async fn get_path_by_hash(&self, hash: &str) -> Result<Option<PathBuf>>;
}

/// Content-addressed file storage implementation.
///
/// Stores files in `~/.lattice/files/{hash}/filename` layout.
///
/// # Thread Safety
///
/// All operations are async and use Tokio's thread-safe file I/O.
///
/// # Deduplication
///
/// Files with identical content (same SHA256) are stored only once.
/// Subsequent imports return the existing library path.
pub struct ContentAddressedStorage {
    /// Root directory for library storage (~/.lattice/files/)
    library_root: PathBuf,
}

impl ContentAddressedStorage {
    /// Create a new content-addressed storage instance.
    ///
    /// Uses default library root: `~/.lattice/files/`
    ///
    /// # Returns
    ///
    /// New storage instance
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidState` if home directory cannot be determined
    ///
    /// # Example
    ///
    /// ```rust
    /// let storage = ContentAddressedStorage::new()?;
    /// ```
    pub fn new() -> Result<Self> {
        let library_root = Self::default_library_root()?;
        Ok(Self { library_root })
    }

    /// Create storage with custom library root.
    ///
    /// # Arguments
    ///
    /// * `library_root` - Custom library root directory
    ///
    /// # Example
    ///
    /// ```rust
    /// let storage = ContentAddressedStorage::with_root(PathBuf::from("/custom/library"));
    /// ```
    pub fn with_root(library_root: PathBuf) -> Self {
        Self { library_root }
    }

    /// Get default library root directory.
    ///
    /// Returns `~/.lattice/files/`
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidState` if home directory cannot be determined
    fn default_library_root() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::InvalidState("Cannot determine home directory".to_string()))?;

        Ok(home.join(".lattice").join("files"))
    }

    /// Compute SHA256 hash of file content.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to file to hash
    ///
    /// # Returns
    ///
    /// Hex-encoded SHA256 hash string
    ///
    /// # Errors
    ///
    /// - `AppError::FileNotFound` if file doesn't exist
    /// - `AppError::FileRead` if cannot read file
    async fn compute_file_hash(&self, path: &Path) -> Result<String> {
        // Read file content
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

        // Compute SHA-256 hash
        let mut hasher = Sha256::new();
        hasher.update(&content);
        let hash = hasher.finalize();

        // Convert to hex string
        Ok(format!("{:x}", hash))
    }

    /// Get directory path for a hash.
    ///
    /// Returns `{library_root}/{hash}/`
    fn hash_directory(&self, hash: &str) -> PathBuf {
        self.library_root.join(hash)
    }

    /// Extract filename from path.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to extract filename from
    ///
    /// # Returns
    ///
    /// Filename as string
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
}

#[async_trait]
impl ContentAddressedStoragePortTrait for ContentAddressedStorage {
    async fn import_file(&self, source_path: &Path) -> Result<(PathBuf, String)> {
        // 1. Compute SHA256 hash of file content
        let hash = self.compute_file_hash(source_path).await?;

        // 2. Get hash-based directory
        let hash_dir = self.hash_directory(&hash);

        // 3. Check if hash directory already exists
        if hash_dir.exists() {
            // File already in library - find existing file
            let filename = Self::extract_filename(source_path)?;
            let library_path = hash_dir.join(&filename);

            if library_path.exists() {
                // Exact file exists, return it
                tracing::info!(
                    hash = %hash,
                    path = %library_path.display(),
                    "File already in library (duplicate detected by hash)"
                );
                return Ok((library_path, hash));
            }

            // Hash directory exists but different filename - still a duplicate
            // Find first file in directory
            let mut entries = fs::read_dir(&hash_dir).await.map_err(|e| {
                AppError::FileStorage(format!("Failed to read hash directory: {}", e))
            })?;

            if let Some(entry) = entries.next_entry().await.map_err(|e| {
                AppError::FileStorage(format!("Failed to read directory entry: {}", e))
            })? {
                let existing_path = entry.path();
                tracing::info!(
                    hash = %hash,
                    existing = %existing_path.display(),
                    source = %source_path.display(),
                    "Duplicate content detected (different filename)"
                );
                return Ok((existing_path, hash));
            }

            // Directory exists but is empty - fall through to copy
        }

        // 4. File is new - create hash directory and copy file
        fs::create_dir_all(&hash_dir).await.map_err(|e| {
            AppError::FileStorage(format!("Failed to create hash directory: {}", e))
        })?;

        // 5. Copy file to library with original filename
        let filename = Self::extract_filename(source_path)?;
        let library_path = hash_dir.join(&filename);

        fs::copy(source_path, &library_path)
            .await
            .map_err(|e| AppError::FileStorage(format!("Failed to copy file to library: {}", e)))?;

        tracing::info!(
            hash = %hash,
            source = %source_path.display(),
            library = %library_path.display(),
            "File imported to library"
        );

        // 6. Return library path and hash
        Ok((library_path, hash))
    }

    async fn exists_by_hash(&self, hash: &str) -> Result<bool> {
        let hash_dir = self.hash_directory(hash);
        Ok(hash_dir.exists())
    }

    async fn get_path_by_hash(&self, hash: &str) -> Result<Option<PathBuf>> {
        let hash_dir = self.hash_directory(hash);

        if !hash_dir.exists() {
            return Ok(None);
        }

        // Find first file in hash directory
        let mut entries = fs::read_dir(&hash_dir)
            .await
            .map_err(|e| AppError::FileStorage(format!("Failed to read hash directory: {}", e)))?;

        if let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AppError::FileStorage(format!("Failed to read directory entry: {}", e)))?
        {
            Ok(Some(entry.path()))
        } else {
            // Directory exists but is empty
            Ok(None)
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_import_file_new() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let source_file = temp_dir.path().join("source.txt");

        // Create source file
        fs::write(&source_file, "Test content").await.unwrap();

        // Create storage
        let storage = ContentAddressedStorage::with_root(library_root.clone());

        // Import file
        let (library_path, hash) = storage.import_file(&source_file).await.unwrap();

        // Verify library path structure
        assert!(library_path.starts_with(&library_root));
        assert!(
            library_path
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                == hash
        );
        assert_eq!(library_path.file_name().unwrap(), "source.txt");

        // Verify file was copied
        assert!(library_path.exists());
        let content = fs::read_to_string(&library_path).await.unwrap();
        assert_eq!(content, "Test content");

        // Verify hash is hex string
        assert_eq!(hash.len(), 64); // SHA-256 = 64 hex chars
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn test_import_file_duplicate() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let source_file1 = temp_dir.path().join("file1.txt");
        let source_file2 = temp_dir.path().join("file2.txt");

        // Create two files with identical content
        fs::write(&source_file1, "Identical content").await.unwrap();
        fs::write(&source_file2, "Identical content").await.unwrap();

        let storage = ContentAddressedStorage::with_root(library_root.clone());

        // Import first file
        let (path1, hash1) = storage.import_file(&source_file1).await.unwrap();

        // Import second file (duplicate)
        let (path2, hash2) = storage.import_file(&source_file2).await.unwrap();

        // Should have same hash
        assert_eq!(hash1, hash2);

        // Should return same library path (or path in same hash directory)
        assert_eq!(path1.parent(), path2.parent());
    }

    #[tokio::test]
    async fn test_import_file_different_content() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let source_file1 = temp_dir.path().join("file1.txt");
        let source_file2 = temp_dir.path().join("file2.txt");

        // Create two files with different content
        fs::write(&source_file1, "Content 1").await.unwrap();
        fs::write(&source_file2, "Content 2").await.unwrap();

        let storage = ContentAddressedStorage::with_root(library_root);

        // Import both files
        let (path1, hash1) = storage.import_file(&source_file1).await.unwrap();
        let (path2, hash2) = storage.import_file(&source_file2).await.unwrap();

        // Should have different hashes
        assert_ne!(hash1, hash2);

        // Should have different library paths
        assert_ne!(path1, path2);

        // Both files should exist
        assert!(path1.exists());
        assert!(path2.exists());
    }

    #[tokio::test]
    async fn test_exists_by_hash() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let source_file = temp_dir.path().join("file.txt");

        fs::write(&source_file, "Test content").await.unwrap();

        let storage = ContentAddressedStorage::with_root(library_root);

        // Import file
        let (_path, hash) = storage.import_file(&source_file).await.unwrap();

        // Check existence
        assert!(storage.exists_by_hash(&hash).await.unwrap());
        assert!(!storage.exists_by_hash("nonexistent_hash").await.unwrap());
    }

    #[tokio::test]
    async fn test_get_path_by_hash() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let source_file = temp_dir.path().join("file.txt");

        fs::write(&source_file, "Test content").await.unwrap();

        let storage = ContentAddressedStorage::with_root(library_root);

        // Import file
        let (original_path, hash) = storage.import_file(&source_file).await.unwrap();

        // Retrieve path by hash
        let retrieved_path = storage.get_path_by_hash(&hash).await.unwrap();

        assert_eq!(retrieved_path, Some(original_path));
    }

    #[tokio::test]
    async fn test_get_path_by_hash_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");

        let storage = ContentAddressedStorage::with_root(library_root);

        let result = storage.get_path_by_hash("nonexistent").await.unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_filename() {
        let path = Path::new("/path/to/file.txt");
        let filename = ContentAddressedStorage::extract_filename(path).unwrap();
        assert_eq!(filename, "file.txt");

        let path = Path::new("file.txt");
        let filename = ContentAddressedStorage::extract_filename(path).unwrap();
        assert_eq!(filename, "file.txt");
    }

    #[tokio::test]
    async fn test_import_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let library_root = temp_dir.path().join("library");
        let nonexistent = temp_dir.path().join("nonexistent.txt");

        let storage = ContentAddressedStorage::with_root(library_root);

        let result = storage.import_file(&nonexistent).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(AppError::FileNotFound { .. })));
    }
}
