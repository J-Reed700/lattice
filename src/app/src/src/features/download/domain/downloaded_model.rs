//! Downloaded Model Domain Entity
//!
//! Represents a model that has been downloaded to the local filesystem.
//! Tracks usage statistics and configuration for local models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};

use crate::domain::model_type_classifier::ModelTypeClassifier;

// Re-export ModelType from model_metadata for backward compatibility
pub use crate::domain::model_metadata::ModelType;

/// Domain entity for a downloaded model
///
/// This entity represents a model file that has been downloaded
/// and is available for use in the application (e.g., for chat, embeddings, OCR).
///
/// ## Business Rules
///
/// - Only one model can be active for chat at a time (enforced by database trigger)
/// - Only one model can be active for embedding at a time (enforced by database trigger)
/// - `use_count` increments each time the model is used
/// - `last_used_at` updates to current time when model is used
/// - Model metadata stored as JSON for flexibility
/// - Model type must match operation type (chat models for chat, embedding models for embeddings)
///
/// ## Invariants
///
/// - `model_id` must be unique across all downloaded models
/// - `file_path` must point to an existing file when model is created
/// - `file_size_bytes` must be > 0
/// - `use_count` must be >= 0
/// - `model_type` must match file extension (.gguf for chat, .onnx for embedding)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedModel {
    /// Unique identifier for this downloaded model record
    id: String,

    /// Human-readable name of the model
    model_name: String,

    /// Unique model identifier (e.g., "llama-3.2-1b-instruct")
    model_id: String,

    /// Absolute path to the model file on local filesystem
    file_path: PathBuf,

    /// Size of the model file in bytes
    file_size_bytes: i64,

    /// Type of model (chat or embedding)
    model_type: ModelType,

    /// Model architecture family (e.g., "llama", "mistral", "phi", "bert", "bge")
    /// Required by inference engines (Llama.cpp, ONNX Runtime) to identify model family
    architecture: String,

    /// Timestamp when model was downloaded
    downloaded_at: DateTime<Utc>,

    /// Timestamp when model was last used (None if never used)
    last_used_at: Option<DateTime<Utc>>,

    /// Number of times this model has been used
    use_count: i64,

    /// Whether this model is currently active for chat feature
    is_active_for_chat: bool,

    /// Whether this model is currently active for embedding feature
    is_active_for_embedding: bool,

    /// Additional metadata about the model (provider, version, capabilities, etc.)
    /// Stored as JSON for flexibility
    metadata: Option<JsonValue>,
}

impl DownloadedModel {
    /// Create a new downloaded model record
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for this record
    /// * `model_name` - Human-readable name
    /// * `model_id` - Unique model identifier
    /// * `file_path` - Absolute path to model file
    /// * `file_size_bytes` - Size of file in bytes
    /// * `architecture` - Model architecture family (e.g., "llama", "mistral", "phi")
    /// * `metadata` - Optional JSON metadata
    ///
    /// # Business Logic
    ///
    /// - Sets `downloaded_at` to current timestamp
    /// - Initializes `use_count` to 0
    /// - Sets `last_used_at` to None (never used yet)
    /// - Sets `is_active_for_chat` to false by default
    /// - Sets `is_active_for_embedding` to false by default
    /// - Infers `model_type` from file extension
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - `file_size_bytes` <= 0
    /// - `model_id` is empty
    /// - `model_name` is empty
    /// - `architecture` is empty
    pub fn new(
        id: String,
        model_name: String,
        model_id: String,
        file_path: PathBuf,
        file_size_bytes: i64,
        architecture: String,
        metadata: Option<JsonValue>,
    ) -> Result<Self, String> {
        // Validate inputs
        if model_id.trim().is_empty() {
            return Err("model_id cannot be empty".to_string());
        }

        if model_name.trim().is_empty() {
            return Err("model_name cannot be empty".to_string());
        }

        if architecture.trim().is_empty() {
            return Err("architecture cannot be empty".to_string());
        }

        if file_size_bytes <= 0 {
            return Err("file_size_bytes must be greater than 0".to_string());
        }

        // Classify model type using multi-strategy classifier
        let classifier = ModelTypeClassifier;
        let classification = classifier.classify(&model_name, Some(&file_path));

        tracing::info!(
            model_id = %model_id,
            model_name = %model_name,
            model_type = ?classification.model_type,
            strategy = ?classification.strategy,
            confidence = classification.confidence,
            "Model type classified"
        );

        let model_type = classification.model_type;

        Ok(Self {
            id,
            model_name,
            model_id,
            file_path,
            file_size_bytes,
            model_type,
            architecture,
            downloaded_at: Utc::now(),
            last_used_at: None,
            use_count: 0,
            is_active_for_chat: false,
            is_active_for_embedding: false,
            metadata,
        })
    }

    /// Create a DownloadedModel from database fields
    ///
    /// Used by repository when reconstructing from database.
    /// Does not perform validation since data is assumed to be valid in DB.
    #[allow(clippy::too_many_arguments)]
    pub fn from_db(
        id: String,
        model_name: String,
        model_id: String,
        file_path: PathBuf,
        file_size_bytes: i64,
        model_type: ModelType,
        architecture: String,
        downloaded_at: DateTime<Utc>,
        last_used_at: Option<DateTime<Utc>>,
        use_count: i64,
        is_active_for_chat: bool,
        is_active_for_embedding: bool,
        metadata: Option<JsonValue>,
    ) -> Self {
        Self {
            id,
            model_name,
            model_id,
            file_path,
            file_size_bytes,
            model_type,
            architecture,
            downloaded_at,
            last_used_at,
            use_count,
            is_active_for_chat,
            is_active_for_embedding,
            metadata,
        }
    }

    /// Mark this model as used
    ///
    /// Increments `use_count` and updates `last_used_at` to current time.
    ///
    /// # Business Logic
    ///
    /// - Atomically increments use count
    /// - Sets last_used_at to now
    /// - Returns self for method chaining
    pub fn mark_used(&mut self) {
        self.use_count += 1;
        self.last_used_at = Some(Utc::now());
    }

    /// Set whether this model is active for chat
    ///
    /// # Arguments
    ///
    /// * `active` - True to set as active chat model, false otherwise
    ///
    /// # Business Logic
    ///
    /// - Database trigger ensures only one model can be active for chat
    /// - This just updates the field; trigger handles deactivating others
    pub fn set_active_for_chat(&mut self, active: bool) {
        self.is_active_for_chat = active;
    }

    /// Set whether this model is active for embedding
    ///
    /// # Arguments
    ///
    /// * `active` - True to set as active embedding model, false otherwise
    ///
    /// # Business Logic
    ///
    /// - Database trigger ensures only one model can be active for embedding
    /// - This just updates the field; trigger handles deactivating others
    pub fn set_active_for_embedding(&mut self, active: bool) {
        self.is_active_for_embedding = active;
    }

    /// Serialize metadata to JSON string
    ///
    /// Returns JSON string for database storage, or None if no metadata.
    pub fn metadata_to_json(&self) -> Option<String> {
        self.metadata.as_ref().map(|m| m.to_string())
    }

    /// Deserialize metadata from JSON string
    ///
    /// # Arguments
    ///
    /// * `json` - JSON string to parse
    ///
    /// # Errors
    ///
    /// Returns error if JSON is invalid
    pub fn metadata_from_json(json: Option<&str>) -> Result<Option<JsonValue>, String> {
        match json {
            Some(s) if !s.trim().is_empty() => {
                serde_json::from_str(s).map(Some).map_err(|e| e.to_string())
            }
            _ => Ok(None),
        }
    }

    /// Validate that model type matches the intended operation
    ///
    /// # Arguments
    ///
    /// * `for_chat` - True if validating for chat operations
    ///
    /// # Returns
    ///
    /// Ok(()) if model type is compatible, Err with user-friendly message otherwise
    ///
    /// # Examples
    ///
    /// ```
    /// let chat_model = DownloadedModel::new(..., PathBuf::from("model.gguf"), ...)?;
    /// assert!(chat_model.validate_for_operation(true).is_ok());
    /// assert!(chat_model.validate_for_operation(false).is_err());
    /// ```
    pub fn validate_for_operation(&self, for_chat: bool) -> Result<(), String> {
        if for_chat {
            if !self.model_type.is_chat_compatible() {
                return Err(format!(
                    "Cannot use embedding model '{}' for chat. Please download a chat model (.gguf format).",
                    self.model_name
                ));
            }
        } else if !self.model_type.is_embedding_compatible() {
            return Err(format!(
                "Cannot use chat model '{}' for embeddings. Please download an embedding model (.onnx format).",
                self.model_name
            ));
        }
        Ok(())
    }

    /// Check if this model is a chat model
    pub fn is_chat_model(&self) -> bool {
        self.model_type.is_chat_compatible()
    }

    /// Check if this model is an embedding model
    pub fn is_embedding_model(&self) -> bool {
        self.model_type.is_embedding_compatible()
    }

    // Getters
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    pub fn file_path(&self) -> &PathBuf {
        &self.file_path
    }

    pub fn file_size_bytes(&self) -> i64 {
        self.file_size_bytes
    }

    pub fn model_type(&self) -> ModelType {
        self.model_type
    }

    pub fn architecture(&self) -> &str {
        &self.architecture
    }

    pub fn downloaded_at(&self) -> &DateTime<Utc> {
        &self.downloaded_at
    }

    pub fn last_used_at(&self) -> Option<&DateTime<Utc>> {
        self.last_used_at.as_ref()
    }

    pub fn use_count(&self) -> i64 {
        self.use_count
    }

    pub fn is_active_for_chat(&self) -> bool {
        self.is_active_for_chat
    }

    pub fn is_active_for_embedding(&self) -> bool {
        self.is_active_for_embedding
    }

    pub fn metadata(&self) -> Option<&JsonValue> {
        self.metadata.as_ref()
    }
}
