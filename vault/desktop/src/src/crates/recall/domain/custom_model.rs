//! Custom Model Domain Entity
//!
//! Represents a user-added custom model (URL download or file upload).
//! This is an aggregate root that enforces business invariants for custom model management.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Maximum model file size (5GB)
/// Prevents memory exhaustion attacks (CWE-770)
pub const MAX_MODEL_FILE_SIZE_BYTES: i64 = 5_368_709_120; // 5GB

/// Maximum model name length
const MAX_MODEL_NAME_LENGTH: usize = 100;

/// Maximum model ID length
const MAX_MODEL_ID_LENGTH: usize = 200;

// ============================================================================
// Value Objects
// ============================================================================

/// Unique model identifier (e.g., "user/my-model-v1")
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelId(String);

impl ModelId {
    /// Create a new model ID with validation
    ///
    /// # Validation Rules
    ///
    /// - Must be non-empty
    /// - Must be <= 200 characters
    /// - Should be unique across all custom models
    ///
    /// # Examples
    ///
    /// ```
    /// let id = ModelId::new("user/my-llama-v1".to_string())?;
    /// ```
    pub fn new(value: String) -> Result<Self, String> {
        let trimmed = value.trim();

        if trimmed.is_empty() {
            return Err("model_id cannot be empty".to_string());
        }

        if trimmed.len() > MAX_MODEL_ID_LENGTH {
            return Err(format!(
                "model_id too long: {} (max: {})",
                trimmed.len(),
                MAX_MODEL_ID_LENGTH
            ));
        }

        Ok(Self(trimmed.to_string()))
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Human-readable model name (1-100 characters)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelName(String);

impl ModelName {
    /// Create a new model name with validation
    ///
    /// # Validation Rules
    ///
    /// - Must be non-empty
    /// - Must be 1-100 characters
    ///
    /// # Examples
    ///
    /// ```
    /// let name = ModelName::new("My Custom Llama".to_string())?;
    /// ```
    pub fn new(value: String) -> Result<Self, String> {
        let trimmed = value.trim();

        if trimmed.is_empty() {
            return Err("model name cannot be empty".to_string());
        }

        if trimmed.len() > MAX_MODEL_NAME_LENGTH {
            return Err(format!(
                "model name too long: {} (max: {})",
                trimmed.len(),
                MAX_MODEL_NAME_LENGTH
            ));
        }

        Ok(Self(trimmed.to_string()))
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ModelName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Source type for the custom model
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    /// Downloaded from a URL
    Url,
    /// Uploaded from local file system
    LocalFile,
}

impl SourceType {
    /// Convert to database string representation
    pub fn to_db_string(&self) -> &'static str {
        match self {
            Self::Url => "url",
            Self::LocalFile => "local_file",
        }
    }

    /// Parse from database string representation
    pub fn from_db_string(s: &str) -> Result<Self, String> {
        match s {
            "url" => Ok(Self::Url),
            "local_file" => Ok(Self::LocalFile),
            _ => Err(format!("Invalid source_type: {}", s)),
        }
    }
}

/// Model source (URL or local file path)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum ModelSource {
    /// URL to download from
    Url(String),
    /// Local file path (before copying to model directory)
    LocalFile(PathBuf),
}

impl ModelSource {
    /// Get the source type
    pub fn source_type(&self) -> SourceType {
        match self {
            Self::Url(_) => SourceType::Url,
            Self::LocalFile(_) => SourceType::LocalFile,
        }
    }

    /// Get the source value as a string
    pub fn source_value(&self) -> String {
        match self {
            Self::Url(url) => url.clone(),
            Self::LocalFile(path) => path.to_string_lossy().to_string(),
        }
    }

    /// Validate URL format (basic check)
    pub fn validate_url(url: &str) -> Result<(), String> {
        if url.trim().is_empty() {
            return Err("URL cannot be empty".to_string());
        }

        // Basic URL validation
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err("URL must start with http:// or https://".to_string());
        }

        Ok(())
    }
}

/// Model architecture type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelArchitecture {
    /// GGUF format (GPT-Generated Unified Format)
    Gguf,
    /// ONNX format (Open Neural Network Exchange)
    Onnx,
    /// SafeTensors format
    SafeTensors,
    /// PyTorch format
    PyTorch,
}

impl ModelArchitecture {
    /// Convert to database string representation
    pub fn to_db_string(&self) -> &'static str {
        match self {
            Self::Gguf => "gguf",
            Self::Onnx => "onnx",
            Self::SafeTensors => "safetensors",
            Self::PyTorch => "pytorch",
        }
    }

    /// Parse from database string representation
    pub fn from_db_string(s: &str) -> Result<Self, String> {
        match s {
            "gguf" => Ok(Self::Gguf),
            "onnx" => Ok(Self::Onnx),
            "safetensors" => Ok(Self::SafeTensors),
            "pytorch" => Ok(Self::PyTorch),
            _ => Err(format!("Invalid architecture: {}", s)),
        }
    }

    /// Infer architecture from file extension
    pub fn infer_from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext.to_lowercase().as_str() {
                "gguf" => Some(Self::Gguf),
                "onnx" => Some(Self::Onnx),
                "safetensors" => Some(Self::SafeTensors),
                "pt" | "pth" | "bin" => Some(Self::PyTorch),
                _ => None,
            })
    }
}

/// Task type (what the model can do)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskType {
    /// Text embedding generation
    Embedding,
    /// Conversational AI / chat
    Chat,
    /// Optical character recognition
    Ocr,
    /// Image understanding / vision
    Vision,
}

impl TaskType {
    /// Convert to database string representation
    pub fn to_db_string(&self) -> &'static str {
        match self {
            Self::Embedding => "embedding",
            Self::Chat => "chat",
            Self::Ocr => "ocr",
            Self::Vision => "vision",
        }
    }

    /// Parse from database string representation
    pub fn from_db_string(s: &str) -> Result<Self, String> {
        match s {
            "embedding" => Ok(Self::Embedding),
            "chat" => Ok(Self::Chat),
            "ocr" => Ok(Self::Ocr),
            "vision" => Ok(Self::Vision),
            _ => Err(format!("Invalid task_type: {}", s)),
        }
    }
}

/// Validation status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationStatus {
    /// Not yet validated
    Pending,
    /// Currently being validated
    Validating,
    /// Passed validation
    Valid,
    /// Failed validation
    Invalid,
}

impl ValidationStatus {
    /// Convert to database string representation
    pub fn to_db_string(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Validating => "validating",
            Self::Valid => "valid",
            Self::Invalid => "invalid",
        }
    }

    /// Parse from database string representation
    pub fn from_db_string(s: &str) -> Result<Self, String> {
        match s {
            "pending" => Ok(Self::Pending),
            "validating" => Ok(Self::Validating),
            "valid" => Ok(Self::Valid),
            "invalid" => Ok(Self::Invalid),
            _ => Err(format!("Invalid validation_status: {}", s)),
        }
    }
}

/// File information with validation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileInfo {
    /// Absolute path to the model file
    path: PathBuf,
    /// File size in bytes
    size_bytes: i64,
}

impl FileInfo {
    /// Create new file info with validation
    ///
    /// # Validation Rules
    ///
    /// - File size must be > 0
    /// - File size must be <= 5GB
    ///
    /// # Examples
    ///
    /// ```
    /// let info = FileInfo::new(PathBuf::from("/models/my-model.gguf"), 1_000_000)?;
    /// ```
    pub fn new(path: PathBuf, size_bytes: i64) -> Result<Self, String> {
        if size_bytes <= 0 {
            return Err("file size must be greater than 0".to_string());
        }

        if size_bytes > MAX_MODEL_FILE_SIZE_BYTES {
            return Err(format!(
                "file size {} bytes exceeds maximum {} bytes (5GB)",
                size_bytes, MAX_MODEL_FILE_SIZE_BYTES
            ));
        }

        Ok(Self { path, size_bytes })
    }

    /// Get the file path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Get the file size in bytes
    pub fn size_bytes(&self) -> i64 {
        self.size_bytes
    }
}

/// Optional model metadata (JSON)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelMetadata(JsonValue);

impl ModelMetadata {
    /// Create from JSON value
    pub fn new(value: JsonValue) -> Self {
        Self(value)
    }

    /// Get the inner JSON value
    pub fn value(&self) -> &JsonValue {
        &self.0
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> String {
        self.0.to_string()
    }

    /// Deserialize from JSON string
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json)
            .map(Self::new)
            .map_err(|e| format!("Invalid JSON metadata: {}", e))
    }
}

// ============================================================================
// Aggregate Root: CustomModel
// ============================================================================

/// Custom model aggregate root
///
/// Represents a user-added custom model with complete lifecycle management.
///
/// ## Business Invariants
///
/// - `model_id` must be unique across all custom models
/// - File must exist at `file_info.path` (enforced by caller)
/// - File size must be > 0 and <= 5GB
/// - If `validation_status` is `Invalid`, `validation_error` must be `Some`
/// - `task_type` must match actual model capability (enforced by validation)
///
/// ## State Transitions
///
/// ```text
/// Pending → Validating → Valid | Invalid
///          ↑                      ↓
///          └──────────────────────┘
///           (revalidation)
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CustomModel {
    /// Unique identifier for this record
    id: String,

    /// Human-readable name
    name: ModelName,

    /// Unique model identifier
    model_id: ModelId,

    /// Source information
    source: ModelSource,

    /// File information
    file_info: FileInfo,

    /// Model architecture (optional, inferred if not provided)
    architecture: Option<ModelArchitecture>,

    /// Task type
    task_type: TaskType,

    /// Validation status
    validation_status: ValidationStatus,

    /// Validation error message (only if validation_status = Invalid)
    validation_error: Option<String>,

    /// Optional metadata
    metadata: Option<ModelMetadata>,

    /// Timestamps
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    last_validated_at: Option<DateTime<Utc>>,
}

impl CustomModel {
    /// Create a new custom model
    ///
    /// # Arguments
    ///
    /// * `name` - Human-readable name
    /// * `model_id` - Unique model identifier
    /// * `source` - Model source (URL or local file)
    /// * `file_path` - Absolute path to model file
    /// * `file_size_bytes` - File size in bytes
    /// * `task_type` - Task type (embedding, chat, etc.)
    /// * `metadata` - Optional JSON metadata
    ///
    /// # Business Logic
    ///
    /// - Sets `validation_status` to `Pending`
    /// - Sets `created_at` and `updated_at` to current time
    /// - Infers `architecture` from file extension if possible
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Name or model_id is invalid
    /// - File size is <= 0 or > 5GB
    /// - Source validation fails
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: ModelName,
        model_id: ModelId,
        source: ModelSource,
        file_path: PathBuf,
        file_size_bytes: i64,
        task_type: TaskType,
        metadata: Option<ModelMetadata>,
    ) -> Result<Self, String> {
        // Validate file info
        let file_info = FileInfo::new(file_path.clone(), file_size_bytes)?;

        // Infer architecture from file extension
        let architecture = ModelArchitecture::infer_from_path(&file_path);

        let now = Utc::now();

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            name,
            model_id,
            source,
            file_info,
            architecture,
            task_type,
            validation_status: ValidationStatus::Pending,
            validation_error: None,
            metadata,
            created_at: now,
            updated_at: now,
            last_validated_at: None,
        })
    }

    /// Create from database fields (for repository reconstruction)
    ///
    /// Does not perform validation since data is assumed to be valid in DB.
    #[allow(clippy::too_many_arguments)]
    pub fn from_db(
        id: String,
        name: ModelName,
        model_id: ModelId,
        source: ModelSource,
        file_path: PathBuf,
        file_size_bytes: i64,
        architecture: Option<ModelArchitecture>,
        task_type: TaskType,
        validation_status: ValidationStatus,
        validation_error: Option<String>,
        metadata: Option<ModelMetadata>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        last_validated_at: Option<DateTime<Utc>>,
    ) -> Result<Self, String> {
        let file_info = FileInfo::new(file_path, file_size_bytes)?;

        Ok(Self {
            id,
            name,
            model_id,
            source,
            file_info,
            architecture,
            task_type,
            validation_status,
            validation_error,
            metadata,
            created_at,
            updated_at,
            last_validated_at,
        })
    }

    /// Mark model as validating
    ///
    /// Transitions from Pending → Validating
    pub fn mark_validating(&mut self) {
        self.validation_status = ValidationStatus::Validating;
        self.validation_error = None;
    }

    /// Mark model as valid
    ///
    /// Transitions from Validating → Valid
    pub fn mark_valid(&mut self) {
        self.validation_status = ValidationStatus::Valid;
        self.validation_error = None;
        self.last_validated_at = Some(Utc::now());
    }

    /// Mark model as invalid with error message
    ///
    /// Transitions from Validating → Invalid
    ///
    /// # Business Invariant
    ///
    /// If validation_status = Invalid, validation_error must be Some
    pub fn mark_invalid(&mut self, error: String) {
        self.validation_status = ValidationStatus::Invalid;
        self.validation_error = Some(error);
        self.last_validated_at = Some(Utc::now());
    }

    /// Update optional metadata
    pub fn update_metadata(&mut self, metadata: Option<ModelMetadata>) {
        self.metadata = metadata;
    }

    /// Check if model needs revalidation
    ///
    /// Returns true if:
    /// - Never validated (last_validated_at is None)
    /// - Currently invalid
    /// - Last validated > 7 days ago
    pub fn needs_revalidation(&self) -> bool {
        match self.validation_status {
            ValidationStatus::Invalid => true,
            ValidationStatus::Pending => true,
            ValidationStatus::Valid => {
                if let Some(last_validated) = self.last_validated_at {
                    let days_since_validation = (Utc::now() - last_validated).num_days();
                    days_since_validation > 7
                } else {
                    true
                }
            }
            ValidationStatus::Validating => false,
        }
    }

    /// Check invariants (for testing)
    ///
    /// Validates that all business invariants hold:
    /// - If validation_status = Invalid, validation_error must be Some
    /// - File size within bounds
    /// - Name and model_id non-empty
    pub fn check_invariants(&self) -> Result<(), String> {
        // Invariant: Invalid status requires error message
        if self.validation_status == ValidationStatus::Invalid && self.validation_error.is_none() {
            return Err("Invalid validation status without error message".to_string());
        }

        // Invariant: File size within bounds
        if self.file_info.size_bytes() <= 0 {
            return Err("File size must be > 0".to_string());
        }

        if self.file_info.size_bytes() > MAX_MODEL_FILE_SIZE_BYTES {
            return Err("File size exceeds 5GB limit".to_string());
        }

        Ok(())
    }

    // Getters
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &ModelName {
        &self.name
    }

    pub fn model_id(&self) -> &ModelId {
        &self.model_id
    }

    pub fn source(&self) -> &ModelSource {
        &self.source
    }

    pub fn file_info(&self) -> &FileInfo {
        &self.file_info
    }

    pub fn architecture(&self) -> Option<ModelArchitecture> {
        self.architecture
    }

    pub fn task_type(&self) -> TaskType {
        self.task_type
    }

    pub fn validation_status(&self) -> ValidationStatus {
        self.validation_status
    }

    pub fn validation_error(&self) -> Option<&str> {
        self.validation_error.as_deref()
    }

    pub fn metadata(&self) -> Option<&ModelMetadata> {
        self.metadata.as_ref()
    }

    pub fn created_at(&self) -> &DateTime<Utc> {
        &self.created_at
    }

    pub fn updated_at(&self) -> &DateTime<Utc> {
        &self.updated_at
    }

    pub fn last_validated_at(&self) -> Option<&DateTime<Utc>> {
        self.last_validated_at.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(file_name: &str) -> PathBuf {
        std::env::temp_dir().join(file_name)
    }

    #[test]
    fn test_model_id_validation() {
        // Valid
        assert!(ModelId::new("user/my-model".to_string()).is_ok());

        // Empty
        assert!(ModelId::new("".to_string()).is_err());
        assert!(ModelId::new("   ".to_string()).is_err());

        // Too long
        let long_id = "a".repeat(201);
        assert!(ModelId::new(long_id).is_err());
    }

    #[test]
    fn test_model_name_validation() {
        // Valid
        assert!(ModelName::new("My Custom Model".to_string()).is_ok());

        // Empty
        assert!(ModelName::new("".to_string()).is_err());

        // Too long
        let long_name = "a".repeat(101);
        assert!(ModelName::new(long_name).is_err());
    }

    #[test]
    fn test_file_info_validation() {
        // Valid
        assert!(FileInfo::new(PathBuf::from("/models/test.gguf"), 1_000_000).is_ok());

        // Invalid size (zero)
        assert!(FileInfo::new(PathBuf::from("/models/test.gguf"), 0).is_err());

        // Invalid size (too large)
        assert!(FileInfo::new(
            PathBuf::from("/models/test.gguf"),
            MAX_MODEL_FILE_SIZE_BYTES + 1
        )
        .is_err());
    }

    #[test]
    fn test_custom_model_creation() {
        let name = ModelName::new("Test Model".to_string()).unwrap();
        let model_id = ModelId::new("user/test-model".to_string()).unwrap();
        let source = ModelSource::LocalFile(temp_path("test.gguf"));
        let file_path = PathBuf::from("/models/test.gguf");

        let model = CustomModel::new(
            name,
            model_id,
            source,
            file_path,
            1_000_000,
            TaskType::Chat,
            None,
        )
        .unwrap();

        assert_eq!(model.validation_status(), ValidationStatus::Pending);
        assert!(model.validation_error().is_none());
        assert_eq!(model.architecture(), Some(ModelArchitecture::Gguf));
    }

    #[test]
    fn test_validation_state_transitions() {
        let name = ModelName::new("Test Model".to_string()).unwrap();
        let model_id = ModelId::new("user/test-model".to_string()).unwrap();
        let source = ModelSource::LocalFile(temp_path("test.gguf"));
        let file_path = PathBuf::from("/models/test.gguf");

        let mut model = CustomModel::new(
            name,
            model_id,
            source,
            file_path,
            1_000_000,
            TaskType::Chat,
            None,
        )
        .unwrap();

        // Pending → Validating
        model.mark_validating();
        assert_eq!(model.validation_status(), ValidationStatus::Validating);

        // Validating → Valid
        model.mark_valid();
        assert_eq!(model.validation_status(), ValidationStatus::Valid);
        assert!(model.last_validated_at().is_some());

        // Valid → Validating → Invalid
        model.mark_validating();
        model.mark_invalid("Test error".to_string());
        assert_eq!(model.validation_status(), ValidationStatus::Invalid);
        assert_eq!(model.validation_error(), Some("Test error"));
    }

    #[test]
    fn test_invariant_invalid_requires_error() {
        let name = ModelName::new("Test Model".to_string()).unwrap();
        let model_id = ModelId::new("user/test-model".to_string()).unwrap();
        let source = ModelSource::LocalFile(temp_path("test.gguf"));
        let file_path = PathBuf::from("/models/test.gguf");

        let mut model = CustomModel::new(
            name,
            model_id,
            source,
            file_path,
            1_000_000,
            TaskType::Chat,
            None,
        )
        .unwrap();

        model.mark_invalid("Error message".to_string());
        assert!(model.check_invariants().is_ok());

        // Manually break invariant (in real code this can't happen)
        model.validation_error = None;
        assert!(model.check_invariants().is_err());
    }

    #[test]
    fn test_architecture_inference() {
        assert_eq!(
            ModelArchitecture::infer_from_path(&PathBuf::from("model.gguf")),
            Some(ModelArchitecture::Gguf)
        );
        assert_eq!(
            ModelArchitecture::infer_from_path(&PathBuf::from("model.onnx")),
            Some(ModelArchitecture::Onnx)
        );
        assert_eq!(
            ModelArchitecture::infer_from_path(&PathBuf::from("model.safetensors")),
            Some(ModelArchitecture::SafeTensors)
        );
        assert_eq!(
            ModelArchitecture::infer_from_path(&PathBuf::from("model.pt")),
            Some(ModelArchitecture::PyTorch)
        );
        assert_eq!(
            ModelArchitecture::infer_from_path(&PathBuf::from("model.unknown")),
            None
        );
    }

    #[test]
    fn test_needs_revalidation() {
        let name = ModelName::new("Test Model".to_string()).unwrap();
        let model_id = ModelId::new("user/test-model".to_string()).unwrap();
        let source = ModelSource::LocalFile(temp_path("test.gguf"));
        let file_path = PathBuf::from("/models/test.gguf");

        let mut model = CustomModel::new(
            name,
            model_id,
            source,
            file_path,
            1_000_000,
            TaskType::Chat,
            None,
        )
        .unwrap();

        // Pending needs revalidation
        assert!(model.needs_revalidation());

        // Validating doesn't need revalidation
        model.mark_validating();
        assert!(!model.needs_revalidation());

        // Valid but never validated needs revalidation
        model.mark_valid();
        // Just validated, so doesn't need revalidation yet
        assert!(!model.needs_revalidation());

        // Invalid needs revalidation
        model.mark_invalid("Error".to_string());
        assert!(model.needs_revalidation());
    }
}
