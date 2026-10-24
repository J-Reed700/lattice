use lattice_core::shared::error::{AppError, Result};
use serde_json::Value;

/// Upper bound on a chat message, in characters: room for a long pasted article.
pub const MAX_CHAT_MESSAGE_CHARS: usize = 200_000;

/// Validates and sanitizes user inputs.
#[derive(Debug, Clone)]
pub struct InputValidator;

impl Default for InputValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl InputValidator {
    pub fn new() -> Self {
        Self
    }

    /// Validate and sanitize a search query.
    /// Note: SQL injection protection is handled by SQLx parameterized queries (.bind()), not character filtering.
    pub fn validate_search_query(&self, query: &str) -> Result<String> {
        if query.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Search query cannot be empty".to_string(),
            ));
        }

        if query.len() > 10000 {
            return Err(AppError::InvalidInput(
                "Search query too long (max 10000 characters)".to_string(),
            ));
        }

        Ok(query.trim().to_string())
    }

    /// Validate a chat message. Chat turns routinely carry a pasted article,
    /// so the cap is far above a search query's and is counted in characters
    /// (not bytes) so non-Latin text gets the same allowance.
    pub fn validate_chat_message(&self, message: &str) -> Result<String> {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return Err(AppError::InvalidInput(
                "Message cannot be empty".to_string(),
            ));
        }

        if trimmed.chars().count() > MAX_CHAT_MESSAGE_CHARS {
            return Err(AppError::InvalidInput(format!(
                "Message too long (max {MAX_CHAT_MESSAGE_CHARS} characters)"
            )));
        }

        Ok(trimmed.to_string())
    }

    /// Validate JSON input for deserialization
    pub fn validate_json(json_str: &str, max_depth: usize) -> Result<Value> {
        if json_str.len() > 10 * 1024 * 1024 {
            return Err(AppError::InvalidInput(
                "JSON input too large (max 10MB)".to_string(),
            ));
        }

        let value: Value = serde_json::from_str(json_str)?;

        if Self::get_json_depth(&value) > max_depth {
            return Err(AppError::InvalidInput(format!(
                "JSON nesting too deep (max depth: {})",
                max_depth
            )));
        }

        Ok(value)
    }

    fn get_json_depth(value: &Value) -> usize {
        match value {
            Value::Object(map) => 1 + map.values().map(Self::get_json_depth).max().unwrap_or(0),
            Value::Array(arr) => 1 + arr.iter().map(Self::get_json_depth).max().unwrap_or(0),
            _ => 0,
        }
    }

    pub fn validate_file_upload(
        filename: &str,
        content: &[u8],
        allowed_extensions: &[&str],
        max_size: usize,
    ) -> Result<()> {
        if filename.is_empty() {
            return Err(AppError::InvalidInput(
                "Filename cannot be empty".to_string(),
            ));
        }

        if content.len() > max_size {
            return Err(AppError::InvalidInput(format!(
                "File too large: {} bytes (max: {} bytes)",
                content.len(),
                max_size
            )));
        }

        let extension = filename
            .rsplit('.')
            .next()
            .ok_or_else(|| AppError::InvalidInput("File has no extension".to_string()))?
            .to_lowercase();

        if !allowed_extensions.contains(&extension.as_str()) {
            return Err(AppError::InvalidInput(format!(
                "File type '{}' not allowed. Allowed types: {:?}",
                extension, allowed_extensions
            )));
        }

        if !Self::verify_file_signature(content, &extension) {
            return Err(AppError::InvalidInput(
                "File content does not match extension".to_string(),
            ));
        }

        Ok(())
    }

    fn verify_file_signature(content: &[u8], extension: &str) -> bool {
        if content.is_empty() {
            return false;
        }

        match extension {
            "pdf" => content.starts_with(b"%PDF"),
            "png" => content.starts_with(&[0x89, 0x50, 0x4E, 0x47]),
            "jpg" | "jpeg" => content.starts_with(&[0xFF, 0xD8, 0xFF]),
            "gif" => content.starts_with(b"GIF87a") || content.starts_with(b"GIF89a"),
            "zip" => content.starts_with(&[0x50, 0x4B, 0x03, 0x04]),
            "txt" | "md" | "json" | "xml" | "html" | "css" | "js" => true,
            _ => true,
        }
    }

    pub fn sanitize_html(html: &str) -> String {
        let mut sanitized = html.to_string();
        sanitized = sanitized.replace("<script", "&lt;script");
        sanitized = sanitized.replace("</script>", "&lt;/script&gt;");

        let event_handlers = [
            "onload",
            "onerror",
            "onclick",
            "onmouseover",
            "onmouseout",
            "onkeydown",
            "onkeyup",
            "onfocus",
            "onblur",
            "onchange",
        ];

        for handler in &event_handlers {
            sanitized = sanitized.replace(handler, "data-disabled");
        }

        sanitized = sanitized.replace("javascript:", "");
        sanitized = sanitized.replace("data:", "");
        sanitized = sanitized.replace("vbscript:", "");

        sanitized
    }

    pub fn validate_email(email: &str) -> Result<String> {
        if email.is_empty() {
            return Err(AppError::InvalidInput("Email cannot be empty".to_string()));
        }

        if email.len() > 254 {
            return Err(AppError::InvalidInput("Email too long".to_string()));
        }

        if !email.contains('@') || email.starts_with('@') || email.ends_with('@') {
            return Err(AppError::InvalidInput("Invalid email format".to_string()));
        }

        let parts: Vec<&str> = email.split('@').collect();
        let (local, domain) = match parts.as_slice() {
            [local, domain] => (*local, *domain),
            _ => return Err(AppError::InvalidInput("Invalid email format".to_string())),
        };

        if local.is_empty() || domain.is_empty() {
            return Err(AppError::InvalidInput("Invalid email format".to_string()));
        }

        if !domain.contains('.') {
            return Err(AppError::InvalidInput("Invalid email domain".to_string()));
        }

        Ok(email.to_lowercase())
    }

    pub fn validate_url(url: &str) -> Result<String> {
        if url.is_empty() {
            return Err(AppError::InvalidInput("URL cannot be empty".to_string()));
        }

        if url.len() > 2048 {
            return Err(AppError::InvalidInput("URL too long".to_string()));
        }

        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(AppError::InvalidInput(
                "Only HTTP/HTTPS URLs are allowed".to_string(),
            ));
        }

        let sanitized = url
            .replace("<", "%3C")
            .replace(">", "%3E")
            .replace("\"", "%22")
            .replace("'", "%27");

        Ok(sanitized)
    }

    /// Validate model name/ID for model management operations (prevents path traversal).
    pub fn validate_model_name(&self, name: &str) -> Result<String> {
        if name.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Model name cannot be empty".to_string(),
            ));
        }

        if name.len() > 255 {
            return Err(AppError::InvalidInput(
                "Model name must be 1-255 characters".to_string(),
            ));
        }

        if name.contains("..") {
            return Err(AppError::InvalidInput(
                "Model name cannot contain '..' (path traversal)".to_string(),
            ));
        }

        if name.contains('/') || name.contains('\\') {
            return Err(AppError::InvalidInput(
                "Model name cannot contain path separators".to_string(),
            ));
        }

        if name.contains('\0') {
            return Err(AppError::InvalidInput(
                "Model name contains null bytes".to_string(),
            ));
        }

        Ok(name.trim().to_string())
    }

    /// Validate directory path for watch folder operations.
    pub fn validate_directory_path(&self, path: &str, require_exists: bool) -> Result<String> {
        if path.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Directory path cannot be empty".to_string(),
            ));
        }

        if path.contains('\0') {
            return Err(AppError::Security(
                "Path contains null bytes (CWE-158)".to_string(),
            ));
        }

        if path.contains("..") {
            return Err(AppError::Security(format!(
                "Path traversal detected (CWE-22): path contains '..' - {}",
                path
            )));
        }

        let path_obj = std::path::Path::new(path);
        if !path_obj.is_absolute() {
            return Err(AppError::InvalidInput(
                "Directory path must be absolute".to_string(),
            ));
        }

        if require_exists {
            if !path_obj.exists() {
                return Err(AppError::FileNotFound {
                    path: path.to_string(),
                });
            }

            let metadata = std::fs::metadata(path_obj).map_err(|e| {
                AppError::FileSystem(format!("Failed to read directory metadata: {}", e))
            })?;

            if !metadata.is_dir() {
                return Err(AppError::InvalidInput(format!(
                    "Path is not a directory: {}",
                    path
                )));
            }
        }

        Ok(path.trim().to_string())
    }

    /// Validate model file path to prevent traversal or symlink escapes.
    pub fn validate_model_path(
        &self,
        path: &std::path::Path,
        models_dir: &std::path::Path,
    ) -> Result<std::path::PathBuf> {
        let path_str = path
            .to_str()
            .ok_or_else(|| AppError::Security("Path contains invalid UTF-8".to_string()))?;

        if path_str.contains('\0') {
            return Err(AppError::Security(
                "Path contains null bytes (CWE-158)".to_string(),
            ));
        }

        if path_str.contains("..") {
            return Err(AppError::Security(format!(
                "Path traversal detected (CWE-22): path contains '..' - {}",
                path.display()
            )));
        }

        if !path.exists() {
            return Err(AppError::FileNotFound {
                path: path.display().to_string(),
            });
        }

        let metadata = std::fs::symlink_metadata(path)
            .map_err(|e| AppError::FileSystem(format!("Failed to read file metadata: {}", e)))?;

        if metadata.file_type().is_symlink() {
            return Err(AppError::Security(format!(
                "Path is a symlink (CWE-59): {}",
                path.display()
            )));
        }

        if metadata.is_dir() {
            let config_marker = path.join("config.json");
            if !config_marker.is_file() {
                return Err(AppError::InvalidInput(format!(
                    "Model directory missing config.json marker — expected a safetensors layout: {}",
                    path.display()
                )));
            }
        } else if metadata.is_file() {
            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .ok_or_else(|| {
                    AppError::InvalidInput(format!("File has no extension: {}", path.display()))
                })?;

            if extension != "gguf" && extension != "safetensors" {
                return Err(AppError::InvalidInput(format!(
                    "Invalid model file extension '{}' (expected '.gguf' or '.safetensors'; or a directory containing config.json): {}",
                    extension,
                    path.display()
                )));
            }
        } else {
            return Err(AppError::Security(format!(
                "Path is neither a regular file nor a directory: {}",
                path.display()
            )));
        }

        let canonical_path = path.canonicalize().map_err(|e| {
            AppError::Security(format!(
                "Failed to canonicalize path (possible symlink attack): {}",
                e
            ))
        })?;

        let canonical_models_dir = models_dir.canonicalize().map_err(|e| {
            AppError::InvalidState(format!("Models directory doesn't exist: {}", e))
        })?;

        if !canonical_path.starts_with(&canonical_models_dir) {
            return Err(AppError::Security(format!(
                "Path traversal detected (CWE-22): model file '{}' is outside allowed directory '{}'",
                canonical_path.display(),
                canonical_models_dir.display()
            )));
        }

        Ok(canonical_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::shared::testing::paths as test_paths;

    #[test]
    fn test_search_query_validation() {
        let validator = InputValidator::new();

        assert!(validator.validate_search_query("normal query").is_ok());
        assert!(validator.validate_search_query("O'Brien").is_ok());
        assert!(validator.validate_search_query("don't").is_ok());
        assert!(validator
            .validate_search_query("SELECT * FROM table;")
            .is_ok());

        assert!(validator.validate_search_query("").is_err());
        assert!(validator.validate_search_query("   ").is_err());

        let result = validator
            .validate_search_query("O'Brien's \"research\" on SQL;")
            .unwrap();
        assert_eq!(result, "O'Brien's \"research\" on SQL;");
    }

    #[test]
    fn chat_message_allows_a_pasted_article_and_counts_characters() {
        let validator = InputValidator::new();

        let article = "word ".repeat(10_000);
        assert!(validator.validate_search_query(&article).is_err());
        assert_eq!(
            validator
                .validate_chat_message(&format!("  {article}  "))
                .unwrap(),
            article.trim()
        );

        let at_cap = "é".repeat(MAX_CHAT_MESSAGE_CHARS);
        assert!(validator.validate_chat_message(&at_cap).is_ok());
        assert!(validator
            .validate_chat_message(&format!("{at_cap}é"))
            .is_err());

        assert!(validator.validate_chat_message("   ").is_err());
    }

    #[test]
    fn test_json_validation() {
        let valid_json = r#"{"key": "value"}"#;
        assert!(InputValidator::validate_json(valid_json, 10).is_ok());

        let deep_json = r#"{"a":{"b":{"c":{"d":{"e":"f"}}}}}"#;
        assert!(InputValidator::validate_json(deep_json, 3).is_err());
        assert!(InputValidator::validate_json(deep_json, 10).is_ok());
    }

    #[test]
    fn test_email_validation() {
        assert!(InputValidator::validate_email("test@example.com").is_ok());
        assert!(InputValidator::validate_email("invalid").is_err());
        assert!(InputValidator::validate_email("@example.com").is_err());
        assert!(InputValidator::validate_email("test@").is_err());
    }

    #[test]
    fn test_model_name_validation() {
        let validator = InputValidator::new();

        assert!(validator
            .validate_model_name("llama-3.2-1b-instruct")
            .is_ok());
        assert!(validator.validate_model_name("model-name-v1").is_ok());
        assert!(validator.validate_model_name("simple_model").is_ok());

        assert!(validator.validate_model_name("").is_err());
        assert!(validator.validate_model_name("   ").is_err());

        assert!(validator
            .validate_model_name("../../../etc/passwd")
            .is_err());
        assert!(validator.validate_model_name("..").is_err());

        assert!(validator.validate_model_name("path/to/model").is_err());
        assert!(validator.validate_model_name("path\\to\\model").is_err());

        assert!(validator.validate_model_name("model\0name").is_err());

        let long_name = "a".repeat(256);
        assert!(validator.validate_model_name(&long_name).is_err());
    }

    #[test]
    fn test_directory_path_validation() {
        let validator = InputValidator::new();

        assert!(validator
            .validate_directory_path(&test_paths::abs_str("Users/example/Documents"), false)
            .is_ok());
        assert!(validator
            .validate_directory_path(&test_paths::abs_str("home/user/folder"), false)
            .is_ok());

        #[cfg(target_os = "windows")]
        assert!(validator
            .validate_directory_path("C:\\Users\\josh\\Documents", false)
            .is_ok());

        assert!(validator.validate_directory_path("", false).is_err());
        assert!(validator.validate_directory_path("   ", false).is_err());

        assert!(validator
            .validate_directory_path(&test_paths::abs_str("Users/example/../../../etc"), false)
            .is_err());
        assert!(validator
            .validate_directory_path(&test_paths::abs_str("home/../root"), false)
            .is_err());
        assert!(validator
            .validate_directory_path("/var/log/..\\..\\etc", false)
            .is_err());

        assert!(validator
            .validate_directory_path("Documents/folder", false)
            .is_err());
        assert!(validator
            .validate_directory_path("./folder", false)
            .is_err());
        assert!(validator.validate_directory_path("folder", false).is_err());

        assert!(validator
            .validate_directory_path("/Users/example\0/evil", false)
            .is_err());

        let temp = tempfile::tempdir().unwrap();
        let temp_path = temp.path().to_str().unwrap();

        assert!(validator.validate_directory_path(temp_path, true).is_ok());

        assert!(validator
            .validate_directory_path(&test_paths::abs_str("nonexistent/path/xyz123"), true)
            .is_err());
    }
}
