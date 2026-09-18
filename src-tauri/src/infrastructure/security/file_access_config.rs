use super::validated_file::{ValidatedFile, ValidatedFileError};
use crate::shared::error::AppError;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Configuration for file access validation
///
/// Manages allowed root directories for secure file access.
/// All file operations on user-supplied paths must be validated
/// against these allowed roots.
pub struct FileAccessConfig {
    /// Allowed root directories (indexed folders, lattice directory)
    allowed_roots: Arc<RwLock<HashSet<PathBuf>>>,
}

impl FileAccessConfig {
    /// Create configuration with allowed root directories
    ///
    /// # Arguments
    /// * `allowed_roots` - Vector of directory paths that are allowed for file access
    ///
    /// # Example
    /// ```
    /// use std::path::PathBuf;
    /// use crate::infrastructure::security::FileAccessConfig;
    ///
    /// let config = FileAccessConfig::new(vec![
    ///     PathBuf::from("/home/user/lattice"),
    ///     PathBuf::from("/home/user/documents"),
    /// ]);
    /// ```
    pub fn new(allowed_roots: Vec<PathBuf>) -> Self {
        // CRITICAL FIX: Handle both real paths (canonicalize) and test paths (keep as-is)
        // - Real paths that exist: canonicalize to resolve symlinks (e.g., /var -> /private/var)
        // - Test paths that don't exist: keep normalized form for testing
        // This allows both production use and comprehensive testing
        let canonical_roots = allowed_roots
            .into_iter()
            .map(|p| {
                if let Ok(canonical) = p.canonicalize() {
                    // Path exists - use canonical form
                    canonical
                } else {
                    // Path doesn't exist (test path) - use as-is
                    // This allows tests to use fake paths like /lattice
                    p
                }
            })
            .collect();

        Self {
            allowed_roots: Arc::new(RwLock::new(canonical_roots)),
        }
    }

    /// Validate path and open file (convenience method)
    ///
    /// This is the primary method for secure file access. It:
    /// 1. Validates path syntax (no traversal, null bytes, etc.)
    /// 2. Checks path is within allowed roots
    /// 3. Opens file atomically
    /// 4. Returns validated file handle
    ///
    /// # Arguments
    /// * `path` - Path to validate and open
    ///
    /// # Returns
    /// * `Ok(ValidatedFile)` - Validated file handle
    /// * `Err(AppError::Security)` - Path validation failed
    /// * `Err(AppError::FileNotFound)` - File doesn't exist
    ///
    /// # Example
    /// ```
    /// use std::path::PathBuf;
    /// use crate::infrastructure::security::FileAccessConfig;
    ///
    /// let config = FileAccessConfig::new(vec![PathBuf::from("/home/user/lattice")]);
    /// let file = config.open_file("/home/user/lattice/doc.txt")?;
    /// let content = file.read_to_string()?;
    /// # Ok::<(), crate::shared::error::AppError>(())
    /// ```
    pub fn open_file<P: AsRef<Path>>(&self, path: P) -> Result<ValidatedFile, AppError> {
        let display_path = path.as_ref().display().to_string();
        // This probe only preserves the public NotFound error contract. A
        // positive result grants no access: ValidatedFile still performs the
        // authoritative scope check and open below on its own handle.
        match path.as_ref().try_exists() {
            Ok(false) => {
                return Err(AppError::NotFound(format!(
                    "File not found: {display_path}"
                )))
            }
            Ok(true) => {}
            Err(error) => {
                return Err(AppError::FileRead {
                    path: display_path,
                    reason: error.to_string(),
                });
            }
        }
        let roots = self
            .allowed_roots
            .read()
            .map_err(|e| AppError::InternalError(format!("Lock error: {}", e)))?;

        let roots_vec: Vec<PathBuf> = roots.iter().cloned().collect();

        ValidatedFile::open(path, &roots_vec).map_err(|error| match error {
            ValidatedFileError::Io(io_error) if io_error.kind() == std::io::ErrorKind::NotFound => {
                AppError::NotFound(format!("File not found: {display_path}"))
            }
            ValidatedFileError::Io(io_error) => AppError::FileRead {
                path: display_path,
                reason: io_error.to_string(),
            },
            other => AppError::Security(other.to_string()),
        })
    }

    /// Validate path against allowed roots (scope validation only).
    ///
    /// Ensures the path is within one of the allowed roots (prevents directory traversal).
    /// **Does NOT require the path to exist** (separates scope validation from existence checking).
    ///
    /// # Architectural Intent
    ///
    /// FileAccessConfig is the **Policy Layer** (what's allowed), not the **Enforcement Layer** (what exists).
    /// - This method checks: "Is this path within allowed scope?"
    /// - File storage layer checks: "Does this path exist?"
    ///
    /// Use this when you need to validate a path but not open the file yet.
    /// For example, checking if a path is allowed before passing to external tools.
    ///
    /// # Arguments
    /// * `path` - Path to validate
    ///
    /// # Returns
    /// * `Ok(PathBuf)` - Normalized absolute path if within allowed roots
    /// * `Err(AppError::Security)` - Path not in allowed roots or path traversal attempt
    /// * `Err(AppError::InvalidInput)` - Path is invalid
    pub fn validate_path<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, AppError> {
        use std::path::Component;

        let path = path.as_ref();

        // STRATEGY: Try to canonicalize (resolves symlinks, matches allowed_roots form).
        // If that fails (path doesn't exist), fall back to path normalization.
        // This allows validation of non-existent paths while maintaining symlink consistency.

        let resolved_path = if let Ok(canonical) = path.canonicalize() {
            // Path exists - use canonical form (matches allowed_roots which are canonical)
            canonical
        } else {
            // Path doesn't exist - CRITICAL FIX: canonicalize parent directory,
            // then append filename to maintain symlink consistency with allowed_roots.
            // This prevents false denials on macOS where /var -> /private/var.

            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()
                    .map_err(|e| {
                        AppError::InvalidInput(format!("Cannot resolve relative path: {}", e))
                    })?
                    .join(path)
            };

            // Normalize path components (resolve .. and . without I/O)
            let mut normalized_components = Vec::new();
            for component in absolute.components() {
                match component {
                    Component::ParentDir => {
                        // Go up one directory level
                        normalized_components.pop();
                    }
                    Component::CurDir => {
                        // Skip "." (current directory)
                    }
                    _ => {
                        // Keep normal components (RootDir, Prefix, Normal)
                        normalized_components.push(component);
                    }
                }
            }
            let normalized: PathBuf = normalized_components.iter().collect();

            // CRITICAL FIX: Canonicalize parent directory if it exists, then append filename
            // This ensures symlink consistency with allowed_roots
            if let Some(parent) = normalized.parent() {
                if let Ok(canonical_parent) = parent.canonicalize() {
                    // Parent exists and can be canonicalized
                    if let Some(filename) = normalized.file_name() {
                        canonical_parent.join(filename)
                    } else {
                        // No filename (path is a directory), use normalized
                        normalized
                    }
                } else {
                    // Parent doesn't exist either, use normalized
                    normalized
                }
            } else {
                // No parent (root path?), use normalized
                normalized
            }
        };

        if self.is_allowed(&resolved_path) {
            Ok(resolved_path)
        } else {
            Err(AppError::Security(
                "File access denied: path not in allowed directories".to_string(),
            ))
        }
    }

    /// Check if path is within allowed roots
    ///
    /// # Arguments
    /// * `path` - Canonical path to check
    ///
    /// # Returns
    /// * `true` if path is within any allowed root
    /// * `false` otherwise
    fn is_allowed(&self, path: &Path) -> bool {
        let roots = self.allowed_roots.read().ok();

        if let Some(roots) = roots {
            roots.iter().any(|root| path.starts_with(root))
        } else {
            false
        }
    }

    /// Add new allowed root (for dynamic directory indexing)
    ///
    /// Call this when user indexes a new directory to add it to allowed roots.
    ///
    /// # Arguments
    /// * `root` - Directory path to add to allowed roots
    ///
    /// # Example
    /// ```
    /// use std::path::PathBuf;
    /// use crate::infrastructure::security::FileAccessConfig;
    ///
    /// let config = FileAccessConfig::new(vec![]);
    ///
    /// // User indexes a new directory
    /// config.add_allowed_root(PathBuf::from("/home/user/new_vault"))?;
    ///
    /// // Now files in that directory can be accessed
    /// let file = config.open_file("/home/user/new_vault/doc.txt")?;
    /// # Ok::<(), crate::shared::error::AppError>(())
    /// ```
    pub fn add_allowed_root(&self, root: PathBuf) -> Result<(), AppError> {
        let mut roots = self
            .allowed_roots
            .write()
            .map_err(|e| AppError::InternalError(format!("Lock error: {}", e)))?;

        // Canonicalize root before adding
        let canonical = root.canonicalize().map_err(|e| AppError::FileNotFound {
            path: format!("Invalid root: {}", e),
        })?;

        roots.insert(canonical);
        Ok(())
    }

    /// Replace the entire allowed-roots set atomically.
    ///
    /// Use when recomputing policy from settings — adding the new roots and
    /// removing the stale ones separately would leave a window in which a
    /// just-unindexed directory is still readable, and would let roots from a
    /// previous vault accumulate for the life of the process.
    ///
    /// Roots that cannot be canonicalized (not yet created, unmounted volume,
    /// cloud placeholder) are skipped with a warning rather than failing the
    /// whole update — a single bad entry in settings must not leave the app
    /// with no readable roots at all.
    pub fn set_allowed_roots(&self, roots: Vec<PathBuf>) -> Result<(), AppError> {
        let mut resolved = std::collections::HashSet::new();

        for root in roots {
            match root.canonicalize() {
                Ok(canonical) => {
                    resolved.insert(canonical);
                }
                Err(e) => {
                    tracing::warn!(
                        root = %root.display(),
                        error = %e,
                        "skipping unreadable allowed root"
                    );
                }
            }
        }

        let mut guard = self
            .allowed_roots
            .write()
            .map_err(|e| AppError::InternalError(format!("Lock error: {}", e)))?;
        *guard = resolved;
        Ok(())
    }

    /// Remove allowed root
    ///
    /// Call this when user removes an indexed directory.
    ///
    /// # Arguments
    /// * `root` - Directory path to remove from allowed roots
    pub fn remove_allowed_root(&self, root: &Path) -> Result<(), AppError> {
        let mut roots = self
            .allowed_roots
            .write()
            .map_err(|e| AppError::InternalError(format!("Lock error: {}", e)))?;

        let canonical = root.canonicalize().map_err(|e| AppError::FileNotFound {
            path: format!("Invalid root: {}", e),
        })?;

        roots.remove(&canonical);
        Ok(())
    }

    /// Get list of allowed roots (for debugging/UI display)
    pub fn get_allowed_roots(&self) -> Result<Vec<PathBuf>, AppError> {
        let roots = self
            .allowed_roots
            .read()
            .map_err(|e| AppError::InternalError(format!("Lock error: {}", e)))?;

        Ok(roots.iter().cloned().collect())
    }

    /// Clone configuration (creates new Arc reference)
    pub fn clone_config(&self) -> Self {
        Self {
            allowed_roots: Arc::clone(&self.allowed_roots),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_create_config() {
        let temp = TempDir::new().unwrap();
        let config = FileAccessConfig::new(vec![temp.path().to_path_buf()]);

        let roots = config.get_allowed_roots().unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn test_open_file_in_allowed_root() {
        let temp = TempDir::new().unwrap();
        let test_file = temp.path().join("test.txt");
        fs::write(&test_file, "content").unwrap();

        let config = FileAccessConfig::new(vec![temp.path().to_path_buf()]);
        let mut validated = config.open_file(&test_file).unwrap();

        let content = validated.read_to_string().unwrap();
        assert_eq!(content, "content");
    }

    #[test]
    fn test_open_file_outside_allowed_root() {
        let temp = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let config = FileAccessConfig::new(vec![temp.path().to_path_buf()]);

        // The property under test: a file that exists but sits outside every
        // allowed root is refused as a *security* denial, not merely reported
        // as missing. `open_file` probes existence first, so the hostile path
        // has to really exist or the assertion silently degrades into
        // AppError::NotFound — which is exactly what `/etc/passwd` did here on
        // Windows, where it resolves to a non-existent `C:\etc\passwd`. A
        // second temp directory exists on every platform and is outside the
        // allowed root.
        let outside_file = outside.path().join("secret.txt");
        fs::write(&outside_file, "secret").unwrap();

        let result = config.open_file(&outside_file);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::Security(_)));
    }

    #[test]
    fn test_validate_path() {
        let temp = TempDir::new().unwrap();
        let test_file = temp.path().join("test.txt");
        fs::write(&test_file, "content").unwrap();

        // Need to canonicalize temp path and add it to config
        let canonical_temp = temp.path().canonicalize().unwrap();
        let config = FileAccessConfig::new(vec![canonical_temp.clone()]);

        let validated_path = config.validate_path(&test_file).unwrap();

        assert!(validated_path.is_absolute());
        assert!(validated_path.starts_with(&canonical_temp));
    }

    #[test]
    fn test_add_allowed_root() {
        let temp1 = TempDir::new().unwrap();
        let temp2 = TempDir::new().unwrap();

        let config = FileAccessConfig::new(vec![temp1.path().to_path_buf()]);

        // Initially only temp1 allowed
        let roots = config.get_allowed_roots().unwrap();
        assert_eq!(roots.len(), 1);

        config.add_allowed_root(temp2.path().to_path_buf()).unwrap();

        let roots = config.get_allowed_roots().unwrap();
        assert_eq!(roots.len(), 2);

        // Can now access files in temp2
        let test_file = temp2.path().join("test.txt");
        fs::write(&test_file, "content").unwrap();

        let result = config.open_file(&test_file);
        assert!(result.is_ok());
    }

    #[test]
    fn test_remove_allowed_root() {
        let temp = TempDir::new().unwrap();
        let test_file = temp.path().join("test.txt");
        fs::write(&test_file, "content").unwrap();

        // Canonicalize temp path
        let canonical_temp = temp.path().canonicalize().unwrap();
        let config = FileAccessConfig::new(vec![canonical_temp.clone()]);

        // Initially can access
        assert!(config.open_file(&test_file).is_ok());

        config.remove_allowed_root(&canonical_temp).unwrap();

        // Now cannot access
        let result = config.open_file(&test_file);
        assert!(result.is_err());
    }

    #[test]
    fn test_relative_path_in_constructor() {
        // Non-canonical constructor paths are canonicalized.
        // This tests the fix for the canonicalization mismatch bug
        let temp = TempDir::new().unwrap();
        let test_file = temp.path().join("test.txt");
        fs::write(&test_file, "content").unwrap();

        // CRITICAL: Pass NON-CANONICAL path to constructor (temp.path() may contain symlinks)
        // The fix should canonicalize this internally
        let config = FileAccessConfig::new(vec![temp.path().to_path_buf()]);

        let result = config.open_file(&test_file);
        assert!(
            result.is_ok(),
            "Should be able to access file when constructor receives non-canonical root"
        );
    }
}
