use crate::shared::error::{AppError, Result};
use std::path::{Path, PathBuf};

pub fn path_to_string(path: &Path) -> Result<String> {
    if path.as_os_str().is_empty() {
        return Err(AppError::InvalidInput("Path is empty".to_string()));
    }

    let path_str = path.to_str().ok_or_else(|| {
        AppError::InvalidInput(format!("Path contains invalid UTF-8: {}", path.display()))
    })?;

    Ok(path_str.to_string())
}

pub fn validate_path(path: &Path) -> Result<String> {
    if !path.exists() {
        return Err(AppError::FileNotFound {
            path: path.display().to_string(),
        });
    }

    path_to_string(path)
}

/// A path spelled the way a person writes it, for messages and logs: Windows
/// canonicalisation yields a verbatim `\\?\` path, which nobody should have to
/// read and many Win32 programs reject.
#[cfg(not(windows))]
pub fn plainly_spelled(path: PathBuf) -> PathBuf {
    path
}

#[cfg(windows)]
pub fn plainly_spelled(path: PathBuf) -> PathBuf {
    use std::path::{Component, Prefix};

    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return path;
    };
    // Only the drive form is safe to simplify. Verbatim UNC (`\\?\UNC\…`) and
    // device paths (`\\?\PIPE\…`) are not the same path without their prefix,
    // so they keep it even though they read badly.
    let Prefix::VerbatimDisk(letter) = prefix.kind() else {
        return path;
    };
    // The prefix is consumed; `RootDir` is the separator we just rewrote.
    let mut plain = PathBuf::from(format!("{}:\\", letter as char));
    plain.extend(components.filter(|component| !matches!(component, Component::RootDir)));
    plain
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_empty_path() {
        let empty_path = PathBuf::from("");
        let result = path_to_string(&empty_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_nonexistent_path() {
        let path = PathBuf::from("/nonexistent/file.txt");
        let result = validate_path(&path);
        assert!(result.is_err());
        // AppError::FileNotFound displays as "File not found: {path}"
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not found") || err_msg.contains("nonexistent"));
    }

    #[test]
    fn test_valid_path() {
        let path = PathBuf::from("Cargo.toml");
        if path.exists() {
            let result = validate_path(&path);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_path_to_string_valid() {
        let path = PathBuf::from("some/path");
        let result = path_to_string(&path);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "some/path");
    }

    #[cfg(windows)]
    #[test]
    fn plainly_spelled_drops_the_verbatim_prefix_of_a_drive_path() {
        assert_eq!(
            plainly_spelled(PathBuf::from(r"\\?\C:\a\b")),
            PathBuf::from(r"C:\a\b")
        );
    }

    #[cfg(windows)]
    #[test]
    fn plainly_spelled_keeps_a_verbatim_unc_path() {
        let unc = PathBuf::from(r"\\?\UNC\server\share\x");
        assert_eq!(plainly_spelled(unc.clone()), unc);
    }

    #[cfg(windows)]
    #[test]
    fn plainly_spelled_keeps_a_plain_drive_path() {
        let plain = PathBuf::from(r"C:\a");
        assert_eq!(plainly_spelled(plain.clone()), plain);
    }

    #[cfg(not(windows))]
    #[test]
    fn plainly_spelled_keeps_a_path_unchanged() {
        assert_eq!(
            plainly_spelled(PathBuf::from("/a/b")),
            PathBuf::from("/a/b")
        );
    }
}
