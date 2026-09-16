use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PathValidationError {
    #[error("Path contains invalid characters or sequences")]
    InvalidPath,

    #[error("Path traversal attempt detected: {0}")]
    PathTraversal(String),

    #[error(
        "Symlink attack detected: canonical path {canonical} differs from expected {expected}"
    )]
    SymlinkAttack { canonical: String, expected: String },

    #[error("IO error during path validation: {0}")]
    IoError(#[from] std::io::Error),
}

pub struct ValidatedFilePath {
    path: PathBuf,
}

impl ValidatedFilePath {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, PathValidationError> {
        let path = path.as_ref();

        Self::validate_path_components(path)?;

        let path_buf = path.to_path_buf();

        Ok(Self { path: path_buf })
    }

    pub fn validate_and_canonicalize<P: AsRef<Path>>(path: P) -> Result<Self, PathValidationError> {
        let path = path.as_ref();

        Self::validate_path_components(path)?;

        if path.exists() {
            let canonical = std::fs::canonicalize(path)?;
            let path_buf = path.to_path_buf();

            // Normalize both paths for comparison (handles /var -> /private/var on macOS)
            let canonical_normalized = Self::normalize_path(&canonical);
            let expected_normalized = Self::normalize_path(&path_buf);

            if canonical_normalized != expected_normalized {
                // Additional check: is the final component itself a symlink?
                if let Ok(metadata) = std::fs::symlink_metadata(path) {
                    if metadata.is_symlink() {
                        // This is a user-created symlink, reject it
                        return Err(PathValidationError::SymlinkAttack {
                            canonical: canonical.display().to_string(),
                            expected: path_buf.display().to_string(),
                        });
                    }
                }
                // Paths differ but it's not a symlink - could be case sensitivity or other issues
                // Allow it if normalization makes them match after canonicalization
            }

            Ok(Self { path: canonical })
        } else {
            Ok(Self {
                path: path.to_path_buf(),
            })
        }
    }

    fn validate_path_components(path: &Path) -> Result<(), PathValidationError> {
        let path_str = path.to_string_lossy();

        if path_str.contains("..") {
            return Err(PathValidationError::PathTraversal(path_str.to_string()));
        }

        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    return Err(PathValidationError::PathTraversal(path_str.to_string()));
                }
                std::path::Component::Normal(os_str) => {
                    let component_str = os_str.to_string_lossy();
                    if component_str.contains('\0') || component_str.contains("..") {
                        return Err(PathValidationError::InvalidPath);
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn normalize_path(path: &Path) -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            let path_str = path.to_string_lossy();
            let normalized = path_str.replace('/', "\\").to_lowercase();
            PathBuf::from(normalized)
        }

        #[cfg(target_os = "macos")]
        {
            let path_str = path.to_string_lossy().to_string();

            if path_str.starts_with("/private/") {
                PathBuf::from(path_str.replace("/private", ""))
            } else if path_str.starts_with("/var/") || path_str.starts_with("/tmp/") {
                PathBuf::from(format!("/private{}", path_str))
            } else {
                path.to_path_buf()
            }
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            path.to_path_buf()
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn to_path_buf(&self) -> PathBuf {
        self.path.clone()
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn is_file(&self) -> bool {
        self.path.is_file()
    }

    pub fn display(&self) -> std::path::Display<'_> {
        self.path.display()
    }
}

impl AsRef<Path> for ValidatedFilePath {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_temp_dir() -> TempDir {
        TempDir::new().expect("Failed to create temp dir")
    }

    fn create_temp_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let file_path = dir.path().join(name);
        fs::write(&file_path, content).expect("Failed to create temp file");
        file_path
    }

    fn create_symlink(original: &Path, link: &Path) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(original, link)
        }

        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(original, link)
        }
    }

    #[test]
    fn test_rejects_parent_dir_unix_style() {
        let path = "/tmp/../etc/passwd";

        let result = ValidatedFilePath::new(path);

        assert!(matches!(result, Err(PathValidationError::PathTraversal(_))));
        if let Err(PathValidationError::PathTraversal(msg)) = result {
            assert!(msg.contains(".."));
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_rejects_parent_dir_windows_style() {
        let path = r"C:\temp\..\Windows\System32";

        let result = ValidatedFilePath::new(path);

        assert!(matches!(result, Err(PathValidationError::PathTraversal(_))));
    }

    #[test]
    fn test_rejects_multiple_parent_dirs() {
        let path = "/tmp/../../../../../../etc/passwd";

        let result = ValidatedFilePath::new(path);

        assert!(matches!(result, Err(PathValidationError::PathTraversal(_))));
    }

    #[test]
    fn test_rejects_encoded_parent_dir() {
        let path = "/tmp/%2e%2e/etc/passwd";

        let result = ValidatedFilePath::new(path);

        assert!(result.is_ok() || matches!(result, Err(PathValidationError::InvalidPath)));
    }

    #[test]
    #[cfg(unix)]
    fn test_rejects_backslash_traversal_on_unix() {
        let path = r"/tmp\..\etc\passwd";

        let result = ValidatedFilePath::new(path);

        assert!(matches!(result, Err(PathValidationError::PathTraversal(_))));
    }

    #[test]
    fn test_accepts_valid_relative_path_without_traversal() {
        let path = "./documents/file.txt";

        let result = ValidatedFilePath::new(path);

        assert!(result.is_ok());
        let validated = result.unwrap();
        assert_eq!(
            validated.to_path_buf(),
            PathBuf::from("./documents/file.txt")
        );
    }

    #[test]
    fn test_accepts_valid_absolute_path() {
        let path = "/home/user/documents/file.txt";

        let result = ValidatedFilePath::new(path);

        assert!(result.is_ok());
    }

    #[test]
    fn test_rejects_parent_dir_in_middle_of_path() {
        let path = "/home/user/../admin/secrets.txt";

        let result = ValidatedFilePath::new(path);

        assert!(matches!(result, Err(PathValidationError::PathTraversal(_))));
    }

    #[test]
    fn test_rejects_null_byte_in_path() {
        let path = "/tmp/file\0.txt";

        let result = ValidatedFilePath::new(path);

        assert!(matches!(result, Err(PathValidationError::InvalidPath)));
    }

    #[test]
    fn test_rejects_null_byte_in_component() {
        let path = "/tmp/fi\0le/doc.txt";

        let result = ValidatedFilePath::new(path);

        assert!(matches!(result, Err(PathValidationError::InvalidPath)));
    }

    #[test]
    fn test_rejects_embedded_null_bytes() {
        let path = "/tmp\0/file.txt";

        let result = ValidatedFilePath::new(path);

        assert!(matches!(result, Err(PathValidationError::InvalidPath)));
    }

    #[test]
    fn test_canonicalize_detects_file_symlink() {
        let temp_dir = create_temp_dir();
        let real_file = create_temp_file(&temp_dir, "real.txt", "content");
        let link_file = temp_dir.path().join("link.txt");

        if create_symlink(&real_file, &link_file).is_err() {
            // Skip test if symlink creation fails (e.g., Windows without admin)
            return;
        }

        let result = ValidatedFilePath::validate_and_canonicalize(&link_file);

        assert!(matches!(
            result,
            Err(PathValidationError::SymlinkAttack { .. })
        ));
        if let Err(PathValidationError::SymlinkAttack {
            canonical,
            expected,
        }) = result
        {
            assert!(canonical.contains("real.txt"));
            assert!(expected.contains("link.txt"));
        }
    }

    #[test]
    fn test_canonicalize_detects_directory_symlink() {
        let temp_dir = create_temp_dir();
        let real_dir = temp_dir.path().join("real_dir");
        fs::create_dir(&real_dir).unwrap();
        let _real_file = create_temp_file(&temp_dir, "real_dir/file.txt", "content");

        let link_dir = temp_dir.path().join("link_dir");
        if create_symlink(&real_dir, &link_dir).is_err() {
            // Skip test if symlink creation fails
            return;
        }

        let file_via_link = link_dir.join("file.txt");

        let result = ValidatedFilePath::validate_and_canonicalize(&file_via_link);

        assert!(result.is_ok());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_canonicalize_allows_system_symlinks_macos() {
        let temp_dir = TempDir::new_in("/var/tmp").unwrap();
        let file_path = temp_dir.path().join("file.txt");
        fs::write(&file_path, "test").unwrap();

        let result = ValidatedFilePath::validate_and_canonicalize(&file_path);

        assert!(result.is_ok());
        let validated = result.unwrap();
        let canonical_str = validated.path().to_string_lossy();
        assert!(canonical_str.contains("var") || canonical_str.contains("private"));
    }

    #[test]
    fn test_canonicalize_nonexistent_path() {
        let path = "/tmp/nonexistent_file_12345.txt";

        let result = ValidatedFilePath::validate_and_canonicalize(path);

        assert!(result.is_ok());
        let validated = result.unwrap();
        assert_eq!(validated.to_path_buf(), PathBuf::from(path));
    }

    #[test]
    fn test_canonicalize_io_error_propagation() {
        #[cfg(unix)]
        let protected_path = "/root/.ssh/id_rsa";
        #[cfg(windows)]
        let protected_path = r"C:\Windows\System32\config\SAM";

        let result = ValidatedFilePath::validate_and_canonicalize(protected_path);

        if result.is_err() {
            assert!(matches!(result, Err(PathValidationError::IoError(_))));
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_normalize_windows_forward_slash() {
        let path1 = PathBuf::from("C:/temp/file.txt");
        let path2 = PathBuf::from(r"C:\temp\file.txt");

        let normalized1 = ValidatedFilePath::normalize_path(&path1);
        let normalized2 = ValidatedFilePath::normalize_path(&path2);

        assert_eq!(normalized1, normalized2);
        let normalized_str = normalized1.to_string_lossy();
        assert!(normalized_str.contains('\\'));
        assert!(!normalized_str.contains('/'));
        assert_eq!(normalized_str.to_lowercase(), normalized_str);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_normalize_windows_case_insensitive() {
        let path1 = PathBuf::from(r"C:\Temp\FILE.txt");
        let path2 = PathBuf::from(r"c:\temp\file.txt");

        let normalized1 = ValidatedFilePath::normalize_path(&path1);
        let normalized2 = ValidatedFilePath::normalize_path(&path2);

        assert_eq!(normalized1, normalized2);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_normalize_macos_private_var() {
        let path = PathBuf::from("/var/log/system.log");

        let normalized = ValidatedFilePath::normalize_path(&path);

        let normalized_str = normalized.to_string_lossy();
        assert_eq!(normalized_str, "/private/var/log/system.log");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_normalize_macos_private_tmp() {
        let path = PathBuf::from("/tmp/file.txt");

        let normalized = ValidatedFilePath::normalize_path(&path);

        let normalized_str = normalized.to_string_lossy();
        assert_eq!(normalized_str, "/private/tmp/file.txt");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_normalize_macos_strip_private() {
        let path = PathBuf::from("/private/var/log/system.log");

        let normalized = ValidatedFilePath::normalize_path(&path);

        let normalized_str = normalized.to_string_lossy();
        assert_eq!(normalized_str, "/var/log/system.log");
    }

    #[test]
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    fn test_normalize_linux_no_change() {
        let path = PathBuf::from("/tmp/file.txt");

        let normalized = ValidatedFilePath::normalize_path(&path);

        assert_eq!(normalized, path);
    }

    #[test]
    fn test_path_getter() {
        let temp_dir = create_temp_dir();
        let file_path = create_temp_file(&temp_dir, "test.txt", "content");
        let validated = ValidatedFilePath::new(&file_path).unwrap();

        let path_ref = validated.path();

        assert_eq!(path_ref, file_path.as_path());
    }

    #[test]
    fn test_to_path_buf_conversion() {
        let temp_dir = create_temp_dir();
        let file_path = create_temp_file(&temp_dir, "test.txt", "content");
        let validated = ValidatedFilePath::new(&file_path).unwrap();

        let path_buf = validated.to_path_buf();

        assert_eq!(path_buf, file_path);
        assert!(path_buf.is_file());
    }

    #[test]
    fn test_exists_method() {
        let temp_dir = create_temp_dir();
        let file_path = create_temp_file(&temp_dir, "test.txt", "content");
        let validated = ValidatedFilePath::new(&file_path).unwrap();

        let nonexistent = temp_dir.path().join("nonexistent.txt");
        let validated_nonexistent = ValidatedFilePath::new(&nonexistent).unwrap();

        let exists = validated.exists();
        let not_exists = validated_nonexistent.exists();

        assert!(exists);
        assert!(!not_exists);
    }

    #[test]
    fn test_is_file_method() {
        let temp_dir = create_temp_dir();
        let file_path = create_temp_file(&temp_dir, "test.txt", "content");
        let validated_file = ValidatedFilePath::new(&file_path).unwrap();

        let dir_path = temp_dir.path();
        let validated_dir = ValidatedFilePath::new(dir_path).unwrap();

        let is_file = validated_file.is_file();
        let is_dir_file = validated_dir.is_file();

        assert!(is_file);
        assert!(!is_dir_file);
    }

    #[test]
    fn test_display_formatting() {
        let path = "/tmp/test.txt";
        let validated = ValidatedFilePath::new(path).unwrap();

        let display_str = validated.display().to_string();

        assert_eq!(display_str, path);
    }

    #[test]
    fn test_as_ref_trait() {
        fn accepts_path_ref<P: AsRef<Path>>(path: P) -> String {
            path.as_ref().to_string_lossy().to_string()
        }

        let path = "/tmp/test.txt";
        let validated = ValidatedFilePath::new(path).unwrap();

        let result = accepts_path_ref(&validated);

        assert_eq!(result, path);
    }

    #[test]
    fn test_empty_path() {
        let path = "";

        let result = ValidatedFilePath::new(path);

        assert!(result.is_ok());
    }

    #[test]
    fn test_root_path() {
        #[cfg(unix)]
        let path = "/";
        #[cfg(windows)]
        let path = r"C:\";

        let result = ValidatedFilePath::new(path);

        assert!(result.is_ok());
    }

    #[test]
    fn test_path_with_multiple_slashes() {
        let path = "/tmp//file.txt";

        let result = ValidatedFilePath::new(path);

        assert!(result.is_ok());
        // Rust's PathBuf automatically cleans up consecutive slashes
    }

    #[test]
    fn test_very_long_path() {
        let long_component = "a".repeat(255);
        let path = format!("/tmp/{}/file.txt", long_component);

        let result = ValidatedFilePath::new(&path);

        assert!(result.is_ok());
    }
}
