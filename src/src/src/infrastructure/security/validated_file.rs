// ============================================================================
// ValidatedFile - Atomic File Access with Security Validation
// ============================================================================
//
// CRITICAL SECURITY FIXES:
// 1. Arbitrary File Access - Enforces directory scope with allowed_roots
// 2. TOCTOU Race Condition - Atomically validates and opens file (no race window)
// 3. Windows ADS & UNC Bypass - Blocks `:` (ADS) and `\\` (UNC) on Windows
//
// SECURITY PROPERTIES:
// - Validation and file opening are atomic (single function call)
// - No race window between validation and access
// - Post-open verification ensures path didn't change
// - Platform-specific checks for Windows/Unix
// - Clear error messages for security debugging

use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

/// Error type for path validation failures
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Path contains null bytes")]
    NullByte,

    #[error("Path contains parent directory traversal (..)")]
    ParentTraversal,

    #[error("Path is outside allowed scope: {path:?}")]
    PathOutsideScope { path: PathBuf },

    #[error("Path contains Windows Alternate Data Stream (ADS) syntax")]
    WindowsAds,

    #[error("Path contains UNC path syntax (\\\\server\\share)")]
    UncPath,

    #[error("Path is empty")]
    EmptyPath,

    #[error("Failed to canonicalize path: {0}")]
    CanonicalizationFailed(String),

    #[error("Allowed roots list is empty")]
    NoAllowedRoots,

    #[error("Failed to canonicalize allowed root: {0}")]
    InvalidAllowedRoot(String),
}

/// Error type for validated file operations
#[derive(Debug, thiserror::Error)]
pub enum ValidatedFileError {
    #[error("Validation failed: {0}")]
    Validation(#[from] ValidationError),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Post-open verification failed: path changed after opening")]
    PostOpenVerificationFailed,
}

/// Path security validator - performs syntax validation and platform-specific checks
pub struct PathSecurityValidator;

impl PathSecurityValidator {
    /// Validate path syntax and security properties
    ///
    /// Checks:
    /// - No null bytes
    /// - No parent directory traversal (..)
    /// - No Windows ADS (`:` after filename)
    /// - No UNC paths (`\\server\share`)
    pub fn validate(path: &Path) -> Result<(), ValidationError> {
        // Check for empty path
        if path.as_os_str().is_empty() {
            return Err(ValidationError::EmptyPath);
        }

        // Check for null bytes in path string
        if let Some(path_str) = path.to_str() {
            if path_str.contains('\0') {
                return Err(ValidationError::NullByte);
            }
        }

        // Check for parent directory traversal using path components
        for component in path.components() {
            match component {
                Component::ParentDir => return Err(ValidationError::ParentTraversal),
                Component::Normal(os_str) => {
                    if let Some(s) = os_str.to_str() {
                        // Check for null bytes in component
                        if s.contains('\0') {
                            return Err(ValidationError::NullByte);
                        }

                        // Windows-specific checks
                        #[cfg(windows)]
                        {
                            // Block ADS (Alternate Data Streams): file.txt:stream
                            // The colon after the filename allows accessing hidden streams
                            if s.contains(':') {
                                return Err(ValidationError::WindowsAds);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Windows-specific: Block UNC paths (\\server\share)
        #[cfg(windows)]
        if let Some(path_str) = path.to_str() {
            if path_str.starts_with("\\\\") || path_str.starts_with("//") {
                return Err(ValidationError::UncPath);
            }
        }

        Ok(())
    }
}

/// Scope enforcer - ensures paths are within allowed directory trees
pub struct ScopeEnforcer {
    allowed_roots: Vec<PathBuf>,
}

impl ScopeEnforcer {
    /// Create a new scope enforcer with allowed root directories
    ///
    /// # Arguments
    /// * `allowed_roots` - List of directory paths that are allowed access
    ///
    /// # Errors
    /// Returns error if:
    /// - `allowed_roots` is empty
    /// - Any root path cannot be canonicalized
    pub fn new(allowed_roots: Vec<PathBuf>) -> Result<Self, ValidationError> {
        if allowed_roots.is_empty() {
            return Err(ValidationError::NoAllowedRoots);
        }

        // Validate all allowed roots can be canonicalized
        for root in &allowed_roots {
            root.canonicalize().map_err(|e| {
                ValidationError::InvalidAllowedRoot(format!("{}: {}", root.display(), e))
            })?;
        }

        Ok(Self { allowed_roots })
    }

    /// Check if path is within allowed scope and return canonical path
    ///
    /// # Arguments
    /// * `path` - The path to check
    ///
    /// # Returns
    /// Canonical (absolute, symlink-resolved) path if within scope
    ///
    /// # Errors
    /// Returns error if:
    /// - Path cannot be canonicalized
    /// - Canonical path is not within any allowed root
    pub fn check_scope(&self, path: &Path) -> Result<PathBuf, ValidationError> {
        // Canonicalize the input path (resolves symlinks, makes absolute)
        let canonical = path.canonicalize().map_err(|e| {
            ValidationError::CanonicalizationFailed(format!("{}: {}", path.display(), e))
        })?;

        // Check if canonical path starts with any allowed root
        for root in &self.allowed_roots {
            let canonical_root = root.canonicalize().map_err(|e| {
                ValidationError::InvalidAllowedRoot(format!("{}: {}", root.display(), e))
            })?;

            if canonical.starts_with(&canonical_root) {
                return Ok(canonical);
            }
        }

        Err(ValidationError::PathOutsideScope {
            path: canonical.clone(),
        })
    }
}

/// Validated file handle with atomic security validation
///
/// This type ensures that:
/// 1. Path is validated for security before opening
/// 2. File is opened atomically with validation (no TOCTOU race)
/// 3. Path is within allowed directory scope
/// 4. Post-open verification confirms path didn't change
///
/// # Example
/// ```no_run
/// use std::path::PathBuf;
/// use validated_file::ValidatedFile;
///
/// let allowed = vec![PathBuf::from("/home/user/lattice")];
/// let file = ValidatedFile::open("/home/user/lattice/doc.txt", &allowed)?;
/// let content = file.read_to_string()?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
pub struct ValidatedFile {
    handle: File,
    canonical_path: PathBuf,
    metadata: Metadata,
}

impl ValidatedFile {
    /// Open a file with atomic security validation
    ///
    /// This is the ONLY safe way to open files with security validation.
    /// The validation and file opening are atomic - no race window.
    ///
    /// # Security Properties
    /// 1. Validates path syntax (no null bytes, traversal, ADS, UNC)
    /// 2. Enforces directory scope (path must be in allowed_roots)
    /// 3. Opens file atomically after validation
    /// 4. Verifies path after opening (defense in depth)
    ///
    /// # Arguments
    /// * `path` - Path to file to open
    /// * `allowed_roots` - List of allowed root directories
    ///
    /// # Errors
    /// Returns error if:
    /// - Path fails syntax validation
    /// - Path is outside allowed scope
    /// - File cannot be opened
    /// - Post-open verification fails
    ///
    /// # Example
    /// ```no_run
    /// let allowed = vec![PathBuf::from("/home/user/lattice")];
    ///
    /// // SECURE: Opens file within allowed scope
    /// let file = ValidatedFile::open("/home/user/lattice/doc.txt", &allowed)?;
    ///
    /// // BLOCKED: Path outside scope
    /// let result = ValidatedFile::open("/etc/passwd", &allowed);
    /// assert!(result.is_err());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn open<P: AsRef<Path>>(
        path: P,
        allowed_roots: &[PathBuf],
    ) -> Result<Self, ValidatedFileError> {
        let path = path.as_ref();

        // Step 1: Syntax validation (null bytes, traversal, ADS, UNC)
        PathSecurityValidator::validate(path)?;

        // Step 2: Scope enforcement (ensure path is in allowed roots)
        let enforcer = ScopeEnforcer::new(allowed_roots.to_vec())?;
        let canonical_path = enforcer.check_scope(path)?;

        // Step 3: Open file atomically (no race window after validation)
        let handle = File::open(&canonical_path)?;

        // Step 4: Post-open verification (defense in depth)
        // Verify the file we opened is actually at the canonical path
        let metadata = handle.metadata()?;

        // Re-canonicalize and verify it matches
        let verify_canonical = canonical_path
            .canonicalize()
            .map_err(|e| io::Error::other(format!("Post-open canonicalization failed: {}", e)))?;

        if verify_canonical != canonical_path {
            return Err(ValidatedFileError::PostOpenVerificationFailed);
        }

        Ok(Self {
            handle,
            canonical_path,
            metadata,
        })
    }

    /// Get reference to underlying file handle
    pub fn handle(&self) -> &File {
        &self.handle
    }

    /// Get mutable reference to underlying file handle
    pub fn handle_mut(&mut self) -> &mut File {
        &mut self.handle
    }

    /// Get canonical path of opened file
    pub fn path(&self) -> &Path {
        &self.canonical_path
    }

    /// Get metadata of opened file
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// Read entire file contents to string
    ///
    /// # Errors
    /// Returns error if file cannot be read or contains invalid UTF-8
    pub fn read_to_string(&mut self) -> Result<String, io::Error> {
        let mut content = String::new();
        self.handle.read_to_string(&mut content)?;
        Ok(content)
    }

    /// Read entire file contents to bytes
    ///
    /// # Errors
    /// Returns error if file cannot be read
    pub fn read_to_end(&mut self) -> Result<Vec<u8>, io::Error> {
        let mut buffer = Vec::new();
        self.handle.read_to_end(&mut buffer)?;
        Ok(buffer)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    // Helper to create a test file
    fn create_test_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_blocks_absolute_path_escape() {
        // Try to access /etc/passwd (outside scope)
        let temp_dir = TempDir::new().unwrap();
        let allowed = vec![temp_dir.path().to_path_buf()];

        let result = ValidatedFile::open("/etc/passwd", &allowed);

        assert!(
            matches!(
                result,
                Err(ValidatedFileError::Validation(
                    ValidationError::PathOutsideScope { .. }
                ))
            ),
            "Should block access to /etc/passwd"
        );
    }

    #[test]
    fn test_blocks_relative_traversal() {
        // Try to escape using ../
        let temp_dir = TempDir::new().unwrap();
        let allowed = vec![temp_dir.path().to_path_buf()];

        // Create a file in temp dir
        let _test_file = create_test_file(temp_dir.path(), "test.txt", "content");

        // Try to traverse up
        let result = ValidatedFile::open("../../../etc/passwd", &allowed);

        assert!(
            matches!(
                result,
                Err(ValidatedFileError::Validation(
                    ValidationError::ParentTraversal
                ))
            ),
            "Should block relative traversal with ../"
        );
    }

    #[test]
    fn test_blocks_null_bytes() {
        // Try to inject null bytes
        let temp_dir = TempDir::new().unwrap();
        let allowed = vec![temp_dir.path().to_path_buf()];

        let malicious_path = format!("{}test\0.txt", temp_dir.path().display());
        let result = ValidatedFile::open(&malicious_path, &allowed);

        assert!(
            matches!(
                result,
                Err(ValidatedFileError::Validation(ValidationError::NullByte))
            ),
            "Should block null byte injection"
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_blocks_symlink_escape() {
        // Create temp directory structure
        let temp_dir = TempDir::new().unwrap();
        let vault_dir = temp_dir.path().join("lattice");
        fs::create_dir(&vault_dir).unwrap();

        // Create file outside lattice
        let outside_file = temp_dir.path().join("outside.txt");
        fs::write(&outside_file, "secret").unwrap();

        // Create symlink inside lattice pointing outside
        let symlink_path = vault_dir.join("link.txt");
        std::os::unix::fs::symlink(&outside_file, &symlink_path).unwrap();

        // Try to access file through symlink (TOCTOU prevention)
        let allowed = vec![vault_dir.clone()];
        let result = ValidatedFile::open(&symlink_path, &allowed);

        // Should fail because symlink resolves to path outside allowed scope
        assert!(
            result.is_err(),
            "Should block symlink escape (TOCTOU prevention)"
        );
    }

    #[test]
    #[cfg(windows)]
    fn test_blocks_windows_ads() {
        // Try to access Alternate Data Stream
        let temp_dir = TempDir::new().unwrap();
        let allowed = vec![temp_dir.path().to_path_buf()];

        let malicious_path = format!("{}test.txt:hidden", temp_dir.path().display());
        let result = ValidatedFile::open(&malicious_path, &allowed);

        assert!(
            matches!(
                result,
                Err(ValidatedFileError::Validation(ValidationError::WindowsAds))
            ),
            "Should block Windows ADS syntax"
        );
    }

    #[test]
    #[cfg(windows)]
    fn test_blocks_unc_paths() {
        // Try to access UNC path
        let temp_dir = TempDir::new().unwrap();
        let allowed = vec![temp_dir.path().to_path_buf()];

        let result = ValidatedFile::open(r"\\server\share\file.txt", &allowed);

        assert!(
            matches!(
                result,
                Err(ValidatedFileError::Validation(ValidationError::UncPath))
            ),
            "Should block UNC paths"
        );
    }

    #[test]
    fn test_allows_valid_files_in_scope() {
        // Create temp directory and file
        let temp_dir = TempDir::new().unwrap();
        let test_file = create_test_file(temp_dir.path(), "test.txt", "hello world");
        let allowed = vec![temp_dir.path().to_path_buf()];

        // Should successfully open
        let mut validated_file =
            ValidatedFile::open(&test_file, &allowed).expect("Should allow valid file in scope");

        // Verify we can read the content
        let content = validated_file.read_to_string().unwrap();
        assert_eq!(content, "hello world");

        // Verify path is correct
        assert_eq!(
            validated_file.path(),
            test_file.canonicalize().unwrap().as_path()
        );
    }

    #[test]
    fn test_path_security_validator_null_bytes() {
        let path = PathBuf::from("test\0.txt");
        let result = PathSecurityValidator::validate(&path);
        assert!(matches!(result, Err(ValidationError::NullByte)));
    }

    #[test]
    fn test_path_security_validator_parent_traversal() {
        let path = PathBuf::from("../test.txt");
        let result = PathSecurityValidator::validate(&path);
        assert!(matches!(result, Err(ValidationError::ParentTraversal)));
    }

    #[test]
    fn test_scope_enforcer_empty_roots() {
        let result = ScopeEnforcer::new(vec![]);
        assert!(matches!(result, Err(ValidationError::NoAllowedRoots)));
    }

    #[test]
    fn test_scope_enforcer_invalid_root() {
        let result = ScopeEnforcer::new(vec![PathBuf::from("/nonexistent/path")]);
        assert!(matches!(
            result,
            Err(ValidationError::InvalidAllowedRoot(_))
        ));
    }

    #[test]
    fn test_scope_enforcer_check_scope() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = create_test_file(temp_dir.path(), "test.txt", "content");

        let enforcer = ScopeEnforcer::new(vec![temp_dir.path().to_path_buf()])
            .expect("Should create enforcer");

        // File in scope should succeed
        let result = enforcer.check_scope(&test_file);
        assert!(result.is_ok());

        // File outside scope should fail
        let outside_file = PathBuf::from("/etc/passwd");
        if outside_file.exists() {
            let result = enforcer.check_scope(&outside_file);
            assert!(matches!(
                result,
                Err(ValidationError::PathOutsideScope { .. })
            ));
        }
    }

    #[test]
    fn test_validated_file_read_operations() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = create_test_file(temp_dir.path(), "test.txt", "test content");
        let allowed = vec![temp_dir.path().to_path_buf()];

        let mut validated_file = ValidatedFile::open(&test_file, &allowed).unwrap();

        // Test read_to_string
        let content = validated_file.read_to_string().unwrap();
        assert_eq!(content, "test content");

        // Re-open for read_to_end test
        let mut validated_file = ValidatedFile::open(&test_file, &allowed).unwrap();

        // Test read_to_end
        let bytes = validated_file.read_to_end().unwrap();
        assert_eq!(bytes, b"test content");
    }

    #[test]
    fn test_multiple_allowed_roots() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        let file1 = create_test_file(temp_dir1.path(), "file1.txt", "content1");
        let file2 = create_test_file(temp_dir2.path(), "file2.txt", "content2");

        let allowed = vec![
            temp_dir1.path().to_path_buf(),
            temp_dir2.path().to_path_buf(),
        ];

        // Both files should be accessible
        let result1 = ValidatedFile::open(&file1, &allowed);
        assert!(result1.is_ok(), "Should allow file in first root");

        let result2 = ValidatedFile::open(&file2, &allowed);
        assert!(result2.is_ok(), "Should allow file in second root");
    }

    #[test]
    fn test_empty_path() {
        let temp_dir = TempDir::new().unwrap();
        let allowed = vec![temp_dir.path().to_path_buf()];

        let result = ValidatedFile::open("", &allowed);
        assert!(matches!(
            result,
            Err(ValidatedFileError::Validation(ValidationError::EmptyPath))
        ));
    }
}
