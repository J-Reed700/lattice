use crate::shared::error::{AppError, Result};
use std::path::Path;

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
}
