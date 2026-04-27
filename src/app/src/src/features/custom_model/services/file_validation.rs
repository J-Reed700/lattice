use crate::features::custom_model::domain::{CustomModel, ModelArchitecture};
use crate::shared::domain_types::ValidatedFilePath;
use crate::shared::error::AppError;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Maximum file size: 5GB
const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024 * 1024;

/// Allowed file extensions for model files
const ALLOWED_EXTENSIONS: &[&str] = &[
    "gguf",        // GGUF format
    "onnx",        // ONNX format
    "safetensors", // SafeTensors format
    "pt",          // PyTorch format
    "pth",         // PyTorch format (alternative)
    "bin",         // Binary format (some models)
];

/// Service for validating model files and managing file operations
pub struct FileValidationService {
    models_dir: PathBuf,
}

impl FileValidationService {
    /// Create a new file validation service
    ///
    /// # Arguments
    /// * `models_dir` - Base directory where custom models will be stored
    pub fn new(models_dir: PathBuf) -> Self {
        Self { models_dir }
    }

    /// Validate file before saving to database
    ///
    /// Checks:
    /// - File exists
    /// - File size within limits (max 5GB)
    /// - File extension is allowed
    /// - Path is safe (no directory traversal)
    pub async fn validate_file(&self, file_path: &str) -> Result<ValidatedFilePath, AppError> {
        // Create validated path (prevents directory traversal - CWE-22)
        let validated_path = ValidatedFilePath::new(PathBuf::from(file_path))?;

        // Check file exists
        if !validated_path.as_path().exists() {
            return Err(AppError::ValidationFailed(format!(
                "File does not exist: {}",
                file_path
            )));
        }

        // Check file size
        let metadata = fs::metadata(validated_path.as_path())
            .await
            .map_err(|e| AppError::Io {
                message: format!("Failed to read file metadata: {}", e),
                kind: e.kind().to_string(),
            })?;

        if metadata.len() > MAX_FILE_SIZE {
            return Err(AppError::ValidationFailed(format!(
                "File size ({} bytes) exceeds maximum allowed size ({} bytes)",
                metadata.len(),
                MAX_FILE_SIZE
            )));
        }

        // Check file extension
        let extension = validated_path
            .as_path()
            .extension()
            .and_then(|ext| ext.to_str())
            .ok_or_else(|| AppError::ValidationFailed("File has no extension".to_string()))?;

        if !ALLOWED_EXTENSIONS.contains(&extension) {
            return Err(AppError::ValidationFailed(format!(
                "File extension '{}' is not allowed. Allowed extensions: {}",
                extension,
                ALLOWED_EXTENSIONS.join(", ")
            )));
        }

        Ok(validated_path)
    }

    /// Copy file to models directory
    ///
    /// Creates a safe directory structure: `<models_dir>/custom/<model_id>/<filename>`
    ///
    /// # Arguments
    /// * `source_path` - Source file path (already validated)
    /// * `model` - CustomModel with ID and metadata
    ///
    /// # Returns
    /// Path to the copied file within the models directory
    pub async fn copy_to_models_dir(
        &self,
        source_path: &ValidatedFilePath,
        model: &CustomModel,
    ) -> Result<PathBuf, AppError> {
        // Sanitize model_id (allow only alphanumeric and hyphens)
        let sanitized_id = self.sanitize_model_id(model.id());

        // Create destination directory: <models_dir>/custom/<model_id>/
        let dest_dir = self.models_dir.join("custom").join(&sanitized_id);
        fs::create_dir_all(&dest_dir)
            .await
            .map_err(|e| AppError::Io {
                message: format!("Failed to create model directory: {}", e),
                kind: e.kind().to_string(),
            })?;

        // Get filename from source path
        let filename = source_path
            .as_path()
            .file_name()
            .ok_or_else(|| AppError::ValidationFailed("Source path has no filename".to_string()))?;

        // Destination file path
        let dest_path = dest_dir.join(filename);

        // Copy file
        fs::copy(source_path.as_path(), &dest_path)
            .await
            .map_err(|e| AppError::Io {
                message: format!("Failed to copy file: {}", e),
                kind: e.kind().to_string(),
            })?;

        Ok(dest_path)
    }

    /// Deep validation: Verify file signature (magic bytes)
    ///
    /// Reads first 1MB of file and checks for valid model format:
    /// - GGUF magic bytes
    /// - ONNX magic bytes
    /// - SafeTensors magic bytes
    /// - PyTorch magic bytes
    pub async fn validate_file_deep(&self, file_path: &Path) -> Result<(), AppError> {
        // Read first 1MB
        const MAX_READ_SIZE: usize = 1024 * 1024; // 1MB
        let file = fs::File::open(file_path).await.map_err(|e| AppError::Io {
            message: format!("Failed to open file: {}", e),
            kind: e.kind().to_string(),
        })?;

        let mut buffer = vec![0u8; MAX_READ_SIZE];
        let bytes_read = {
            use tokio::io::AsyncReadExt;
            let mut reader = tokio::io::BufReader::new(file);
            reader.read(&mut buffer).await.map_err(|e| AppError::Io {
                message: format!("Failed to read file: {}", e),
                kind: e.kind().to_string(),
            })?
        };

        // Shrink buffer to actual bytes read
        buffer.truncate(bytes_read);

        // Verify file signature
        self.verify_file_signature(&buffer)?;

        Ok(())
    }

    /// Verify file signature (magic bytes) to ensure it's a valid model file
    fn verify_file_signature(&self, bytes: &[u8]) -> Result<(), AppError> {
        if bytes.len() < 4 {
            return Err(AppError::ValidationFailed(
                "File too small to verify signature".to_string(),
            ));
        }

        // GGUF magic: "GGUF" (0x47 0x47 0x55 0x46)
        if bytes.len() >= 4 {
            if let Some(slice) = bytes.get(0..4) {
                if slice == b"GGUF" {
                    return Ok(());
                }
            }
        }

        // ONNX magic: Protocol Buffers (0x08 followed by field number)
        // ONNX files typically start with 0x08 0x03 or 0x08 0x07 or 0x08 0x01
        if bytes.len() >= 2 {
            if let (Some(&byte0), Some(&byte1)) = (bytes.first(), bytes.get(1)) {
                if byte0 == 0x08 && (byte1 == 0x03 || byte1 == 0x07 || byte1 == 0x01) {
                    return Ok(());
                }
            }
        }

        // SafeTensors magic: starts with 8-byte little-endian header size
        // We'll accept it if first 8 bytes look like a reasonable header size (< 10MB)
        if bytes.len() >= 8 {
            if let Some(header_bytes) = bytes.get(0..8) {
                if let Ok(header_array) = <[u8; 8]>::try_from(header_bytes) {
                    let header_size = u64::from_le_bytes(header_array);
                    if header_size > 0 && header_size < 10_000_000 {
                        // Likely SafeTensors
                        return Ok(());
                    }
                }
            }
        }

        // PyTorch magic: ZIP archive (0x50 0x4B 0x03 0x04) - .pt files are ZIP files
        if bytes.len() >= 4 {
            if let Some(slice) = bytes.get(0..4) {
                if slice == b"PK\x03\x04" {
                    return Ok(());
                }
            }
        }

        // If none match, reject
        Err(AppError::ValidationFailed(
            "File does not appear to be a valid model format (GGUF, ONNX, SafeTensors, or PyTorch)"
                .to_string(),
        ))
    }

    /// Sanitize model ID for use in filesystem paths
    ///
    /// Allows only alphanumeric characters and hyphens to prevent path traversal
    fn sanitize_model_id(&self, model_id: &str) -> String {
        model_id
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect()
    }

    /// Delete model file from disk
    ///
    /// # Arguments
    /// * `model` - CustomModel with file path information
    pub async fn delete_model_file(&self, model: &CustomModel) -> Result<(), AppError> {
        // Get file path from file_info
        let path = model.file_info().path();
        if path.exists() {
            fs::remove_file(path).await.map_err(|e| AppError::Io {
                message: format!("Failed to delete file: {}", e),
                kind: e.kind().to_string(),
            })?;

            // Also try to delete parent directory if empty
            if let Some(parent) = path.parent() {
                if parent.exists() {
                    // Ignore errors if directory is not empty
                    let _ = fs::remove_dir(parent).await;
                }
            }
        }

        Ok(())
    }

    /// Get the expected path for a model file
    ///
    /// Returns: `<models_dir>/custom/<model_id>/<filename>`
    pub fn get_model_file_path(&self, model_id: &str, filename: &str) -> PathBuf {
        let sanitized_id = self.sanitize_model_id(model_id);
        self.models_dir
            .join("custom")
            .join(sanitized_id)
            .join(filename)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_models_dir() -> PathBuf {
        std::env::temp_dir().join("models")
    }

    #[test]
    fn test_sanitize_model_id() {
        let service = FileValidationService::new(temp_models_dir());

        // Valid characters
        assert_eq!(service.sanitize_model_id("model-123"), "model-123");
        assert_eq!(service.sanitize_model_id("model_abc"), "model_abc");

        // Remove invalid characters
        assert_eq!(service.sanitize_model_id("../etc/passwd"), "etcpasswd");
        assert_eq!(service.sanitize_model_id("model/../hack"), "modelhack");
        assert_eq!(service.sanitize_model_id("model/../../etc"), "modeletc");
    }

    #[test]
    fn test_verify_file_signature_gguf() {
        let service = FileValidationService::new(temp_models_dir());

        // GGUF magic bytes
        let gguf_bytes = b"GGUF\x00\x00\x00\x01";
        assert!(service.verify_file_signature(gguf_bytes).is_ok());
    }

    #[test]
    fn test_verify_file_signature_pytorch() {
        let service = FileValidationService::new(temp_models_dir());

        // PyTorch (ZIP) magic bytes
        let pytorch_bytes = b"PK\x03\x04\x00\x00\x00\x00";
        assert!(service.verify_file_signature(pytorch_bytes).is_ok());
    }

    #[test]
    fn test_verify_file_signature_invalid() {
        let service = FileValidationService::new(temp_models_dir());

        // Invalid file
        let invalid_bytes = b"INVALID FILE";
        assert!(service.verify_file_signature(invalid_bytes).is_err());
    }

    #[test]
    fn test_get_model_file_path() {
        let service = FileValidationService::new(temp_models_dir());

        let path = service.get_model_file_path("my-model-123", "model.gguf");
        assert_eq!(
            path,
            temp_models_dir().join("custom/my-model-123/model.gguf")
        );
    }
}
